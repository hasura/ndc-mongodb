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
    field_path: Option<&Vec<ndc::FieldName>>,
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

/// Given the type of a collection and a field path returns the object type of the nested object at
/// that path.
pub fn find_nested_collection_type<S>(
    collection_object_type: plan::ObjectType<S>,
    field_path: &[ndc::FieldName],
) -> Result<plan::ObjectType<S>>
where
    S: Clone,
{
    fn normalize_object_type<S>(
        field_path: &[ndc::FieldName],
        t: plan::Type<S>,
    ) -> Result<plan::ObjectType<S>> {
        match t {
            plan::Type::Object(t) => Ok(t),
            plan::Type::ArrayOf(t) => normalize_object_type(field_path, *t),
            plan::Type::Nullable(t) => normalize_object_type(field_path, *t),
            _ => Err(QueryPlanError::ExpectedObject {
                path: field_path.iter().map(|f| f.to_string()).collect(),
            }),
        }
    }

    field_path
        .iter()
        .try_fold(collection_object_type, |obj_type, field_name| {
            let object_field = find_object_field(&obj_type, field_name)?.clone();
            normalize_object_type(field_path, object_field.r#type)
        })
}

pub fn lookup_relationship<'a>(
    relationships: &'a BTreeMap<ndc::RelationshipName, ndc::Relationship>,
    relationship: &ndc::RelationshipName,
) -> Result<&'a ndc::Relationship> {
    relationships
        .get(relationship)
        .ok_or_else(|| QueryPlanError::UnspecifiedRelation(relationship.to_owned()))
}
