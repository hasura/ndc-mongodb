mod helpers;
mod plan_for_arguments;
pub mod query_context;
pub mod query_plan_error;
mod query_plan_state;
pub mod type_annotated_field;
mod unify_relationship_references;

#[cfg(test)]
mod plan_test_helpers;
#[cfg(test)]
mod tests;

use std::collections::VecDeque;

use crate::{self as plan, type_annotated_field, ObjectType, QueryPlan, Scope};
use indexmap::IndexMap;
use itertools::Itertools;
use ndc::{ExistsInCollection, QueryRequest};
use ndc_models as ndc;
use query_plan_state::QueryPlanInfo;

use self::{
    helpers::{find_object_field, find_object_field_path, lookup_relationship},
    plan_for_arguments::plan_for_arguments,
    query_context::QueryContext,
    query_plan_error::QueryPlanError,
    query_plan_state::QueryPlanState,
};

type Result<T> = std::result::Result<T, QueryPlanError>;

pub fn plan_for_query_request<T: QueryContext>(
    context: &T,
    request: QueryRequest,
) -> Result<QueryPlan<T>> {
    let mut plan_state = QueryPlanState::new(context, &request.collection_relationships);
    let collection_info = context.find_collection(&request.collection)?;
    let collection_object_type = context.find_collection_object_type(&request.collection)?;

    let mut query = plan_for_query(
        &mut plan_state,
        &collection_object_type,
        &collection_object_type,
        request.query,
    )?;
    query.scope = Some(Scope::Root);

    let arguments = plan_for_arguments(
        &mut plan_state,
        &collection_info.arguments,
        request.arguments,
    )?;

    let QueryPlanInfo {
        unrelated_joins,
        variable_types,
    } = plan_state.into_query_plan_info();

    // If there are variables that don't have corresponding entries in the variable_types map that
    // means that those variables were not observed in the query. Filter them out because we don't
    // need them, and we don't want users to have to deal with variables with unknown types.
    let variables = request.variables.map(|variable_sets| {
        variable_sets
            .into_iter()
            .map(|variable_set| {
                variable_set
                    .into_iter()
                    .filter(|(var_name, _)| {
                        variable_types
                            .get(var_name)
                            .map(|types| !types.is_empty())
                            .unwrap_or(false)
                    })
                    .collect()
            })
            .collect()
    });

    Ok(QueryPlan {
        collection: request.collection,
        arguments,
        query,
        variables,
        variable_types,
        unrelated_collections: unrelated_joins,
    })
}

/// root_collection_object_type references the collection type of the nearest enclosing [ndc::Query]
pub fn plan_for_query<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    root_collection_object_type: &plan::ObjectType<T::ScalarType>,
    collection_object_type: &plan::ObjectType<T::ScalarType>,
    query: ndc::Query,
) -> Result<plan::Query<T>> {
    let mut plan_state = plan_state.state_for_subquery();

    let aggregates =
        plan_for_aggregates(plan_state.context, collection_object_type, query.aggregates)?;
    let fields = plan_for_fields(
        &mut plan_state,
        root_collection_object_type,
        collection_object_type,
        query.fields,
    )?;

    let order_by = query
        .order_by
        .map(|order_by| {
            plan_for_order_by(
                &mut plan_state,
                root_collection_object_type,
                collection_object_type,
                order_by,
            )
        })
        .transpose()?;

    let limit = query.limit;
    let offset = query.offset;

    let predicate = query
        .predicate
        .map(|expr| {
            plan_for_expression(
                &mut plan_state,
                root_collection_object_type,
                collection_object_type,
                expr,
            )
        })
        .transpose()?;

    Ok(plan::Query {
        aggregates,
        aggregates_limit: limit,
        fields,
        order_by,
        limit,
        offset,
        predicate,
        relationships: plan_state.into_relationships(),
        scope: None,
    })
}

fn plan_for_aggregates<T: QueryContext>(
    context: &T,
    collection_object_type: &plan::ObjectType<T::ScalarType>,
    ndc_aggregates: Option<IndexMap<ndc::FieldName, ndc::Aggregate>>,
) -> Result<Option<IndexMap<ndc::FieldName, plan::Aggregate<T>>>> {
    ndc_aggregates
        .map(|aggregates| -> Result<_> {
            aggregates
                .into_iter()
                .map(|(name, aggregate)| {
                    Ok((
                        name,
                        plan_for_aggregate(context, collection_object_type, aggregate)?,
                    ))
                })
                .collect()
        })
        .transpose()
}

