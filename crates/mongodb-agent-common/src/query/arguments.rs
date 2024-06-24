use std::collections::BTreeMap;

use indent::indent_all_by;
use itertools::Itertools as _;
use mongodb::bson::Bson;
use ndc_models::Argument;
use thiserror::Error;

use crate::mongo_query_plan::Type;

use super::{
    query_variable_name::query_variable_name,
    serialization::{json_to_bson, JsonToBsonError},
};

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
/// map to declared parameters (no excess arguments).
pub fn resolve_arguments(
    parameters: &BTreeMap<String, Type>,
    mut arguments: BTreeMap<String, Argument>,
) -> Result<BTreeMap<String, Bson>, ArgumentError> {
    validate_no_excess_arguments(parameters, &arguments)?;

    let (arguments, missing): (Vec<(String, Argument, &Type)>, Vec<String>) = parameters
        .iter()
        .map(|(name, parameter_type)| {
            if let Some((name, argument)) = arguments.remove_entry(name) {
                Ok((name, argument, parameter_type))
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
            match argument_to_mongodb_expression(&argument, parameter_type) {
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

fn argument_to_mongodb_expression(
    argument: &Argument,
    parameter_type: &Type,
) -> Result<Bson, JsonToBsonError> {
    match argument {
        Argument::Variable { name } => {
            let mongodb_var_name = query_variable_name(name, parameter_type);
            Ok(format!("$${mongodb_var_name}").into())
        }
        Argument::Literal { value } => json_to_bson(parameter_type, value.clone()),
    }
}

pub fn validate_no_excess_arguments<T>(
    parameters: &BTreeMap<String, Type>,
    arguments: &BTreeMap<String, T>,
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
