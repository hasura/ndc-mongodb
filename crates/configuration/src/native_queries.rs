use std::collections::BTreeMap;

use mongodb::{bson, options::SelectionCriteria};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::schema::{ObjectField, ObjectType, Type};

/// An arbitrary database command using MongoDB's runCommand API.
/// See https://www.mongodb.com/docs/manual/reference/method/db.runCommand/
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeQuery {
    /// You may define object types here to reference in `result_type`. Any types defined here will
    /// be merged with the definitions in `schema.json`. This allows you to maintain hand-written
    /// types for native queries without having to edit a generated `schema.json` file.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub object_types: BTreeMap<String, ObjectType>,

    /// Type of data returned by the query. You may reference object types defined in the
    /// `object_types` list in this definition, or you may reference object types from
    /// `schema.json`.
    pub result_type: Type,

    /// Arguments for per-query customization
    #[serde(default)]
    pub arguments: BTreeMap<String, ObjectField>,

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

    /// Set to `readWrite` if this native query might modify data in the database. When refreshing
    /// a dataconnector native queries will appear in the corresponding `DataConnectorLink`
    /// definition as `functions` if they are read-only, or as `procedures` if they are read-write.
    /// Functions are intended to map to GraphQL Query fields, while procedures map to Mutation
    /// fields.
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
