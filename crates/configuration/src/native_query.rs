use std::collections::BTreeMap;

use mongodb::bson;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{schema::ObjectField, serialized};

/// Internal representation of Native Queries. For doc comments see
/// [crate::serialized::NativeQuery]
///
/// Note: this type excludes `name` and `object_types` from the serialized type. Object types are
/// intended to be merged into one big map so should not be accessed through values of this type.
/// Native query values are stored in maps so names should be taken from map keys.
#[derive(Clone, Debug)]
pub struct NativeQuery {
    pub representation: NativeQueryRepresentation,
    pub arguments: BTreeMap<String, ObjectField>,
    pub result_document_type: String,
    pub pipeline: Vec<bson::Document>,
    pub description: Option<String>,
}

impl From<serialized::NativeQuery> for NativeQuery {
    fn from(value: serialized::NativeQuery) -> Self {
        NativeQuery {
            representation: value.representation,
            arguments: value.arguments,
            result_document_type: value.result_document_type,
            pipeline: value.pipeline,
            description: value.description,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum NativeQueryRepresentation {
    Collection,
    Function,
}
