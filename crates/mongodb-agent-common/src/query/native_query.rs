use std::collections::HashMap;

use configuration::native_query::NativeQuery;
use dc_api_types::{Argument, QueryRequest, VariableSet};
use itertools::Itertools as _;

use crate::{
    interface_types::MongoAgentError,
    mongodb::{Pipeline, Stage},
    procedure::{interpolated_command, ProcedureError},
};

use super::{arguments::resolve_arguments, query_target::QueryTarget, QueryConfig};

/// Returns either the pipeline defined by a native query with variable bindings for arguments, or
/// an empty pipeline if the query request target is not a native query
pub fn pipeline_for_native_query(
    config: QueryConfig<'_>,
    variables: Option<&VariableSet>,
    query_request: &QueryRequest,
) -> Result<Pipeline, MongoAgentError> {
    match QueryTarget::for_request(config, query_request) {
        QueryTarget::Collection(_) => Ok(Pipeline::empty()),
        QueryTarget::NativeQuery {
            native_query,
            arguments,
            ..
        } => make_pipeline(config, variables, native_query, arguments),
    }
}

fn make_pipeline(
    config: QueryConfig<'_>,
    variables: Option<&VariableSet>,
    native_query: &NativeQuery,
    arguments: &HashMap<String, Argument>,
) -> Result<Pipeline, MongoAgentError> {
    let expressions = arguments
        .iter()
        .map(|(name, argument)| {
            Ok((
                name.to_owned(),
                argument_to_mongodb_expression(argument, variables)?,
            )) as Result<_, MongoAgentError>
        })
        .try_collect()?;

    let bson_arguments =
        resolve_arguments(config.object_types, &native_query.arguments, expressions)
            .map_err(ProcedureError::UnresolvableArguments)?;

    // Replace argument placeholders with resolved expressions, convert document list to
    // a `Pipeline` value
    let stages = native_query
        .pipeline
        .iter()
        .map(|document| interpolated_command(document, &bson_arguments))
        .map_ok(Stage::Other)
        .try_collect()?;

    Ok(Pipeline::new(stages))
}

fn argument_to_mongodb_expression(
    argument: &Argument,
    variables: Option<&VariableSet>,
) -> Result<serde_json::Value, MongoAgentError> {
    match argument {
        Argument::Variable { name } => variables
            .and_then(|vs| vs.get(name))
            .ok_or_else(|| MongoAgentError::VariableNotDefined(name.to_owned()))
            .cloned(),
        Argument::Literal { value } => Ok(value.clone()),
        // TODO: Column references are needed for native queries that are a target of a relation.
        // MDB-106
        Argument::Column { .. } => Err(MongoAgentError::NotImplemented(
            "column references in native queries are not currently implemented",
        )),
    }
}

// TODO: test
