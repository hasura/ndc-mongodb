use std::collections::BTreeMap;

use mongodb::bson;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    native_query::NativeQueryRepresentation,
    schema::{ObjectField, ObjectType},
};

/// Define an arbitrary MongoDB aggregation pipeline that can be referenced in your data graph. For
/// details on aggregation pipelines see https://www.mongodb.com/docs/manual/core/aggregation-pipeline/
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NativeQuery {
    /// Representation may be either "collection" or "function". If you choose "collection" then
    /// the native query acts as a virtual collection, or in other words a view. This implies
    /// a list of documents that can be filtered and sorted using the GraphQL arguments like
    /// `where` and `limit` that are available to regular collections. (These arguments are added
    /// to your GraphQL API automatically - there is no need to list them in the `arguments` for
    /// the native query.)
    ///
    /// Choose "function" if you want to produce data that is not a list of documents, or if
    /// filtering and sorting are not sensible operations for this native query. A native query
    /// represented as a function may return any type of data. If you choose "function" then the
    /// native query pipeline *must* produce a single document with a single field named `__value`,
    /// and the `resultType` for the native query *must* be an object type with a single field
    /// named `__value`. In GraphQL queries the value of the `__value` field will be the value of
    /// the function in GraphQL responses.
    ///
    /// This setting determines whether the native query appears as a "collection" or as
    /// a "function" in your ddn configuration.
    pub representation: NativeQueryRepresentation,

    /// Arguments to be supplied for each query invocation. These will be available to the given
    /// pipeline as variables. For information about variables in MongoDB aggregation expressions
    /// see https://www.mongodb.com/docs/manual/reference/aggregation-variables/
    ///
    /// Argument values are standard JSON mapped from GraphQL input types, not Extended JSON.
    /// Values will be converted to BSON according to the types specified here.
    #[serde(default)]
    pub arguments: BTreeMap<String, ObjectField>,

    /// The name of an object type that describes documents produced by the given pipeline. MongoDB
    /// aggregation pipelines always produce a list of documents. This type describes the type of
    /// each of those individual documents.
    ///
    /// You may reference object types defined in the `object_types` list in this definition, or
    /// you may reference object types from `schema.json`.
    #[serde(rename = "result_document_type")]
    pub r#type: String,

    /// You may define object types here to reference in `result_type`. Any types defined here will
    /// be merged with the definitions in `schema.json`. This allows you to maintain hand-written
    /// types for native queries without having to edit a generated `schema.json` file.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub object_types: BTreeMap<String, ObjectType>,

    /// Pipeline to include in MongoDB queries. For details on how to write an aggregation pipeline
    /// see https://www.mongodb.com/docs/manual/core/aggregation-pipeline/
    ///
    /// The pipeline may include Extended JSON.
    ///
    /// Keys and values in the pipeline may contain placeholders of the form `{{variableName}}`
    /// which will be substituted when the native procedure is executed according to the given
    /// arguments.
    ///
    /// Placeholders must be inside quotes so that the pipeline can be stored in JSON format. If
    /// the pipeline includes a string whose only content is a placeholder, when the variable is
    /// substituted the string will be replaced by the type of the variable. For example in this
    /// pipeline,
    ///
    /// ```json
    /// json!([{
    ///   "$documents": "{{ documents }}"
    /// }])
    /// ```
    ///
    /// If the type of the `documents` argument is an array then after variable substitution the
    /// pipeline will expand to:
    ///
    /// ```json
    /// json!([{
    ///   "$documents": [/* array of documents */]
    /// }])
    /// ```
    ///
    #[schemars(with = "Vec<serde_json::Value>")]
    pub pipeline: Vec<bson::Document>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
