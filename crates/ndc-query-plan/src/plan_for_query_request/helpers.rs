use std::collections::BTreeMap;

use ndc_models as ndc;

use crate::{self as plan, ConnectorTypes};

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
    field_path: Option<&Vec<ndc::FieldName>>,
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
/// For now this assumes that if the column has an array type, the value type is a scalar type.
/// That's the simplest option since we don't support array-to-array comparisons yet.
///
/// TODO: When we do support array-to-array comparisons we will need to either:
///
/// - input the [ndc::ComparisonValue] into this function, and any query request variables; check
///   that the given JSON value or variable values are not array values, and if so assume the value
///   type should be a scalar type
/// - or get the GraphQL Engine to include a type with [ndc::ComparisonValue] in which case we can
///   use that as the value type
///
/// It is important that queries behave the same when given an inline value or variables. So we
/// can't just check the value of an [ndc::ComparisonValue::Scalar], and punt on an
/// [ndc::ComparisonValue::Variable] input. The latter requires accessing query request variables,
/// and it will take a little more work to thread those through the code to make them available
/// here.
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

/// In case we need to add a field to query selection so we can make a comparison against it.
///
/// This comes up when filtering rows from a related collection on a field of that collection that
/// is not listed in the original query field selection.
pub fn field_selection_for_comparison_target<T: ConnectorTypes>(
    parent_collection_type: &plan::ObjectType<T::ScalarType>,
    comparison_target: &plan::ComparisonTarget<T>,
) -> Result<plan::Field<T>> {
    let column = comparison_target.column_name().clone();
    let column_type = find_object_field(parent_collection_type, &column)?;
    let field = plan::Field::Column {
        column,
        column_type: column_type.clone(),
        fields: comparison_target
            .field_path()
            .map(|field_path| nested_field_by_parent_type(column_type, field_path))
            .transpose()?,
    };
    Ok(field)
}

pub fn field_path_to_nested_field<T: ConnectorTypes>(
    parent_object_type: &plan::ObjectType<T::ScalarType>,
    field_path: &[ndc::FieldName],
) -> Result<plan::NestedField<T>> {
    let [field_name, rest_path @ ..] = field_path else {
        return Err(QueryPlanError::TypeMismatch("empty field path".to_string()));
    };
    Ok(plan::NestedField::Object(plan::NestedObject {
        fields: [(
            field_name.clone(),
            field_path_to_field_selection(parent_object_type, field_name, rest_path)?,
        )]
        .into(),
    }))
}

fn nested_field_by_parent_type<T: ConnectorTypes>(
    parent_type: &plan::Type<T::ScalarType>,
    field_path: &[ndc::FieldName],
) -> Result<plan::NestedField<T>> {
    match parent_type {
        plan::Type::Object(object_type) => field_path_to_nested_field(object_type, field_path),
        plan::Type::ArrayOf(t) => Ok(plan::NestedField::Array(plan::NestedArray {
            fields: Box::new(nested_field_by_parent_type(t, field_path)?),
        })),
        plan::Type::Nullable(t) => nested_field_by_parent_type(t, field_path),
        plan::Type::Scalar(_) => Err(QueryPlanError::ExpectedObject {
            path: vec![field_path
                .first()
                .map(|f| f.to_string())
                .unwrap_or_else(|| "unknown".to_string())],
        }),
    }
}

fn field_path_to_field_selection<T: ConnectorTypes>(
    parent_object_type: &plan::ObjectType<T::ScalarType>,
    field_name: &ndc::FieldName,
    rest_path: &[ndc::FieldName],
) -> Result<plan::Field<T>> {
    let field_type = find_object_field(parent_object_type, field_name)?;
    field_by_field_type(field_name.clone(), rest_path, field_type)
}

fn field_by_field_type<T: ConnectorTypes>(
    field_name: ndc::FieldName,
    rest_path: &[ndc::FieldName],
    field_type: &plan::Type<T::ScalarType>,
) -> Result<plan::Field<T>> {
    let field = match field_type {
        plan::Type::Scalar(_) => plan::Field::Column {
            column: field_name,
            fields: None,
            column_type: field_type.clone(),
        },
        plan::Type::Object(object_type) => plan::Field::Column {
            column: field_name,
            fields: if rest_path.is_empty() {
                None
            } else {
                Some(field_path_to_nested_field(object_type, rest_path)?)
            },
            column_type: field_type.clone(),
        },
        plan::Type::ArrayOf(element_type) => plan::Field::Column {
            column: field_name,
            fields: if rest_path.is_empty() {
                None
            } else {
                Some(nested_field_by_parent_type(element_type, rest_path)?)
            },
            column_type: field_type.clone(),
        },
        plan::Type::Nullable(t) => field_by_field_type(field_name, rest_path, t)?,
    };
    Ok(field)
}
