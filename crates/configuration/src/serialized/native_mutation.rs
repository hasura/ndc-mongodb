use std::collections::BTreeMap;

use mongodb::{bson, options::SelectionCriteria};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::schema::{ObjectField, ObjectType, Type};

/// An arbitrary database command using MongoDB's runCommand API.
/// See https://www.mongodb.com/docs/manual/reference/method/db.runCommand/
///
/// Native Procedures appear as "procedures" in your data graph.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeMutation {
    /// You may define object types here to reference in `result_type`. Any types defined here will
    /// be merged with the definitions in `schema.json`. This allows you to maintain hand-written
    /// types for native mutations without having to edit a generated `schema.json` file.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub object_types: BTreeMap<ndc_models::ObjectTypeName, ObjectType>,

    /// Type of data returned by the mutation. You may reference object types defined in the
    /// `object_types` list in this definition, or you may reference object types from
    /// `schema.json`.
    pub result_type: Type,

    /// Arguments to be supplied for each mutation invocation. These will be substituted into the
    /// given `command`.
    ///
    /// Argument values are standard JSON mapped from GraphQL input types, not Extended JSON.
    /// Values will be converted to BSON according to the types specified here.
    #[serde(default)]
    pub arguments: BTreeMap<ndc_models::ArgumentName, ObjectField>,

    /// Command to run via MongoDB's `runCommand` API. For details on how to write commands see
    /// https://www.mongodb.com/docs/manual/reference/method/db.runCommand/
    ///
    /// The command is read as Extended JSON. It may be in canonical or relaxed format, or
    /// a mixture of both.
    /// See https://www.mongodb.com/docs/manual/reference/mongodb-extended-json/
    ///
    /// Keys and values in the command may contain placeholders of the form `{{variableName}}`
    /// which will be substituted when the native mutation is executed according to the given
    /// arguments.
    ///
    /// Placeholders must be inside quotes so that the command can be stored in JSON format. If the
    /// command includes a string whose only content is a placeholder, when the variable is
    /// substituted the string will be replaced by the type of the variable. For example in this
    /// command,
    ///
    /// ```json
    /// json!({
    ///   "insert": "posts",
    ///   "documents": "{{ documents }}"
    /// })
    /// ```
    ///
    /// If the type of the `documents` argument is an array then after variable substitution the
    /// command will expand to:
    ///
    /// ```json
    /// json!({
    ///   "insert": "posts",
    ///   "documents": [/* array of documents */]
    /// })
    /// ```
    ///
    #[schemars(with = "Object")]
    pub command: bson::Document,
    // TODO: test extjson deserialization
    /// Determines which servers in a cluster to read from by specifying read preference, or
    /// a predicate to apply to candidate servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "OptionalObject")]
    pub selection_criteria: Option<SelectionCriteria>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

type Object = serde_json::Map<String, serde_json::Value>;
type OptionalObject = Option<Object>;
