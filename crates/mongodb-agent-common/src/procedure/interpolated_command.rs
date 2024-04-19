use std::collections::BTreeMap;

use itertools::Itertools as _;
use mongodb::bson::{self, Bson};

use super::ProcedureError;

type Result<T> = std::result::Result<T, ProcedureError>;

/// Parse native procedure commands, and interpolate arguments.
pub fn interpolated_command(
    command: &bson::Document,
    arguments: &BTreeMap<String, Bson>,
) -> Result<bson::Document> {
    let bson = interpolate_helper(&command.into(), arguments)?;
    match bson {
        Bson::Document(doc) => Ok(doc),
        _ => unreachable!("interpolated_command is guaranteed to produce a document"),
    }
}

fn interpolate_helper(command_node: &Bson, arguments: &BTreeMap<String, Bson>) -> Result<Bson> {
    let result = match command_node {
        Bson::Array(values) => interpolate_array(values.to_vec(), arguments)?.into(),
        Bson::Document(doc) => interpolate_document(doc.clone(), arguments)?.into(),
        Bson::String(string) => interpolate_string(string, arguments)?,
        // TODO: Support interpolation within other scalar types
        value => value.clone(),
    };
    Ok(result)
}

fn interpolate_array(values: Vec<Bson>, arguments: &BTreeMap<String, Bson>) -> Result<Vec<Bson>> {
    values
        .iter()
        .map(|value| interpolate_helper(value, arguments))
        .try_collect()
}

fn interpolate_document(
    document: bson::Document,
    arguments: &BTreeMap<String, Bson>,
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
fn interpolate_string(string: &str, arguments: &BTreeMap<String, Bson>) -> Result<Bson> {
    let parts = parse_native_procedure(string);
    if parts.len() == 1 {
        let mut parts = parts;
        match parts.remove(0) {
            NativeProcedurePart::Text(string) => Ok(Bson::String(string)),
            NativeProcedurePart::Parameter(param) => resolve_argument(&param, arguments),
        }
    } else {
        let interpolated_parts: Vec<String> = parts
            .into_iter()
            .map(|part| match part {
                NativeProcedurePart::Text(string) => Ok(string),
                NativeProcedurePart::Parameter(param) => {
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

fn resolve_argument(argument_name: &str, arguments: &BTreeMap<String, Bson>) -> Result<Bson> {
    let argument = arguments
        .get(argument_name)
        .ok_or_else(|| ProcedureError::MissingArgument(argument_name.to_owned()))?;
    Ok(argument.clone())
}

/// A part of a Native Procedure command text, either raw text or a parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
enum NativeProcedurePart {
    /// A raw text part
    Text(String),
    /// A parameter
    Parameter(String),
}

/// Parse a string or key in a native procedure into parts where variables have the syntax
/// `{{<variable>}}`.
fn parse_native_procedure(string: &str) -> Vec<NativeProcedurePart> {
    let vec: Vec<Vec<NativeProcedurePart>> = string
        .split("{{")
        .filter(|part| !part.is_empty())
        .map(|part| match part.split_once("}}") {
            None => vec![NativeProcedurePart::Text(part.to_string())],
            Some((var, text)) => {
                if text.is_empty() {
                    vec![NativeProcedurePart::Parameter(var.trim().to_owned())]
                } else {
                    vec![
                        NativeProcedurePart::Parameter(var.trim().to_owned()),
                        NativeProcedurePart::Text(text.to_string()),
                    ]
                }
            }
        })
        .collect();
    vec.concat()
}

#[cfg(test)]
mod tests {
    use configuration::{
        native_procedure::NativeProcedure,
        schema::{ObjectField, ObjectType, Type},
    };
    use mongodb::bson::doc;
    use mongodb_support::BsonScalarType as S;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use crate::query::arguments::resolve_arguments;

    use super::*;

    // TODO: key
    // TODO: key with multiple placeholders

    #[test]
    fn interpolates_non_string_type() -> anyhow::Result<()> {
        let native_procedure = NativeProcedure {
            result_type: Type::Object("InsertArtist".to_owned()),
            arguments: [
                (
                    "id".to_owned(),
                    ObjectField {
                        r#type: Type::Scalar(S::Int),
                        description: Default::default(),
                    },
                ),
                (
                    "name".to_owned(),
                    ObjectField {
                        r#type: Type::Scalar(S::String),
                        description: Default::default(),
                    },
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
            ("id".to_owned(), json!(1001)),
            ("name".to_owned(), json!("Regina Spektor")),
        ]
        .into_iter()
        .collect();

        let arguments = resolve_arguments(
            &Default::default(),
            &native_procedure.arguments,
            input_arguments,
        )?;
        let command = interpolated_command(&native_procedure.command, &arguments)?;

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
        let native_procedure = NativeProcedure {
            result_type: Type::Object("InsertArtist".to_owned()),
            arguments: [(
                "documents".to_owned(),
                ObjectField {
                    r#type: Type::ArrayOf(Box::new(Type::Object("ArtistInput".to_owned()))),
                    description: Default::default(),
                },
            )]
            .into(),
            command: doc! {
                "insert": "Artist",
                "documents": "{{ documents }}",
            },
            selection_criteria: Default::default(),
            description: Default::default(),
        };

        let object_types = [(
            "ArtistInput".to_owned(),
            ObjectType {
                fields: [
                    (
                        "ArtistId".to_owned(),
                        ObjectField {
                            r#type: Type::Scalar(S::Int),
                            description: Default::default(),
                        },
                    ),
                    (
                        "Name".to_owned(),
                        ObjectField {
                            r#type: Type::Scalar(S::String),
                            description: Default::default(),
                        },
                    ),
                ]
                .into(),
                description: Default::default(),
            },
        )]
        .into();

        let input_arguments = [(
            "documents".to_owned(),
            json!([
                { "ArtistId": 1001, "Name": "Regina Spektor" } ,
                { "ArtistId": 1002, "Name": "Ok Go" } ,
            ]),
        )]
        .into_iter()
        .collect();

        let arguments =
            resolve_arguments(&object_types, &native_procedure.arguments, input_arguments)?;
        let command = interpolated_command(&native_procedure.command, &arguments)?;

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
        let native_procedure = NativeProcedure {
            result_type: Type::Object("Insert".to_owned()),
            arguments: [
                (
                    "prefix".to_owned(),
                    ObjectField {
                        r#type: Type::Scalar(S::String),
                        description: Default::default(),
                    },
                ),
                (
                    "basename".to_owned(),
                    ObjectField {
                        r#type: Type::Scalar(S::String),
                        description: Default::default(),
                    },
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
            ("prefix".to_owned(), json!("current")),
            ("basename".to_owned(), json!("some-coll")),
        ]
        .into_iter()
        .collect();

        let arguments = resolve_arguments(
            &Default::default(),
            &native_procedure.arguments,
            input_arguments,
        )?;
        let command = interpolated_command(&native_procedure.command, &arguments)?;

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
