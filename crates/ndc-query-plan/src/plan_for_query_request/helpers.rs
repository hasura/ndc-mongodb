use std::collections::BTreeMap;

use ndc_models::{self as ndc};

use crate::{self as plan};

use super::query_plan_error::QueryPlanError;

type Result<T> = std::result::Result<T, QueryPlanError>;

pub fn find_object_field<'a, S>(
    object_type: &'a plan::ObjectType<S>,
    field_name: &ndc::FieldName,
) -> Result<&'a plan::ObjectField<S>> {
    object_type.fields.get(field_name).ok_or_else(|| {
        QueryPlanError::UnknownObjectTypeField {
            object_type: object_type.name.clone(),
            field_name: field_name.clone(),
            path: Default::default(), // TODO: set a path for more helpful error reporting
        }
    })
}

pub fn get_object_field_by_path<'a, S>(
    object_type: &'a plan::ObjectType<S>,
    field_name: &ndc::FieldName,
    field_path: Option<&[ndc::FieldName]>,
) -> Result<&'a plan::ObjectField<S>> {
    match field_path {
        None => find_object_field(object_type, field_name),
        Some(field_path) => get_object_field_by_path_helper(object_type, field_name, field_path),
    }
}

fn get_object_field_by_path_helper<'a, S>(
    object_type: &'a plan::ObjectType<S>,
    field_name: &ndc::FieldName,
    field_path: &[ndc::FieldName],
) -> Result<&'a plan::ObjectField<S>> {
    let object_field = find_object_field(object_type, field_name)?;
    let field_type = &object_field.r#type;
    match field_path {
        [] => Ok(object_field),
        [nested_field_name, rest @ ..] => {
            let o = find_object_type(field_type, &object_type.name, field_name)?;
            get_object_field_by_path_helper(o, nested_field_name, rest)
        }
    }
}

fn find_object_type<'a, S>(
    t: &'a plan::Type<S>,
    parent_type: &Option<ndc::ObjectTypeName>,
    field_name: &ndc::FieldName,
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

/// Given the type of a collection and a field path returns the type of the nested values in an
/// array field at that path.
pub fn find_nested_collection_type<S>(
    collection_object_type: plan::ObjectType<S>,
    field_path: &[ndc::FieldName],
) -> Result<plan::Type<S>>
where
    S: Clone + std::fmt::Debug,
{
    let nested_field = match field_path {
        [field_name] => get_object_field_by_path(&collection_object_type, field_name, None),
        [field_name, rest_of_path @ ..] => {
            get_object_field_by_path(&collection_object_type, field_name, Some(rest_of_path))
        }
        [] => Err(QueryPlanError::UnknownCollection(format!(
            "{}",
            field_path.join(".")
        ))),
    }?;
    let element_type = nested_field.r#type.clone().into_array_element_type()?;
    Ok(element_type)
}

/// Given the type of a collection and a field path returns the object type of the nested object at
/// that path.
///
/// This function differs from [find_nested_collection_type] in that it this one returns
/// [plan::ObjectType] instead of [plan::Type], and returns an error if the nested type is not an
/// object type.
pub fn find_nested_collection_object_type<S>(
    collection_object_type: plan::ObjectType<S>,
    field_path: &[ndc::FieldName],
) -> Result<plan::ObjectType<S>>
where
    S: Clone + std::fmt::Debug,
{
    let collection_element_type = find_nested_collection_type(collection_object_type, field_path)?;
    collection_element_type.into_object_type()
}

pub fn lookup_relationship<'a>(
    relationships: &'a BTreeMap<ndc::RelationshipName, ndc::Relationship>,
    relationship: &ndc::RelationshipName,
) -> Result<&'a ndc::Relationship> {
    relationships
        .get(relationship)
        .ok_or_else(|| QueryPlanError::UnspecifiedRelation(relationship.to_owned()))
}
