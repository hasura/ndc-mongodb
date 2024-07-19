use std::collections::BTreeMap;

use itertools::Itertools as _;
use mongodb::bson::Bson;
use ndc_models as ndc;

use crate::{
    mongo_query_plan::MutationProcedureArgument,
    query::{make_selector, serialization::json_to_bson},
};

use super::ProcedureError;

pub fn arguments_to_mongodb_expressions(
    arguments: BTreeMap<ndc::ArgumentName, MutationProcedureArgument>,
) -> Result<BTreeMap<ndc::ArgumentName, Bson>, ProcedureError> {
    arguments
        .into_iter()
        .map(|(name, argument)| {
            let bson = argument_to_mongodb_expression(&name, argument)?;
            Ok((name, bson)) as Result<_, ProcedureError>
        })
        .try_collect()
}

fn argument_to_mongodb_expression(
    name: &ndc::ArgumentName,
    argument: MutationProcedureArgument,
) -> Result<Bson, ProcedureError> {
    let bson = match argument {
        MutationProcedureArgument::Literal {
            value,
            argument_type,
        } => json_to_bson(&argument_type, value).map_err(|error| {
            ProcedureError::ErrorParsingArgument {
                argument_name: name.to_string(),
                error,
            }
        })?,
        MutationProcedureArgument::Predicate { expression } => make_selector(&expression)
            .map_err(|error| ProcedureError::ErrorParsingPredicate {
                argument_name: name.to_string(),
                error: Box::new(error),
            })?
            .into(),
    };
    Ok(bson)
}
