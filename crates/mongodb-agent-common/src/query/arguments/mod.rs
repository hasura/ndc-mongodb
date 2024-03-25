mod json_to_bson;

use std::collections::BTreeMap;

use configuration::schema::{ObjectField, ObjectType};
use itertools::Itertools as _;
use mongodb::bson::Bson;
use serde_json::Value;

use crate::interface_types::MongoAgentError;

use self::json_to_bson::json_to_bson;

pub use self::json_to_bson::{JsonToBsonError, json_to_bson_scalar};

/// Translate arguments to queries or native queries to BSON according to declared parameter types.
///
/// Checks that all arguments have been provided, and that no arguments have been given that do not
/// map to declared paremeters (no excess arguments).
pub fn resolve_arguments(
    object_types: &BTreeMap<String, ObjectType>,
    parameters: &BTreeMap<String, ObjectField>,
    arguments: BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Bson>, MongoAgentError> {
    validate_no_excess_arguments(parameters, &arguments)?;
    parameters
        .iter()
        .map(|(key, parameter)| {
            let argument = arguments
                .get(key)
                .ok_or_else(|| MongoAgentError::VariableNotDefined(key.to_owned()))?;
            Ok((
                key.clone(),
                json_to_bson(&parameter.r#type, object_types, argument.clone())?,
            ))
        })
        .try_collect()
}

pub fn validate_no_excess_arguments(
    parameters: &BTreeMap<String, ObjectField>,
    arguments: &BTreeMap<String, Value>,
) -> Result<(), MongoAgentError> {
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
        Err(MongoAgentError::UnknownVariables(excess))
    } else {
        Ok(())
    }
}
