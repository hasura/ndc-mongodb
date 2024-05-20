use itertools::Itertools as _;
use ndc_models as ndc;

use crate::{
    Field, Nullable, ObjectType, Query, QueryContext, QueryPlanError, Type, NON_NULLABLE, NULLABLE,
};

use super::{helpers::find_object_field, query_plan_state::QueryPlanState};

type Result<T> = std::result::Result<T, QueryPlanError>;

/// Translates [ndc::Field] to [Field]. The latter includes type annotations.
pub fn type_annotated_field<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    collection_object_type: &ObjectType<T::ScalarType>,
    field: ndc::Field,
) -> Result<Field<T>> {
    type_annotated_field_helper(plan_state, collection_object_type, field, &[])
}

fn type_annotated_field_helper<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    collection_object_type: &ObjectType<T::ScalarType>,
    field: ndc::Field,
    path: &[&str],
) -> Result<Field<T>> {
    let field = match field {
        ndc::Field::Column {
            column,
            fields: None,
        } => Field::Column {
            column,
            column_type: find_object_field(collection_object_type, &column)?.clone(),
        },

        ndc::Field::Column {
            column,
            fields: Some(nested_field),
        } => type_annotated_nested_field_helper(
            plan_state,
            column,
            find_object_field(collection_object_type, &column)?.clone(),
            NON_NULLABLE,
            nested_field,
            path,
        )?,

        ndc::Field::Relationship {
            query,
            relationship,
            ..
        } => {
            let (relationship_key, plan_relationship) =
                plan_state.register_relationship(relationship, *query, [])?;
            Field::Relationship {
                relationship: relationship_key.to_owned(),
                aggregates: plan_relationship.query.aggregates,
                fields: plan_relationship.query.fields,
            }
        }
    };
    Ok(field)
}

/// Translates [ndc::NestedField] to [Field]. The latter includes type annotations.
pub fn type_annotated_nested_field<T: QueryContext>(
    query_context: &T,
    result_type: Type<T::ScalarType>,
    requested_fields: ndc::NestedField,
) -> Result<Field<T>> {
    type_annotated_nested_field_helper(
        &mut QueryPlanState::new(query_context),
        "".to_string(), // TODO
        result_type,
        NON_NULLABLE,
        requested_fields,
        &[],
    )
}

fn type_annotated_nested_field_helper<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    field_name: String,
    result_type: Type<T::ScalarType>,
    is_nullable: Nullable,
    requested_fields: ndc::NestedField,
    path: &[&str],
) -> Result<Field<T>> {
    let field = match (requested_fields, result_type) {
        (ndc::NestedField::Object(object), Type::Object(object_type)) => Field::NestedObject {
            column: field_name.to_owned(),
            query: Box::new(Query {
                fields: Some(
                    object
                        .fields
                        .iter()
                        .map(|(name, field)| {
                            Ok((
                                name.clone(),
                                type_annotated_field_helper(
                                    plan_state,
                                    &object_type,
                                    field.clone(),
                                    &append_to_path(path, [name.as_ref()]),
                                )?,
                            ))
                        })
                        .try_collect()?,
                ),
                ..Default::default()
            }),
            is_nullable,
        },
        (ndc::NestedField::Array(array), Type::ArrayOf(element_type)) => Field::NestedArray {
            field: Box::new(type_annotated_nested_field_helper(
                plan_state,
                "".to_owned(), // TODO
                *element_type,
                NON_NULLABLE,
                *array.fields,
                &append_to_path(path, ["[]"]),
            )?),
            limit: None,
            offset: None,
            predicate: None,
            is_nullable,
        },
        (nested, Type::Nullable(t)) => {
            // let path = append_to_path(path, [])
            type_annotated_nested_field_helper(plan_state, field_name, *t, NULLABLE, nested, path)?
        }
        (ndc::NestedField::Object(_), _) => Err(QueryPlanError::ExpectedObject {
            path: vec!["procedure".to_owned()],
        })?,
        (ndc::NestedField::Array(_), _) => Err(QueryPlanError::ExpectedArray {
            path: vec!["array".to_owned()],
        })?,
    };
    Ok(field)
}

fn append_to_path<'a>(path: &[&'a str], elems: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    path.iter().copied().chain(elems).collect()
}

fn path_to_owned(path: &[&str]) -> Vec<String> {
    path.iter().map(|x| (*x).to_owned()).collect()
}
