use std::collections::BTreeMap;

use itertools::Itertools as _;
use mongodb::{bson, options::SelectionCriteria};
use ndc_models as ndc;
use ndc_query_plan as plan;
use plan::{inline_object_types, QueryPlanError};

use crate::{serialized, MongoScalarType};

/// Internal representation of Native Mutations. For doc comments see
/// [crate::serialized::NativeMutation]
///
/// Note: this type excludes `name` and `object_types` from the serialized type. Object types are
/// intended to be merged into one big map so should not be accessed through values of this type.
/// Native query values are stored in maps so names should be taken from map keys.
#[derive(Clone, Debug)]
pub struct NativeMutation {
    pub result_type: plan::Type<MongoScalarType>,
    pub arguments: BTreeMap<String, plan::Type<MongoScalarType>>,
    pub command: bson::Document,
    pub selection_criteria: Option<SelectionCriteria>,
    pub description: Option<String>,
}

impl NativeMutation {
    pub fn from_serialized(
        object_types: &BTreeMap<String, ndc::ObjectType>,
        input: serialized::NativeMutation,
    ) -> Result<NativeMutation, QueryPlanError> {
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
                )) as Result<_, QueryPlanError>
            })
            .try_collect()?;

        let result_type = inline_object_types(
            object_types,
            &input.result_type.into(),
            MongoScalarType::lookup_scalar_type,
        )?;

        Ok(NativeMutation {
            result_type,
            arguments,
            command: input.command,
            selection_criteria: input.selection_criteria,
            description: input.description,
        })
    }
}
