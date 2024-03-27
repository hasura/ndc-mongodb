use std::collections::BTreeMap;

use itertools::Itertools as _;
use mongodb::bson::{self, Bson};

use super::ProcedureError;

type Result<T> = std::result::Result<T, ProcedureError>;

/// Parse native query commands, and interpolate arguments.
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
    let parts = parse_native_query(string);
    if parts.len() == 1 {
        let mut parts = parts;
        match parts.remove(0) {
            NativeQueryPart::Text(string) => Ok(Bson::String(string)),
            NativeQueryPart::Parameter(param) => resolve_argument(&param, arguments),
        }
    } else {
        let interpolated_parts: Vec<String> = parts
            .into_iter()
            .map(|part| match part {
                NativeQueryPart::Text(string) => Ok(string),
                NativeQueryPart::Parameter(param) => {
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

/// A part of a Native Query text, either raw text or a parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
enum NativeQueryPart {
    /// A raw text part
    Text(String),
    /// A parameter
    Parameter(String),
}

/// Parse a string or key in a native query into parts where variables have the syntax
/// `{{<variable>}}`.
fn parse_native_query(string: &str) -> Vec<NativeQueryPart> {
    let vec: Vec<Vec<NativeQueryPart>> = string
        .split("{{")
        .filter(|part| !part.is_empty())
        .map(|part| match part.split_once("}}") {
            None => vec![NativeQueryPart::Text(part.to_string())],
            Some((var, text)) => {
                if text.is_empty() {
                    vec![NativeQueryPart::Parameter(var.trim().to_owned())]
                } else {
                    vec![
                        NativeQueryPart::Parameter(var.trim().to_owned()),
                        NativeQueryPart::Text(text.to_string()),
                    ]
                }
            }
        })
        .collect();
    vec.concat()
}

#[cfg(test)]
mod tests {
    use configuration::native_queries::NativeQuery;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use crate::query::arguments::resolve_arguments;

    use super::*;

    // TODO: key
    // TODO: key with multiple placeholders

    #[test]
    fn interpolates_non_string_type() -> anyhow::Result<()> {
        let native_query_input = json!({
            "resultType": { "object": "InsertArtist" },
            "arguments": {
                "id": { "type": { "scalar": "int" } },
                "name": { "type": { "scalar": "string" } },
            },
            "command": {
                "insert": "Artist",
                "documents": [{
                    "ArtistId": "{{ id }}",
                    "Name": "{{name }}",
                }],
            },
        });
        let input_arguments = [
            ("id".to_owned(), json!(1001)),
            ("name".to_owned(), json!("Regina Spektor")),
        ]
        .into_iter()
        .collect();

        let native_query: NativeQuery = serde_json::from_value(native_query_input)?;
        let arguments = resolve_arguments(
            &native_query.object_types,
            &native_query.arguments,
            input_arguments,
        )?;
        let command = interpolated_command(&native_query.command, &arguments)?;

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
        let native_query_input = json!({
            "name": "insertArtist",
            "resultType": { "object": "InsertArtist" },
            "objectTypes": {
                "ArtistInput": {
                    "fields": {
                        "ArtistId": { "type": { "scalar": "int" } },
                        "Name": { "type": { "scalar": "string" } },
                    },
                }
            },
            "arguments": {
                "documents": { "type": { "arrayOf": { "object": "ArtistInput" } } },
            },
            "command": {
                "insert": "Artist",
                "documents": "{{ documents }}",
            },
        });
        let input_arguments = [(
            "documents".to_owned(),
            json!([
                { "ArtistId": 1001, "Name": "Regina Spektor" } ,
                { "ArtistId": 1002, "Name": "Ok Go" } ,
            ]),
        )]
        .into_iter()
        .collect();

        let native_query: NativeQuery = serde_json::from_value(native_query_input)?;
        let arguments = resolve_arguments(
            &native_query.object_types,
            &native_query.arguments,
            input_arguments,
        )?;
        let command = interpolated_command(&native_query.command, &arguments)?;

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
        let native_query_input = json!({
            "name": "insert",
            "resultType": { "object": "Insert" },
            "arguments": {
                "prefix": { "type": { "scalar": "string" } },
                "basename": { "type": { "scalar": "string" } },
            },
            "command": {
                "insert": "{{prefix}}-{{basename}}",
                "empty": "",
            },
        });
        let input_arguments = [
            ("prefix".to_owned(), json!("current")),
            ("basename".to_owned(), json!("some-coll")),
        ]
        .into_iter()
        .collect();

        let native_query: NativeQuery = serde_json::from_value(native_query_input)?;
        let arguments = resolve_arguments(
            &native_query.object_types,
            &native_query.arguments,
            input_arguments,
        )?;
        let command = interpolated_command(&native_query.command, &arguments)?;

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
