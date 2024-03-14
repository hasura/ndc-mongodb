use mongodb::{bson, options::SelectionCriteria};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::schema::ObjectField;

/// An arbitrary database command using MongoDB's runCommand API.
/// See https://www.mongodb.com/docs/manual/reference/method/db.runCommand/
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeQuery {
    /// Name that will be used to identify the query in your data graph
    pub name: String,

    /// The name of an object type that specifies the type of data returned from the query. This
    /// must correspond to a configuration definition in `schema.objectTypes`.
    pub result_type: String,

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
}

type Object = serde_json::Map<String, serde_json::Value>;
type OptionalObject = Option<Object>;
