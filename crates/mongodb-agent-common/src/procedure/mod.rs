mod error;
mod interpolated_command;

use std::borrow::Cow;
use std::collections::BTreeMap;

use configuration::native_procedure::NativeProcedure;
use mongodb::options::SelectionCriteria;
use mongodb::{bson, Database};

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
    pub fn from_native_procedure(
        native_procedure: &'a NativeProcedure,
        arguments: BTreeMap<String, serde_json::Value>,
    ) -> Self {
        Procedure {
            arguments,
            command: Cow::Borrowed(&native_procedure.command),
            parameters: Cow::Borrowed(&native_procedure.arguments),
            result_type: native_procedure.result_type.clone(),
            selection_criteria: native_procedure
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
    let bson_arguments = resolve_arguments(parameters, arguments)?;
    interpolated_command(command, &bson_arguments)
}
