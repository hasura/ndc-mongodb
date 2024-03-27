mod error;
mod interpolated_command;

use std::borrow::Cow;
use std::collections::BTreeMap;

use configuration::native_queries::NativeQuery;
use configuration::schema::{ObjectField, ObjectType};
use mongodb::options::SelectionCriteria;
use mongodb::{bson, Database};

use crate::query::arguments::resolve_arguments;

pub use self::error::ProcedureError;
pub use self::interpolated_command::interpolated_command;

/// Encapsulates running arbitrary mongodb commands with interpolated arguments
#[derive(Clone, Debug)]
pub struct Procedure<'a> {
    arguments: BTreeMap<String, serde_json::Value>,
    command: Cow<'a, bson::Document>,
    object_types: Cow<'a, BTreeMap<String, ObjectType>>,
    parameters: Cow<'a, BTreeMap<String, ObjectField>>,
    selection_criteria: Option<Cow<'a, SelectionCriteria>>,
}

impl<'a> Procedure<'a> {
    /// Note: the `object_types` argument here is not the object types from the native query - it
    /// should be the set of *all* object types collected from schema and native query definitions.
    pub fn from_native_query(
        native_query: &'a NativeQuery,
        object_types: &'a BTreeMap<String, ObjectType>,
        arguments: BTreeMap<String, serde_json::Value>,
    ) -> Self {
        Procedure {
            arguments,
            command: Cow::Borrowed(&native_query.command),
            object_types: Cow::Borrowed(object_types),
            parameters: Cow::Borrowed(&native_query.arguments),
            selection_criteria: native_query.selection_criteria.as_ref().map(Cow::Borrowed),
        }
    }

    pub async fn execute(self, database: Database) -> Result<bson::Document, ProcedureError> {
        let selection_criteria = self.selection_criteria.map(Cow::into_owned);
        let command = interpolate(
            &self.object_types,
            &self.parameters,
            self.arguments,
            &self.command,
        )?;
        let result = database.run_command(command, selection_criteria).await?;
        Ok(result)
    }

    pub fn interpolated_command(self) -> Result<bson::Document, ProcedureError> {
        interpolate(
            &self.object_types,
            &self.parameters,
            self.arguments,
            &self.command,
        )
    }
}

fn interpolate(
    object_types: &BTreeMap<String, ObjectType>,
    parameters: &BTreeMap<String, ObjectField>,
    arguments: BTreeMap<String, serde_json::Value>,
    command: &bson::Document,
) -> Result<bson::Document, ProcedureError> {
    let bson_arguments = resolve_arguments(object_types, parameters, arguments)?;
    interpolated_command(command, &bson_arguments)
}
