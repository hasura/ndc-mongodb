use std::collections::BTreeMap;

use ndc_models as ndc;

use crate::{self as plan};

use super::query_plan_error::QueryPlanError;

type Result<T> = std::result::Result<T, QueryPlanError>;

pub fn find_object_field<'a, S>(
    object_type: &'a plan::ObjectType<S>,
    field_name: &ndc::FieldName,
) -> Result<&'a plan::Type<S>> {
    object_type.fields.get(field_name).ok_or_else(|| {
        QueryPlanError::UnknownObjectTypeField {
            object_type: object_type.name.clone(),
            field_name: field_name.clone(),
            path: Default::default(), // TODO: set a path for more helpful error reporting
        }
    })
}

pub fn find_object_field_path<'a, S>(
    object_type: &'a plan::ObjectType<S>,
    field_name: &ndc::FieldName,
    field_path: &Option<Vec<ndc::FieldName>>,
) -> Result<&'a plan::Type<S>> {
    match field_path {
        None => find_object_field(object_type, field_name),
        Some(field_path) => find_object_field_path_helper(object_type, field_name, field_path),
    }
}

fn find_object_field_path_helper<'a, S>(
    object_type: &'a plan::ObjectType<S>,
    field_name: &ndc::FieldName,
    field_path: &[ndc::FieldName],
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
            let field_type = find_object_field(&obj_type, field_name)?.clone();
            normalize_object_type(field_path, field_type)
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

/// Special case handling for array comparisons! Normally we assume that the right operand of Equal
/// is the same type as the left operand. BUT MongoDB allows comparing arrays to scalar values in
/// which case the condition passes if any array element is equal to the given scalar value. So
/// this function needs to return a scalar type if the user is expecting array-to-scalar
/// comparison, or an array type if the user is expecting array-to-array comparison. Or if the
/// column does not have an array type we fall back to the default assumption that the value type
/// should be the same as the column type.
///
/// We could check if the column has an array type, and the given value is a JSON value that is not
/// an array. But if the comparison value is a _variable_ we don't have any value to check so that
/// strategy would not allow array-to-scalar comparisons with variables which would mean queries
/// behave differently with inline values vs variables.
///
/// The option that gives the most consistency is to assume that the value type is a scalar type if
/// the value is a JSON value or a variable. But that means that in the future we won't be able to
/// get make equality comparisons between to arrays unless they are both column values, or we get
/// type information from the engine. (In the case of column-to-column comparisons we can get types
/// from the schema, which happens in `plan_for_comparison_value`, so none of this is a problem).
pub fn value_type_in_possible_array_equality_comparison<S>(
    column_type: plan::Type<S>,
) -> plan::Type<S>
where
    S: Clone,
{
    match column_type {
        plan::Type::ArrayOf(t) => *t,
        plan::Type::Nullable(t) => match *t {
            v @ plan::Type::ArrayOf(_) => {
                value_type_in_possible_array_equality_comparison(v.clone())
            }
            t => plan::Type::Nullable(Box::new(t)),
        },
        _ => column_type,
    }
}
