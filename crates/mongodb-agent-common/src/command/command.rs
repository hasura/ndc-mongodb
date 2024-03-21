use std::collections::BTreeMap;

use configuration::schema::{ObjectField, Type};
use itertools::Itertools;
use mongodb::bson::{self, Bson};
use mongodb_support::BsonScalarType;
use serde_json::Value;

use super::CommandError;

// type JsonObject = serde_json::Map<String, serde_json::Value>;

/// Parse native query commands, and interpolate variables. Input is serde_json::Value because our
/// configuration format is JSON. Output is BSON because that is the format that MongoDB commands
/// use.
pub fn interpolate(
    command: &bson::Document,
    parameters: &[ObjectField],
    arguments: &BTreeMap<String, Value>,
) -> Result<Bson, CommandError> {
    // let arguments_bson: BTreeMap<String, Bson> = arguments
    //     .iter()
    //     .map(|(key, value)| -> Result<(String, Bson), CommandError> {
    //         Ok((key.to_owned(), value.clone().try_into()?))
    //     })
    //     .try_collect()?;
    interpolate_helper(&command.into(), parameters, arguments)
}

fn interpolate_helper(
    command: &Bson,
    parameters: &[ObjectField],
    arguments: &BTreeMap<String, Value>,
) -> Result<Bson, CommandError> {
    // let result = match command {
    //     exp @ Value::Null => exp.clone(),
    //     exp @ Value::Bool(_) => exp.clone(),
    //     exp @ Value::Number(_) => exp.clone(),
    //     Value::String(string) => interpolate_string(string, parameters, arguments)?,
    //     Value::Array(_) => todo!(),
    //     Value::Object(_) => todo!(),
    // };

    let result = match command {
        Bson::Array(values) => values
            .iter()
            .map(|value| interpolate_helper(value, parameters, arguments))
            .try_collect()?,
        Bson::Document(doc) => interpolate_document(doc.clone(), parameters, arguments)?.into(),
        Bson::String(string) => interpolate_string(string, parameters, arguments)?,
        Bson::RegularExpression(_) => todo!(),
        Bson::JavaScriptCode(_) => todo!(),
        Bson::JavaScriptCodeWithScope(_) => todo!(),
        value => value.clone(),
    };

    Ok(result)
}

fn interpolate_document(
    document: bson::Document,
    parameters: &[ObjectField],
    arguments: &BTreeMap<String, Value>,
) -> Result<bson::Document, CommandError> {
    document
        .into_iter()
        .map(|(key, value)| {
            let interpolated_value = interpolate_helper(&value, parameters, arguments)?;
            let interpolated_key = interpolate_string(&key, parameters, arguments)?;
            match interpolated_key {
                Bson::String(string_key) => Ok((string_key, interpolated_value)),
                _ => Err(CommandError::NonStringKey(interpolated_key)),
            }
        })
        .try_collect()
}

/// Substitute placeholders within a string in the input template. This may produce an output that
/// is not a string if the entire content of the string is a placeholder. For example,
///
///     { "key": "{{recordId}}" }
///
/// might expand to,
///
///     { "key": 42 }
///
/// if the type of the variable `recordId` is `int`.
fn interpolate_string(
    string: &str,
    parameters: &[ObjectField],
    arguments: &BTreeMap<String, Value>,
) -> Result<Bson, CommandError> {
    let parts = parse_native_query(string);
    if parts.len() == 1 {
        let mut parts = parts;
        match parts.remove(0) {
            NativeQueryPart::Text(string) => Ok(Bson::String(string)),
            NativeQueryPart::Parameter(param) => resolve_argument(&param, parameters, arguments),
        }
    } else {
        todo!()
    }
}

/// Looks up an argument value for a given parameter, and produces a BSON value that matches the
/// declared parameter type.
fn resolve_argument(
    param_name: &str,
    parameters: &[ObjectField],
    arguments: &BTreeMap<String, Value>,
) -> Result<Bson, CommandError> {
    let parameter = parameters
        .iter()
        .find(|arg| arg.name == param_name)
        .ok_or_else(|| CommandError::UnknownParameter(param_name.to_owned()))?;
    let argument_json = arguments
        .get(param_name)
        .ok_or_else(|| CommandError::MissingArgument(param_name.to_owned()))?;
    let argument: Bson = argument_json.clone().try_into()?;
    match parameter.r#type {
        Type::Scalar(t) => resolve_scalar_argument(t, argument),
        Type::Object(_) => todo!(),
        Type::ArrayOf(_) => todo!(),
        Type::Nullable(_) => todo!(),
    }
}

fn resolve_scalar_argument(
    parameter_type: BsonScalarType,
    argument: Bson,
) -> Result<Bson, CommandError> {
    let argument_type: BsonScalarType = (&argument).try_into()?;
    if argument_type == parameter_type {
        Ok(argument)
    } else {
        Err(CommandError::TypeMismatch(argument_type, parameter_type))
    }
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

    use super::*;

    // TODO: extjson
    // TODO: nullable
    // TODO: optional

    #[test]
    fn interpolates_non_string_type() -> anyhow::Result<()> {
        let native_query_input = json!({
            "name": "insertArtist",
            "resultType": { "object": "InsertArtist" },
            "arguments": [
                { "name": "id", "type": { "scalar": "int" } },
                { "name": "name", "type": { "scalar": "string" } },
            ],
            "command": {
                "insert": "Artist",
                "documents": [{
                    "ArtistId": "{{ id }}",
                    "Name": "{{name }}",
                }],
            },
        });
        let arguments = [
            ("id".to_owned(), json!(1001)),
            ("name".to_owned(), json!("Regina Spektor")),
        ]
        .into_iter()
        .collect();

        let native_query: NativeQuery = serde_json::from_value(native_query_input)?;
        let interpolated_command = interpolate(
            &native_query.command.into(),
            &native_query.arguments,
            &arguments,
        )?;

        assert_eq!(
            interpolated_command,
            bson::doc! {
                "insert": "Artist",
                "documents": [{
                    "ArtistId": 1001,
                    "Name": "Regina Spektor",
                }],
            }
            .into()
        );
        Ok(())
    }

    #[test]
    fn interpolates_array_argument() -> anyhow::Result<()> {
        let native_query_input = json!({
            "name": "insertArtist",
            "resultType": { "object": "InsertArtist" },
            "objectTypes": [{
                "name": "ArtistInput",
                "fields": [
                    { "name": "ArtistId", "type": { "scalar": "int" } },
                    { "name": "Name", "type": { "scalar": "string" } },
                ],
            }],
            "arguments": [
                { "name": "documents", "type": { "arrayOf": { "object": "ArtistInput" } } },
            ],
            "command": {
                "insert": "Artist",
                "documents": "{{ documents }}",
            },
        });
        let arguments = [
            ("id".to_owned(), json!(1001)),
            ("name".to_owned(), json!("Regina Spektor")),
        ]
        .into_iter()
        .collect();

        let native_query: NativeQuery = serde_json::from_value(native_query_input)?;
        let interpolated_command = interpolate(
            &native_query.command.into(),
            &native_query.arguments,
            &arguments,
        )?;

        assert_eq!(
            interpolated_command,
            bson::doc! {
                "insert": "Artist",
                "documents": [
                    {
                        "ArtistId": "{{ id }}",
                        "Name": "{{name }}",
                    },
                    {
                        "ArtistId": "{{ id }}",
                        "Name": "{{name }}",
                    }
                ],
            }
            .into()
        );
        Ok(())
    }
}
