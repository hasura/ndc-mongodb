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

pub fn find_object_field_path<'a, S>(
    object_type: &'a plan::ObjectType<S>,
    field_name: &str,
    field_path: &Option<Vec<String>>,
) -> Result<&'a plan::Type<S>> {
    match field_path {
        None => find_object_field(object_type, field_name),
        Some(field_path) => find_object_field_path_helper(object_type, field_name, field_path),
    }
}

fn find_object_field_path_helper<'a, S>(
    object_type: &'a plan::ObjectType<S>,
    field_name: &str,
    field_path: &[String],
) -> Result<&'a plan::Type<S>> {
    let field_type = find_object_field(object_type, field_name)?;
    match field_path {
        [] => Ok(field_type),
        [nested_field_name, rest @ ..] => {
            let o = find_object_type(field_type, &object_type.name, field_name)?;
            find_object_field_path_helper(o, nested_field_name, rest)
        }
    }
}

fn find_object_type<'a, S>(
    t: &'a plan::Type<S>,
    parent_type: &Option<String>,
    field_name: &str,
) -> Result<&'a plan::ObjectType<S>> {
    match t {
        crate::Type::Scalar(_) => Err(QueryPlanError::ExpectedObjectTypeAtField {
            parent_type: parent_type.to_owned(),
            field_name: field_name.to_owned(),
            got: "scalar".to_owned(),
        }),
        crate::Type::ArrayOf(_) => Err(QueryPlanError::ExpectedObjectTypeAtField {
            parent_type: parent_type.to_owned(),
            field_name: field_name.to_owned(),
            got: "array".to_owned(),
        }),
        crate::Type::Nullable(t) => find_object_type(t, parent_type, field_name),
        crate::Type::Object(object_type) => Ok(object_type),
    }
}

pub fn lookup_relationship<'a>(
    relationships: &'a BTreeMap<String, ndc::Relationship>,
    relationship: &str,
) -> Result<&'a ndc::Relationship> {
    relationships
        .get(relationship)
        .ok_or_else(|| QueryPlanError::UnspecifiedRelation(relationship.to_owned()))
}