fn plan_for_aggregate<T: QueryContext>(
    context: &T,
    collection_object_type: &plan::ObjectType<T::ScalarType>,
    aggregate: ndc::Aggregate,
) -> Result<plan::Aggregate<T>> {
    match aggregate {
        ndc::Aggregate::ColumnCount {
            column,
            distinct,
            field_path: _,
        } => Ok(plan::Aggregate::ColumnCount { column, distinct }),
        ndc::Aggregate::SingleColumn {
            column,
            function,
            field_path: _,
        } => {
            let object_type_field_type = find_object_field(collection_object_type, &column)?;
            // let column_scalar_type_name = get_scalar_type_name(&object_type_field.r#type)?;
            let (function, definition) =
                context.find_aggregation_function_definition(object_type_field_type, &function)?;
            Ok(plan::Aggregate::SingleColumn {
                column,
                function,
                result_type: definition.result_type.clone(),
            })
        }
        ndc::Aggregate::StarCount {} => Ok(plan::Aggregate::StarCount {}),
    }
}

fn plan_for_fields<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    root_collection_object_type: &plan::ObjectType<T::ScalarType>,
    collection_object_type: &plan::ObjectType<T::ScalarType>,
    ndc_fields: Option<IndexMap<ndc::FieldName, ndc::Field>>,
) -> Result<Option<IndexMap<ndc::FieldName, plan::Field<T>>>> {
    let plan_fields: Option<IndexMap<ndc::FieldName, plan::Field<T>>> = ndc_fields
        .map(|fields| {
            fields
                .into_iter()
                .map(|(name, field)| {
                    Ok((
                        name,
                        type_annotated_field(
                            plan_state,
                            root_collection_object_type,
                            collection_object_type,
                            field,
                        )?,
                    ))
                })
                .collect::<Result<_>>()
        })
        .transpose()?;
    Ok(plan_fields)
}

fn plan_for_order_by<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    root_collection_object_type: &plan::ObjectType<T::ScalarType>,
    object_type: &plan::ObjectType<T::ScalarType>,
    order_by: ndc::OrderBy,
) -> Result<plan::OrderBy<T>> {
    let elements = order_by
        .elements
        .into_iter()
        .map(|element| {
            plan_for_order_by_element(
                plan_state,
                root_collection_object_type,
                object_type,
                element,
            )
        })
        .try_collect()?;
    Ok(plan::OrderBy { elements })
}

fn plan_for_order_by_element<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    root_collection_object_type: &plan::ObjectType<T::ScalarType>,
    object_type: &plan::ObjectType<T::ScalarType>,
    element: ndc::OrderByElement,
) -> Result<plan::OrderByElement<T>> {
    let target = match element.target {
        ndc::OrderByTarget::Column {
            name,
            field_path,
            path,
        } => plan::OrderByTarget::Column {
            name: name.clone(),
            field_path,
            path: plan_for_relationship_path(
                plan_state,
                root_collection_object_type,
                object_type,
                path,
                vec![name],
            )?
            .0,
        },
        ndc::OrderByTarget::SingleColumnAggregate {
            column,
            function,
            path,
            field_path: _,
        } => {
            let (plan_path, target_object_type) = plan_for_relationship_path(
                plan_state,
                root_collection_object_type,
                object_type,
                path,
                vec![], // TODO: MDB-156 propagate requested aggregate to relationship query
            )?;
            let column_type = find_object_field(&target_object_type, &column)?;
            let (function, function_definition) = plan_state
                .context
                .find_aggregation_function_definition(column_type, &function)?;

            plan::OrderByTarget::SingleColumnAggregate {
                column,
                function,
                result_type: function_definition.result_type.clone(),
                path: plan_path,
            }
        }
        ndc::OrderByTarget::StarCountAggregate { path } => {
            let (plan_path, _) = plan_for_relationship_path(
                plan_state,
                root_collection_object_type,
                object_type,
                path,
                vec![], // TODO: MDB-157 propagate requested aggregate to relationship query
            )?;
            plan::OrderByTarget::StarCountAggregate { path: plan_path }
        }
    };

    Ok(plan::OrderByElement {
        order_direction: element.order_direction,
        target,
    })
}

