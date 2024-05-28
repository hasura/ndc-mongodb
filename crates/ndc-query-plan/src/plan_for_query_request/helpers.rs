use std::collections::BTreeMap;

use ndc_models as ndc;
use crate as plan;

use super::query_plan_error::QueryPlanError;

type Result<T> = std::result::Result<T, QueryPlanError>;

pub fn find_object_field<'a, S>(
    object_type: &'a plan::ObjectType<S>,
    field_name: &str,
) -> Result<&'a plan::Type<S>> {
    object_type.fields.get(field_name).ok_or_else(|| {
        QueryPlanError::UnknownObjectTypeField {
            object_type: object_type.name.clone(),
            field_name: field_name.to_string(),
            path: Default::default(), // TODO: set a path for more helpful error reporting
        }
    })
}

pub fn lookup_relationship<'a>(
    relationships: &'a BTreeMap<String, ndc::Relationship>,
    relationship: &str,
) -> Result<&'a ndc::Relationship> {
    relationships
        .get(relationship)
        .ok_or_else(|| QueryPlanError::UnspecifiedRelation(relationship.to_owned()))
}
