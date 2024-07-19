use std::collections::BTreeMap;

use mongodb::bson;
use ndc_models as ndc;
use ndc_query_plan as plan;
use plan::QueryPlanError;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::serialized;

/// Internal representation of Native Queries. For doc comments see
/// [crate::serialized::NativeQuery]
///
/// Note: this type excludes `name` and `object_types` from the serialized type. Object types are
/// intended to be merged into one big map so should not be accessed through values of this type.
/// Native query values are stored in maps so names should be taken from map keys.
#[derive(Clone, Debug)]
pub struct NativeQuery {
    pub representation: NativeQueryRepresentation,
    pub input_collection: Option<ndc::CollectionName>,
    pub result_document_type: ndc::ObjectTypeName,
    pub pipeline: Vec<bson::Document>,
    pub description: Option<String>,
}

impl NativeQuery {
    pub fn from_serialized(
        _object_types: &BTreeMap<ndc::ObjectTypeName, ndc::ObjectType>,
        input: serialized::NativeQuery,
    ) -> Result<NativeQuery, QueryPlanError> {
        Ok(NativeQuery {
            representation: input.representation,
            input_collection: input.input_collection,
            result_document_type: input.result_document_type,
            pipeline: input.pipeline,
            description: input.description,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum NativeQueryRepresentation {
    Collection,
    Function,
}
