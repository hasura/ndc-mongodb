mod arguments_to_mongodb_expressions;
mod error;
mod interpolated_command;

use std::borrow::Cow;
use std::collections::BTreeMap;

use arguments_to_mongodb_expressions::arguments_to_mongodb_expressions;
use configuration::native_mutation::NativeMutation;
use mongodb::options::SelectionCriteria;
use mongodb::{bson, Database};

use crate::mongo_query_plan::{MutationProcedureArgument, Type};

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
        let command = interpolate(self.arguments, &self.command)?;
        let run_command = database.run_command(command);
        let run_command = if let Some(selection_criteria) = self.selection_criteria {
            run_command.selection_criteria(selection_criteria.into_owned())
        } else {
            run_command
        };
        let result = run_command.await?;
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
    let bson_arguments = arguments_to_mongodb_expressions(arguments)?;
    interpolated_command(command, &bson_arguments)
}
