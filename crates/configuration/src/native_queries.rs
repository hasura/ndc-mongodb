use mongodb::{bson, options::SelectionCriteria};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::schema::{ObjectField, Type};

/// An arbitrary database command using MongoDB's runCommand API.
/// See https://www.mongodb.com/docs/manual/reference/method/db.runCommand/
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeQuery {
    /// Name that will be used to identify the query in your data graph
    pub name: String,

    /// Type of data returned by the query.
    pub result_type: Type,

    /// Arguments for per-query customization
    pub arguments: Vec<ObjectField>,

    /// Command to run expressed as a BSON document
    #[schemars(with = "Object")]
    pub command: bson::Document,

    /// Determines which servers in a cluster to read from by specifying read preference, or
    /// a predicate to apply to candidate servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "OptionalObject")]
    pub selection_criteria: Option<SelectionCriteria>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Set to `readWrite` if this native query might modify data in the database.
    #[serde(default)]
    pub mode: Mode,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    ReadOnly,
    ReadWrite,
}

type Object = serde_json::Map<String, serde_json::Value>;
type OptionalObject = Option<Object>;
