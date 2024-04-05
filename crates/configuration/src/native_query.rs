use std::collections::BTreeMap;

use mongodb::{bson, options::SelectionCriteria};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::schema::{ObjectField, ObjectType, Type};

/// Define an arbitrary MongoDB aggregation pipeline that can be referenced in your data graph. For
/// details on aggregation pipelines see https://www.mongodb.com/docs/manual/core/aggregation-pipeline/
///
/// Native queries will appear in your DDN as "functions".
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeQuery {
    /// You may define object types here to reference in `result_type`. Any types defined here will
    /// be merged with the definitions in `schema.json`. This allows you to maintain hand-written
    /// types for native queries without having to edit a generated `schema.json` file.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub object_types: BTreeMap<String, ObjectType>,

    /// Type of data produced by the given pipeline. You may reference object types defined in the
    /// `object_types` list in this definition, or you may reference object types from
    /// `schema.json`.
    pub result_type: Type,

    /// Arguments to be supplied for each query invocation. These will be available to the given
    /// pipeline as variables. For information about variables in MongoDB aggregation expressions
    /// see https://www.mongodb.com/docs/manual/reference/aggregation-variables/
    ///
    /// Argument values are standard JSON mapped from GraphQL input types, not Extended JSON.
    /// Values will be converted to BSON according to the types specified here.
    #[serde(default)]
    pub arguments: BTreeMap<String, ObjectField>,

    /// Pipeline to include in MongoDB queries. For details on how to write an aggregation pipeline
    /// see https://www.mongodb.com/docs/manual/core/aggregation-pipeline/
    ///
    /// The pipeline may include Extended JSON.
    ///
    /// Arguments specified for this native query will be available as variables. See
    /// https://www.mongodb.com/docs/manual/reference/aggregation-variables/
    #[schemars(with = "Vec<serde_json::Value>")]
    pub pipeline: Vec<bson::Document>,

    #[serde(default)]
    pub representation: NativeQueryRepresentation,

    /// Determines which servers in a cluster to read from by specifying read preference, or
    /// a predicate to apply to candidate servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "OptionalObject")]
    pub selection_criteria: Option<SelectionCriteria>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum NativeQueryRepresentation {
    #[default]
    Collection,
    Function,
}

type Object = serde_json::Map<String, serde_json::Value>;
type OptionalObject = Option<Object>;
