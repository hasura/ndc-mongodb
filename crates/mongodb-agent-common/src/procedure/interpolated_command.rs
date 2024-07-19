use std::collections::BTreeMap;

use itertools::Itertools as _;
use mongodb::bson::{self, Bson};

use super::ProcedureError;

type Result<T> = std::result::Result<T, ProcedureError>;

/// Parse native mutation commands, and interpolate arguments.
pub fn interpolated_command(
    command: &bson::Document,
    arguments: &BTreeMap<ndc_models::ArgumentName, Bson>,
) -> Result<bson::Document> {
    let bson = interpolate_helper(&command.into(), arguments)?;
    match bson {
        Bson::Document(doc) => Ok(doc),
        _ => unreachable!("interpolated_command is guaranteed to produce a document"),
    }
}

fn interpolate_helper(
    command_node: &Bson,
    arguments: &BTreeMap<ndc_models::ArgumentName, Bson>,
) -> Result<Bson> {
    let result = match command_node {
        Bson::Array(values) => interpolate_array(values.to_vec(), arguments)?.into(),
        Bson::Document(doc) => interpolate_document(doc.clone(), arguments)?.into(),
        Bson::String(string) => interpolate_string(string, arguments)?,
        // TODO: Support interpolation within other scalar types
        value => value.clone(),
    };
    Ok(result)
}

fn interpolate_array(
    values: Vec<Bson>,
    arguments: &BTreeMap<ndc_models::ArgumentName, Bson>,
) -> Result<Vec<Bson>> {
    values
        .iter()
        .map(|value| interpolate_helper(value, arguments))
        .try_collect()
}

fn interpolate_document(
    document: bson::Document,
    arguments: &BTreeMap<ndc_models::ArgumentName, Bson>,
) -> Result<bson::Document> {
    document
        .into_iter()
        .map(|(key, value)| {
            let interpolated_value = interpolate_helper(&value, arguments)?;
            let interpolated_key = interpolate_string(&key, arguments)?;
            match interpolated_key {
                Bson::String(string_key) => Ok((string_key, interpolated_value)),
                _ => Err(ProcedureError::NonStringKey(interpolated_key)),
            }
        })
        .try_collect()
}

/// Substitute placeholders within a string in the input template. This may produce an output that
/// is not a string if the entire content of the string is a placeholder. For example,
///
/// ```json
/// { "key": "{{recordId}}" }
/// ```
///
/// might expand to,
///
/// ```json
/// { "key": 42 }
/// ```
///
/// if the type of the variable `recordId` is `int`.
fn interpolate_string(
    string: &str,
    arguments: &BTreeMap<ndc_models::ArgumentName, Bson>,
) -> Result<Bson> {
    let parts = parse_native_mutation(string);
    if parts.len() == 1 {
        let mut parts = parts;
        match parts.remove(0) {
            NativeMutationPart::Text(string) => Ok(Bson::String(string)),
            NativeMutationPart::Parameter(param) => resolve_argument(&param, arguments),
        }
    } else {
        let interpolated_parts: Vec<String> = parts
            .into_iter()
            .map(|part| match part {
                NativeMutationPart::Text(string) => Ok(string),
                NativeMutationPart::Parameter(param) => {
                    let argument_value = resolve_argument(&param, arguments)?;
                    match argument_value {
                        Bson::String(string) => Ok(string),
                        _ => Err(ProcedureError::NonStringInStringContext(param)),
                    }
                }
            })
            .try_collect()?;
        Ok(Bson::String(interpolated_parts.join("")))
    }
}

fn resolve_argument(
    argument_name: &ndc_models::ArgumentName,
    arguments: &BTreeMap<ndc_models::ArgumentName, Bson>,
) -> Result<Bson> {
    let argument = arguments
        .get(argument_name)
        .ok_or_else(|| ProcedureError::MissingArgument(argument_name.to_owned()))?;
    Ok(argument.clone())
}

/// A part of a Native Mutation command text, either raw text or a parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
enum NativeMutationPart {
    /// A raw text part
    Text(String),
    /// A parameter
    Parameter(ndc_models::ArgumentName),
}

/// Parse a string or key in a native procedure into parts where variables have the syntax
/// `{{<variable>}}`.
fn parse_native_mutation(string: &str) -> Vec<NativeMutationPart> {
    let vec: Vec<Vec<NativeMutationPart>> = string
        .split("{{")
        .filter(|part| !part.is_empty())
        .map(|part| match part.split_once("}}") {
            None => vec![NativeMutationPart::Text(part.to_string())],
            Some((var, text)) => {
                if text.is_empty() {
                    vec![NativeMutationPart::Parameter(var.trim().into())]
                } else {
                    vec![
                        NativeMutationPart::Parameter(var.trim().into()),
                        NativeMutationPart::Text(text.to_string()),
                    ]
                }
            }
        })
        .collect();
    vec.concat()
}

#[cfg(test)]
mod tests {
    use configuration::{native_mutation::NativeMutation, MongoScalarType};
    use mongodb::bson::doc;
    use mongodb_support::BsonScalarType as S;
    use ndc_query_plan::MutationProcedureArgument;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use crate::{
        mongo_query_plan::{ObjectType, Type},
        procedure::arguments_to_mongodb_expressions::arguments_to_mongodb_expressions,
    };

