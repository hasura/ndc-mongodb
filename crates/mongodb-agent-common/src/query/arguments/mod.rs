mod json_to_bson;

use std::collections::BTreeMap;

use configuration::schema::{ObjectField, ObjectType, Type};
use indent::indent_all_by;
use itertools::Itertools as _;
use mongodb::bson::Bson;
use serde_json::Value;
use thiserror::Error;

use self::json_to_bson::json_to_bson;

pub use self::json_to_bson::{json_to_bson_scalar, JsonToBsonError};

#[derive(Debug, Error)]
pub enum ArgumentError {
    #[error("unknown variables or arguments: {}", .0.join(", "))]
    Excess(Vec<String>),

    #[error("some variables or arguments are invalid:\n{}", format_errors(.0))]
    Invalid(BTreeMap<String, JsonToBsonError>),

    #[error("missing variables or arguments: {}", .0.join(", "))]
    Missing(Vec<String>),
}

/// Translate arguments to queries or native queries to BSON according to declared parameter types.
///
/// Checks that all arguments have been provided, and that no arguments have been given that do not
/// map to declared paremeters (no excess arguments).
pub fn resolve_arguments(
    object_types: &BTreeMap<String, ObjectType>,
    parameters: &BTreeMap<String, ObjectField>,
    mut arguments: BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Bson>, ArgumentError> {
    validate_no_excess_arguments(parameters, &arguments)?;

    let (arguments, missing): (Vec<(String, Value, &Type)>, Vec<String>) = parameters
        .iter()
        .map(|(name, parameter)| {
            if let Some((name, argument)) = arguments.remove_entry(name) {
                Ok((name, argument, &parameter.r#type))
            } else {
                Err(name.clone())
            }
        })
        .partition_result();
    if !missing.is_empty() {
        return Err(ArgumentError::Missing(missing));
    }

    let (resolved, errors): (BTreeMap<String, Bson>, BTreeMap<String, JsonToBsonError>) = arguments
        .into_iter()
        .map(|(name, argument, parameter_type)| {
            match json_to_bson(parameter_type, object_types, argument) {
                Ok(bson) => Ok((name, bson)),
                Err(err) => Err((name, err)),
            }
        })
        .partition_result();
    if !errors.is_empty() {
        return Err(ArgumentError::Invalid(errors));
    }

    Ok(resolved)
}

pub fn validate_no_excess_arguments(
    parameters: &BTreeMap<String, ObjectField>,
    arguments: &BTreeMap<String, Value>,
) -> Result<(), ArgumentError> {
    let excess: Vec<String> = arguments
        .iter()
        .filter_map(|(name, _)| {
            let parameter = parameters.get(name);
            match parameter {
                Some(_) => None,
                None => Some(name.clone()),
            }
        })
        .collect();
    if !excess.is_empty() {
        Err(ArgumentError::Excess(excess))
    } else {
        Ok(())
    }
}

fn format_errors(errors: &BTreeMap<String, JsonToBsonError>) -> String {
    errors
        .iter()
        .map(|(name, error)| format!("  {name}:\n{}", indent_all_by(4, error.to_string())))
        .collect::<Vec<_>>()
        .join("\n")
}
