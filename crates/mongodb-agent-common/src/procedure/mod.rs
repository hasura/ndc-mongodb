mod error;
mod interpolated_command;

use std::borrow::Cow;
use std::collections::BTreeMap;

use configuration::native_mutation::NativeMutation;
use mongodb::options::SelectionCriteria;
use mongodb::{bson, Database};
use ndc_models::Argument;

use crate::mongo_query_plan::Type;
use crate::query::arguments::resolve_arguments;

pub use self::error::ProcedureError;
pub use self::interpolated_command::interpolated_command;

/// Encapsulates running arbitrary mongodb commands with interpolated arguments
#[derive(Clone, Debug)]
pub struct Procedure<'a> {
    arguments: BTreeMap<String, serde_json::Value>,
    command: Cow<'a, bson::Document>,
    parameters: Cow<'a, BTreeMap<String, Type>>,
    result_type: Type,
    selection_criteria: Option<Cow<'a, SelectionCriteria>>,
}

impl<'a> Procedure<'a> {
    pub fn from_native_mutation(
        native_mutation: &'a NativeMutation,
        arguments: BTreeMap<String, serde_json::Value>,
    ) -> Self {
        Procedure {
            arguments,
            command: Cow::Borrowed(&native_mutation.command),
            parameters: Cow::Borrowed(&native_mutation.arguments),
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
        let command = interpolate(&self.parameters, self.arguments, &self.command)?;
        let result = database.run_command(command, selection_criteria).await?;
        Ok((result, self.result_type))
    }

    pub fn interpolated_command(self) -> Result<bson::Document, ProcedureError> {
        interpolate(&self.parameters, self.arguments, &self.command)
    }
}

fn interpolate(
    parameters: &BTreeMap<String, Type>,
    arguments: BTreeMap<String, serde_json::Value>,
    command: &bson::Document,
) -> Result<bson::Document, ProcedureError> {
    let arguments = arguments
        .into_iter()
        .map(|(name, value)| (name, Argument::Literal { value }))
        .collect();
    let bson_arguments = resolve_arguments(parameters, arguments)?;
    interpolated_command(command, &bson_arguments)
}