    use super::*;

    #[test]
    fn interpolates_non_string_type() -> anyhow::Result<()> {
        let native_mutation = NativeMutation {
            result_type: Type::Object(ObjectType {
                name: Some("InsertArtist".into()),
                fields: [("ok".into(), Type::Scalar(MongoScalarType::Bson(S::Bool)))].into(),
            }),
            arguments: [
                ("id".into(), Type::Scalar(MongoScalarType::Bson(S::Int))),
                (
                    "name".into(),
                    Type::Scalar(MongoScalarType::Bson(S::String)),
                ),
            ]
            .into(),
            command: doc! {
                "insert": "Artist",
                "documents": [{
                    "ArtistId": "{{ id }}",
                    "Name": "{{name }}",
                }],
            },
            selection_criteria: Default::default(),
            description: Default::default(),
        };

        let input_arguments = [
            (
                "id".into(),
                MutationProcedureArgument::Literal {
                    value: json!(1001),
                    argument_type: Type::Scalar(MongoScalarType::Bson(S::Int)),
                },
            ),
            (
                "name".into(),
                MutationProcedureArgument::Literal {
                    value: json!("Regina Spektor"),
                    argument_type: Type::Scalar(MongoScalarType::Bson(S::String)),
                },
            ),
        ]
        .into();

        let arguments = arguments_to_mongodb_expressions(input_arguments)?;
        let command = interpolated_command(&native_mutation.command, &arguments)?;

        assert_eq!(
            command,
            bson::doc! {
                "insert": "Artist",
                "documents": [{
                    "ArtistId": 1001,
                    "Name": "Regina Spektor",
                }],
            }
        );
        Ok(())
    }

    #[test]
    fn interpolates_array_argument() -> anyhow::Result<()> {
        let documents_type = Type::ArrayOf(Box::new(Type::Object(ObjectType {
            name: Some("ArtistInput".into()),
            fields: [
                (
                    "ArtistId".into(),
                    Type::Scalar(MongoScalarType::Bson(S::Int)),
                ),
                (
                    "Name".into(),
                    Type::Scalar(MongoScalarType::Bson(S::String)),
                ),
            ]
            .into(),
        })));

        let native_mutation = NativeMutation {
            result_type: Type::Object(ObjectType {
                name: Some("InsertArtist".into()),
                fields: [("ok".into(), Type::Scalar(MongoScalarType::Bson(S::Bool)))].into(),
            }),
            arguments: [("documents".into(), documents_type.clone())].into(),
            command: doc! {
                "insert": "Artist",
                "documents": "{{ documents }}",
            },
            selection_criteria: Default::default(),
            description: Default::default(),
        };

        let input_arguments = [(
            "documents".into(),
            MutationProcedureArgument::Literal {
                value: json!([
                    { "ArtistId": 1001, "Name": "Regina Spektor" } ,
                    { "ArtistId": 1002, "Name": "Ok Go" } ,
                ]),
                argument_type: documents_type,
            },
        )]
        .into_iter()
        .collect();

        let arguments = arguments_to_mongodb_expressions(input_arguments)?;
        let command = interpolated_command(&native_mutation.command, &arguments)?;

        assert_eq!(
            command,
            bson::doc! {
                "insert": "Artist",
                "documents": [
                    {
                        "ArtistId": 1001,
                        "Name": "Regina Spektor",
                    },
                    {
                        "ArtistId": 1002,
                        "Name": "Ok Go",
                    }
                ],
            }
        );
        Ok(())
    }

    #[test]
    fn interpolates_arguments_within_string() -> anyhow::Result<()> {
        let native_mutation = NativeMutation {
            result_type: Type::Object(ObjectType {
                name: Some("Insert".into()),
                fields: [("ok".into(), Type::Scalar(MongoScalarType::Bson(S::Bool)))].into(),
            }),
            arguments: [
                (
                    "prefix".into(),
                    Type::Scalar(MongoScalarType::Bson(S::String)),
                ),
                (
                    "basename".into(),
                    Type::Scalar(MongoScalarType::Bson(S::String)),
                ),
            ]
            .into(),
            command: doc! {
                "insert": "{{prefix}}-{{basename}}",
                "empty": "",
            },
            selection_criteria: Default::default(),
            description: Default::default(),
        };

        let input_arguments = [
            (
                "prefix".into(),
                MutationProcedureArgument::Literal {
                    value: json!("current"),
                    argument_type: Type::Scalar(MongoScalarType::Bson(S::String)),
                },
            ),
            (
                "basename".into(),
                MutationProcedureArgument::Literal {
                    value: json!("some-coll"),
                    argument_type: Type::Scalar(MongoScalarType::Bson(S::String)),
                },
            ),
        ]
        .into_iter()
        .collect();

        let arguments = arguments_to_mongodb_expressions(input_arguments)?;
        let command = interpolated_command(&native_mutation.command, &arguments)?;

        assert_eq!(
            command,
            bson::doc! {
                "insert": "current-some-coll",
                "empty": "",
            }
        );
        Ok(())
    }
}
