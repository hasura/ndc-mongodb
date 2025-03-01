use std::collections::VecDeque;

use crate::{self as plan, ObjectType, QueryContext, QueryPlanError};
use ndc_models::{self as ndc};

use super::{
    helpers::{find_object_field, lookup_relationship},
    plan_for_expression,
    query_plan_state::QueryPlanState,
};

type Result<T> = std::result::Result<T, QueryPlanError>;

/// Returns list of aliases for joins to traverse, plus the object type of the final collection in
/// the path.
pub fn plan_for_relationship_path<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    root_collection_object_type: &plan::ObjectType<T::ScalarType>,
    object_type: &plan::ObjectType<T::ScalarType>,
    relationship_path: Vec<ndc::PathElement>,
    requested_columns: Vec<ndc::FieldName>, // columns to select from last path element
) -> Result<(Vec<ndc::RelationshipName>, ObjectType<T::ScalarType>)> {
    let end_of_relationship_path_object_type = relationship_path
        .last()
        .map(|last_path_element| {
            let relationship = lookup_relationship(
                plan_state.collection_relationships,
                &last_path_element.relationship,
            )?;
            plan_state
                .context
                .find_collection_object_type(&relationship.target_collection)
        })
        .transpose()?;
    let target_object_type = end_of_relationship_path_object_type.unwrap_or(object_type.clone());

    let reversed_relationship_path = {
        let mut path = relationship_path;
        path.reverse();
        path
    };

    let vec_deque = plan_for_relationship_path_helper(
        plan_state,
        root_collection_object_type,
        reversed_relationship_path,
        requested_columns,
    )?;
    let aliases = vec_deque.into_iter().collect();

    Ok((aliases, target_object_type))
}

fn plan_for_relationship_path_helper<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    root_collection_object_type: &plan::ObjectType<T::ScalarType>,
    mut reversed_relationship_path: Vec<ndc::PathElement>,
    requested_columns: Vec<ndc::FieldName>, // columns to select from last path element
) -> Result<VecDeque<ndc::RelationshipName>> {
    if reversed_relationship_path.is_empty() {
        return Ok(VecDeque::new());
    }

    // safety: we just made an early return if the path is empty
    let head = reversed_relationship_path.pop().unwrap();
    let tail = reversed_relationship_path;
    let is_last = tail.is_empty();

    let ndc::PathElement {
        field_path: _, // TODO: ENG-1458 support nested relationships
        relationship,
        arguments,
        predicate,
    } = head;

    let relationship_def = lookup_relationship(plan_state.collection_relationships, &relationship)?;
    let related_collection_type = plan_state
        .context
        .find_collection_object_type(&relationship_def.target_collection)?;
    let mut nested_state = plan_state.state_for_subquery();

    // If this is the last path element then we need to apply the requested fields to the
    // relationship query. Otherwise we need to recursively process the rest of the path. Both
    // cases take ownership of `requested_columns` so we group them together.
    let (mut rest_path, fields) = if is_last {
        let fields = requested_columns
            .into_iter()
            .map(|column_name| {
                let object_field =
                    find_object_field(&related_collection_type, &column_name)?.clone();
                Ok((
                    column_name.clone(),
                    plan::Field::Column {
                        column: column_name,
                        fields: None,
                        column_type: object_field.r#type,
                    },
                ))
            })
            .collect::<Result<_>>()?;
        (VecDeque::new(), Some(fields))
    } else {
        let rest = plan_for_relationship_path_helper(
            &mut nested_state,
            root_collection_object_type,
            tail,
            requested_columns,
        )?;
        (rest, None)
    };

    let predicate_plan = predicate
        .map(|p| {
            plan_for_expression(
                &mut nested_state,
                root_collection_object_type,
                &related_collection_type,
                *p,
            )
        })
        .transpose()?;

    let nested_relationships = nested_state.into_relationships();

    let relationship_query = plan::Query {
        predicate: predicate_plan,
        relationships: nested_relationships,
        fields,
        ..Default::default()
    };

    let relation_key =
        plan_state.register_relationship(relationship, arguments, relationship_query)?;

    rest_path.push_front(relation_key);
    Ok(rest_path)
}
