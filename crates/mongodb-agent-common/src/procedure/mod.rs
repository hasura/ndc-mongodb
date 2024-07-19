mod error;
mod interpolated_command;

use std::borrow::Cow;
use std::collections::BTreeMap;

use configuration::native_mutation::NativeMutation;
use itertools::Itertools as _;
use mongodb::bson::Bson;
use mongodb::options::SelectionCriteria;
use mongodb::{bson, Database};
use ndc_models::ArgumentName;

use crate::mongo_query_plan::{MutationProcedureArgument, Type};
use crate::query::make_selector;
use crate::query::serialization::json_to_bson;

pub use self::error::ProcedureError;
pub use self::interpolated_command::interpolated_command;

/// Encapsulates running arbitrary mongodb commands with interpolated arguments
#[derive(Clone, Debug)]
pub struct Procedure<'a> {
    arguments: BTreeMap<ndc_models::ArgumentName, MutationProcedureArgument>,
    command: Cow<'a, bson::Document>,
    result_type: Type,
    selection_criteria: Option<Cow<'a, SelectionCriteria>>,
}

impl<'a> Procedure<'a> {
    pub fn from_native_mutation(
        native_mutation: &'a NativeMutation,
        arguments: BTreeMap<ndc_models::ArgumentName, MutationProcedureArgument>,
    ) -> Self {
        Procedure {
            arguments,
            command: Cow::Borrowed(&native_mutation.command),
            result_type: native_mutation.result_type.clone(),
            selection_criteria: native_mutation
                .selection_criteria
                .as_ref()
                .map(Cow::Borrowed),
        }
    }

    pub async fn execute(
        self,
        database: Database,
    ) -> Result<(bson::Document, Type), ProcedureError> {
        let selection_criteria = self.selection_criteria.map(Cow::into_owned);
        let command = interpolate(self.arguments, &self.command)?;
        let result = database.run_command(command, selection_criteria).await?;
        Ok((result, self.result_type))
    }

    pub fn interpolated_command(self) -> Result<bson::Document, ProcedureError> {
        interpolate(self.arguments, &self.command)
    }
}

fn interpolate(
    arguments: BTreeMap<ndc_models::ArgumentName, MutationProcedureArgument>,
    command: &bson::Document,
) -> Result<bson::Document, ProcedureError> {
    let bson_arguments = arguments
        .into_iter()
        .map(|(name, argument)| {
            let bson = argument_to_mongodb_expression(&name, argument)?;
            Ok((name, bson)) as Result<_, ProcedureError>
        })
        .try_collect()?;
    interpolated_command(command, &bson_arguments)
}

fn argument_to_mongodb_expression(
    name: &ArgumentName,
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
