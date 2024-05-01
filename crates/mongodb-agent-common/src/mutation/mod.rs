mod error;
mod interpolated_command;

use std::borrow::Cow;
use std::collections::BTreeMap;

use configuration::native_mutation::NativeMutation;
use configuration::schema::{ObjectField, ObjectType, Type};
use mongodb::options::SelectionCriteria;
use mongodb::{bson, Database};

use crate::query::arguments::resolve_arguments;

pub use self::error::MutationError;
pub use self::interpolated_command::interpolated_command;

/// Encapsulates running arbitrary mongodb commands with interpolated arguments
#[derive(Clone, Debug)]
pub struct Mutation<'a> {
    arguments: BTreeMap<String, serde_json::Value>,
    command: Cow<'a, bson::Document>,
    parameters: Cow<'a, BTreeMap<String, ObjectField>>,
    result_type: Type,
    selection_criteria: Option<Cow<'a, SelectionCriteria>>,
}

impl<'a> Mutation<'a> {
    pub fn from_native_mutation(
        native_mutation: &'a NativeMutation,
        arguments: BTreeMap<String, serde_json::Value>,
    ) -> Self {
        Mutation {
            arguments,
            command: Cow::Borrowed(&native_mutation.command),
            parameters: Cow::Borrowed(&native_mutation.arguments),
            result_type: native_mutation.result_type.clone(),
            selection_criteria: native_mutation.selection_criteria.as_ref().map(Cow::Borrowed),
        }
    }

    pub async fn execute(
        self,
        object_types: &BTreeMap<String, ObjectType>,
        database: Database,
    ) -> Result<(bson::Document, Type), MutationError> {
        let selection_criteria = self.selection_criteria.map(Cow::into_owned);
        let command = interpolate(
            object_types,
            &self.parameters,
            self.arguments,
            &self.command,
        )?;
        let result = database.run_command(command, selection_criteria).await?;
        Ok((result, self.result_type))
    }

    pub fn interpolated_command(
        self,
        object_types: &BTreeMap<String, ObjectType>,
    ) -> Result<bson::Document, MutationError> {
        interpolate(
            object_types,
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
) -> Result<bson::Document, MutationError> {
    let bson_arguments = resolve_arguments(object_types, parameters, arguments)?;
    interpolated_command(command, &bson_arguments)
}