/// Returns list of aliases for joins to traverse, plus the object type of the final collection in
/// the path.
fn plan_for_relationship_path<T: QueryContext>(
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
                let column_type =
                    find_object_field(&related_collection_type, &column_name)?.clone();
                Ok((
                    column_name.clone(),
                    plan::Field::Column {
                        column: column_name,
                        fields: None,
                        column_type,
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

fn plan_for_expression<T: QueryContext>(
    plan_state: &mut QueryPlanState<T>,
    root_collection_object_type: &plan::ObjectType<T::ScalarType>,
    object_type: &plan::ObjectType<T::ScalarType>,
    expression: ndc::Expression,
) -> Result<plan::Expression<T>> {
    match expression {
        ndc::Expression::And { expressions } => Ok(plan::Expression::And {
            expressions: expressions
                .into_iter()
                .map(|expr| {
                    plan_for_expression(plan_state, root_collection_object_type, object_type, expr)
                })
                .collect::<Result<_>>()?,
        }),
        ndc::Expression::Or { expressions } => Ok(plan::Expression::Or {
            expressions: expressions
                .into_iter()
                .map(|expr| {
                    plan_for_expression(plan_state, root_collection_object_type, object_type, expr)
                })
                .collect::<Result<_>>()?,
        }),
        ndc::Expression::Not { expression } => Ok(plan::Expression::Not {
            expression: Box::new(plan_for_expression(
                plan_state,
                root_collection_object_type,
                object_type,
                *expression,
            )?),
        }),
        ndc::Expression::UnaryComparisonOperator { column, operator } => {
            Ok(plan::Expression::UnaryComparisonOperator {
                column: plan_for_comparison_target(
                    plan_state,
                    root_collection_object_type,
                    object_type,
                    column,
                )?,
                operator,
            })
        }
        ndc::Expression::BinaryComparisonOperator {
            column,
            operator,
            value,
        } => plan_for_binary_comparison(
            plan_state,
            root_collection_object_type,
            object_type,
            column,
            operator,
            value,
        ),
        ndc::Expression::Exists {
            in_collection,
            predicate,
        } => plan_for_exists(
            plan_state,
            root_collection_object_type,
            in_collection,
            predicate,
        ),
    }
}

fn plan_for_binary_comparison<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    root_collection_object_type: &plan::ObjectType<T::ScalarType>,
    object_type: &plan::ObjectType<T::ScalarType>,
    column: ndc::ComparisonTarget,
    operator: ndc::ComparisonOperatorName,
    value: ndc::ComparisonValue,
) -> Result<plan::Expression<T>> {
    let comparison_target =
        plan_for_comparison_target(plan_state, root_collection_object_type, object_type, column)?;
    let (operator, operator_definition) = plan_state
        .context
        .find_comparison_operator(comparison_target.get_field_type(), &operator)?;
    let value_type = match operator_definition {
        plan::ComparisonOperatorDefinition::Equal => comparison_target.get_field_type().clone(),
        plan::ComparisonOperatorDefinition::In => {
            plan::Type::ArrayOf(Box::new(comparison_target.get_field_type().clone()))
        }
        plan::ComparisonOperatorDefinition::Custom { argument_type } => argument_type.clone(),
    };
    Ok(plan::Expression::BinaryComparisonOperator {
        operator,
        value: plan_for_comparison_value(
            plan_state,
            root_collection_object_type,
            object_type,
            value_type,
            value,
        )?,
        column: comparison_target,
    })
}

fn plan_for_comparison_target<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    root_collection_object_type: &plan::ObjectType<T::ScalarType>,
    object_type: &plan::ObjectType<T::ScalarType>,
    target: ndc::ComparisonTarget,
) -> Result<plan::ComparisonTarget<T>> {
    match target {
        ndc::ComparisonTarget::Column {
            name,
            field_path,
            path,
        } => {
            let requested_columns = vec![name.clone()];
            let (path, target_object_type) = plan_for_relationship_path(
                plan_state,
                root_collection_object_type,
                object_type,
                path,
                requested_columns,
            )?;
            let field_type =
                find_object_field_path(&target_object_type, &name, &field_path)?.clone();
            Ok(plan::ComparisonTarget::Column {
                name,
                field_path,
                path,
                field_type,
            })
        }
        ndc::ComparisonTarget::RootCollectionColumn { name, field_path } => {
            let field_type =
                find_object_field_path(root_collection_object_type, &name, &field_path)?.clone();
            Ok(plan::ComparisonTarget::ColumnInScope {
                name,
                field_path,
                field_type,
                scope: plan_state.scope.clone(),
            })
        }
    }
}

fn plan_for_comparison_value<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    root_collection_object_type: &plan::ObjectType<T::ScalarType>,
    object_type: &plan::ObjectType<T::ScalarType>,
    expected_type: plan::Type<T::ScalarType>,
    value: ndc::ComparisonValue,
) -> Result<plan::ComparisonValue<T>> {
    match value {
        ndc::ComparisonValue::Column { column } => Ok(plan::ComparisonValue::Column {
            column: plan_for_comparison_target(
                plan_state,
                root_collection_object_type,
                object_type,
                column,
            )?,
        }),
        ndc::ComparisonValue::Scalar { value } => Ok(plan::ComparisonValue::Scalar {
            value,
            value_type: expected_type,
        }),
        ndc::ComparisonValue::Variable { name } => {
            plan_state.register_variable_use(&name, expected_type.clone());
            Ok(plan::ComparisonValue::Variable {
                name,
                variable_type: expected_type,
            })
        }
    }
}

fn plan_for_exists<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    root_collection_object_type: &plan::ObjectType<T::ScalarType>,
    in_collection: ExistsInCollection,
    predicate: Option<Box<ndc::Expression>>,
) -> Result<plan::Expression<T>> {
    let mut nested_state = plan_state.state_for_subquery();

    let (in_collection, predicate) = match in_collection {
        ndc::ExistsInCollection::Related {
            relationship,
            arguments,
        } => {
            let ndc_relationship =
                lookup_relationship(plan_state.collection_relationships, &relationship)?;
            let collection_object_type = plan_state
                .context
                .find_collection_object_type(&ndc_relationship.target_collection)?;

            let predicate = predicate
                .map(|expression| {
                    plan_for_expression(
                        &mut nested_state,
                        root_collection_object_type,
                        &collection_object_type,
                        *expression,
                    )
                })
                .transpose()?;

            let fields = predicate.as_ref().map(|p| {
                p.query_local_comparison_targets()
                    .map(|comparison_target| {
                        (
                            comparison_target.column_name().to_owned(),
                            plan::Field::Column {
                                column: comparison_target.column_name().clone(),
                                column_type: comparison_target.get_field_type().clone(),
                                fields: None,
                            },
                        )
                    })
                    .collect()
            });

            let relationship_query = plan::Query {
                fields,
                relationships: nested_state.into_relationships(),
                ..Default::default()
            };

            let relationship_key =
                plan_state.register_relationship(relationship, arguments, relationship_query)?;

            let in_collection = plan::ExistsInCollection::Related {
                relationship: relationship_key,
            };

            Ok((in_collection, predicate)) as Result<_>
        }
        ndc::ExistsInCollection::Unrelated {
            collection,
            arguments,
        } => {
            let collection_object_type = plan_state
                .context
                .find_collection_object_type(&collection)?;

            let predicate = predicate
                .map(|expression| {
                    plan_for_expression(
                        &mut nested_state,
                        root_collection_object_type,
                        &collection_object_type,
                        *expression,
                    )
                })
                .transpose()?;

            let join_query = plan::Query {
                predicate: predicate.clone(),
                relationships: nested_state.into_relationships(),
                ..Default::default()
            };

            let join_key = plan_state.register_unrelated_join(collection, arguments, join_query);

            let in_collection = plan::ExistsInCollection::Unrelated {
                unrelated_collection: join_key,
            };
            Ok((in_collection, predicate))
        }
    }?;

    Ok(plan::Expression::Exists {
        in_collection,
        predicate: predicate.map(Box::new),
    })
}
