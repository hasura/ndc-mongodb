use std::collections::BTreeMap;

use itertools::Itertools as _;
use ndc_models as ndc;

use crate::{
    Field, NestedArray, NestedField, NestedObject, ObjectType, QueryContext, QueryPlanError, Type,
};

use super::{
    helpers::{find_object_field, lookup_relationship},
    plan_for_query,
    query_plan_state::QueryPlanState,
};

type Result<T> = std::result::Result<T, QueryPlanError>;

/// Translates [ndc::Field] to [Field]. The latter includes type annotations.
pub fn type_annotated_field<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    root_collection_object_type: &ObjectType<T::ScalarType>,
    collection_object_type: &ObjectType<T::ScalarType>,
    field: ndc::Field,
) -> Result<Field<T>> {
    type_annotated_field_helper(
        plan_state,
        root_collection_object_type,
        collection_object_type,
        field,
        &[],
    )
}

fn type_annotated_field_helper<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    root_collection_object_type: &ObjectType<T::ScalarType>,
    collection_object_type: &ObjectType<T::ScalarType>,
    field: ndc::Field,
    path: &[&str],
) -> Result<Field<T>> {
    let field = match field {
        ndc::Field::Column { column, fields } => {
            let column_type = find_object_field(collection_object_type, &column)?;
            let fields = fields
                .map(|nested_field| {
                    type_annotated_nested_field_helper(
                        plan_state,
                        root_collection_object_type,
                        column_type,
                        nested_field,
                        path,
                    )
                })
                .transpose()?;
            Field::Column {
                column_type: column_type.clone(),
                column,
                fields,
            }
        }
        ndc::Field::Relationship {
            arguments,
            query,
            relationship,
        } => {
            let relationship_def =
                lookup_relationship(plan_state.collection_relationships, &relationship)?;
            let related_collection_type = plan_state
                .context
                .find_collection_object_type(&relationship_def.target_collection)?;

            let query_plan = plan_for_query(
                &mut plan_state.state_for_subquery(),
                root_collection_object_type,
                &related_collection_type,
                *query,
            )?;

            let (relationship_key, plan_relationship) =
                plan_state.register_relationship(relationship, arguments, query_plan)?;
            Field::Relationship {
                relationship: relationship_key.to_owned(),
                aggregates: plan_relationship.query.aggregates.clone(),
                fields: plan_relationship.query.fields.clone(),
            }
        }
    };
    Ok(field)
}

/// Translates [ndc::NestedField] to [Field]. The latter includes type annotations.
pub fn type_annotated_nested_field<T: QueryContext>(
    query_context: &T,
    collection_relationships: &BTreeMap<String, ndc::Relationship>,
    result_type: &Type<T::ScalarType>,
    requested_fields: ndc::NestedField,
) -> Result<NestedField<T>> {
    // TODO: root column references for mutations
    let root_collection_object_type = &ObjectType {
        name: None,
        fields: Default::default(),
    };
    type_annotated_nested_field_helper(
        &mut QueryPlanState::new(query_context, collection_relationships),
        root_collection_object_type,
        result_type,
        requested_fields,
        &[],
    )
}

fn type_annotated_nested_field_helper<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    root_collection_object_type: &ObjectType<T::ScalarType>,
    parent_type: &Type<T::ScalarType>,
    requested_fields: ndc::NestedField,
    path: &[&str],
) -> Result<NestedField<T>> {
    let field = match (requested_fields, parent_type) {
        (ndc::NestedField::Object(object), Type::Object(object_type)) => {
            NestedField::Object(NestedObject {
                fields: object
                    .fields
                    .iter()
                    .map(|(name, field)| {
                        Ok((
                            name.clone(),
                            type_annotated_field_helper(
                                plan_state,
                                root_collection_object_type,
                                object_type,
                                field.clone(),
                                &append_to_path(path, [name.as_ref()]),
                            )?,
                        )) as Result<_>
                    })
                    .try_collect()?,
            })
        }
        (ndc::NestedField::Array(array), Type::ArrayOf(element_type)) => {
            NestedField::Array(NestedArray {
                fields: Box::new(type_annotated_nested_field_helper(
                    plan_state,
                    root_collection_object_type,
                    element_type,
                    *array.fields,
                    &append_to_path(path, ["[]"]),
                )?),
            })
        }
        (nested, Type::Nullable(t)) => {
            // let path = append_to_path(path, [])
            type_annotated_nested_field_helper(
                plan_state,
                root_collection_object_type,
                t,
                nested,
                path,
            )?
        }
        (ndc::NestedField::Object(_), _) => Err(QueryPlanError::ExpectedObject {
            path: path_to_owned(path),
        })?,
        (ndc::NestedField::Array(_), _) => Err(QueryPlanError::ExpectedArray {
            path: path_to_owned(path),
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
