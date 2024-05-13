use std::collections::BTreeMap;

use itertools::Itertools as _;
use mongodb::bson;
use ndc_models as ndc;
use ndc_query_plan as plan;
use plan::{inline_object_types, QueryPlanError};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{serialized, MongoScalarType};

/// Internal representation of Native Queries. For doc comments see
/// [crate::serialized::NativeQuery]
///
/// Note: this type excludes `name` and `object_types` from the serialized type. Object types are
/// intended to be merged into one big map so should not be accessed through values of this type.
/// Native query values are stored in maps so names should be taken from map keys.
#[derive(Clone, Debug)]
pub struct NativeQuery {
    pub representation: NativeQueryRepresentation,
    pub input_collection: Option<String>,
    pub arguments: BTreeMap<String, plan::Type<MongoScalarType>>,
    pub result_document_type: String,
    pub pipeline: Vec<bson::Document>,
    pub description: Option<String>,
}

impl NativeQuery {
    pub fn from_serialized(
        object_types: &BTreeMap<String, ndc::ObjectType>,
        input: serialized::NativeQuery,
    ) -> Result<NativeQuery, QueryPlanError> {
        let arguments = input
            .arguments
            .into_iter()
            .map(|(name, object_field)| {
                Ok((
                    name,
                    inline_object_types(
                        object_types,
                        &object_field.r#type.into(),
                        MongoScalarType::lookup_scalar_type,
                    )?,
                ))
            })
            .try_collect()?;

        Ok(NativeQuery {
            representation: input.representation,
            input_collection: input.input_collection,
            arguments,
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
