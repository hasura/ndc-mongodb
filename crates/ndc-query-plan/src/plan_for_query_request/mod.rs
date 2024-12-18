mod helpers;
mod plan_for_arguments;
mod plan_for_mutation_request;
pub mod query_context;
pub mod query_plan_error;
mod query_plan_state;
pub mod type_annotated_field;
mod unify_relationship_references;

#[cfg(test)]
mod plan_test_helpers;
#[cfg(test)]
mod tests;

use std::{collections::VecDeque, iter::once};

use crate::{self as plan, type_annotated_field, ObjectType, QueryPlan, Scope};
use indexmap::IndexMap;
use itertools::Itertools;
use ndc::{ExistsInCollection, QueryRequest};
use ndc_models::{self as ndc};
use query_plan_state::QueryPlanInfo;

pub use self::plan_for_mutation_request::plan_for_mutation_request;
use self::{
    helpers::{
        find_nested_collection_type, find_object_field, get_object_field_by_path,
        lookup_relationship,
    },
    plan_for_arguments::{plan_arguments_from_plan_parameters, plan_for_arguments},
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
            arguments,
            distinct,
            field_path,
        } => {
            let arguments = if arguments.is_empty() {
                Default::default()
            } else {
                Err(QueryPlanError::NotImplemented(
                    "arguments on column count aggregate".to_string(),
                ))?
            };

            // TODO: To support aggregate arguments here we need a way to look up aggregate
            // parameters (a map of supported argument names to types). When we have that replace
            // the above `arguments` assignment with this one:
            // let arguments = plan_for_arguments(plan_state, parameters, arguments)?;

            Ok(plan::Aggregate::ColumnCount {
                column,
                arguments,
                distinct,
                field_path,
            })
        }
        ndc::Aggregate::SingleColumn {
            column,
            arguments,
            function,
            field_path,
        } => {
            let arguments = if arguments.is_empty() {
                Default::default()
            } else {
                Err(QueryPlanError::NotImplemented(
                    "arguments on single-column aggregate".to_string(),
                ))?
            };

            // TODO: To support aggregate arguments here we need a way to look up aggregate
            // parameters (a map of supported argument names to types). When we have that replace
            // the above `arguments` assignment with this one:
            // let arguments = plan_for_arguments(plan_state, parameters, arguments)?;

            let object_type_field_type =
                get_object_field_by_path(collection_object_type, &column, field_path.as_ref())?;
            let (function, definition) =
                context.find_aggregation_function_definition(object_type_field_type, &function)?;
            Ok(plan::Aggregate::SingleColumn {
                column,
                arguments,
                field_path,
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
            path,
            name,
            arguments,
            field_path,
        } => {
            let (relationship_names, collection_object_type) = plan_for_relationship_path(
                plan_state,
                root_collection_object_type,
                object_type,
                path,
                vec![name],
            )?;
            let object_field = collection_object_type.get(&name)?;

            let plan_arguments = plan_arguments_from_plan_parameters(
                plan_state,
                &object_field.parameters,
                arguments,
            )?;

            plan::OrderByTarget::Column {
                path: relationship_names,
                name: name.clone(),
                arguments: plan_arguments,
                field_path,
            }
        }
        ndc::OrderByTarget::Aggregate {
            path,
            aggregate:
                ndc::Aggregate::ColumnCount {
                    column,
                    arguments,
                    field_path,
                    distinct,
                },
        } => {
            let (plan_path, collection_object_type) = plan_for_relationship_path(
                plan_state,
                root_collection_object_type,
                object_type,
                path,
                vec![], // TODO: ENG-1019 propagate requested aggregate to relationship query
            )?;

            let object_field = collection_object_type.get(&column)?;

            let plan_arguments = plan_arguments_from_plan_parameters(
                plan_state,
                &object_field.parameters,
                arguments,
            )?;

            plan::OrderByTarget::Aggregate {
                path: plan_path,
                aggregate: plan::Aggregate::ColumnCount {
                    column,
                    arguments: plan_arguments,
                    field_path,
                    distinct,
                },
            }
        }
        ndc::OrderByTarget::Aggregate {
            path,
            aggregate:
                ndc::Aggregate::SingleColumn {
                    column,
                    arguments,
                    field_path,
                    function,
                },
        } => {
            let (plan_path, collection_object_type) = plan_for_relationship_path(
                plan_state,
                root_collection_object_type,
                object_type,
                path,
                vec![], // TODO: ENG-1019 propagate requested aggregate to relationship query
            )?;

            let object_field = collection_object_type.get(&column)?;

            let plan_arguments = plan_arguments_from_plan_parameters(
                plan_state,
                &object_field.parameters,
                arguments,
            )?;

            let column_type = find_object_field(&collection_object_type, &column)?;
            let (function, function_definition) = plan_state
                .context
                .find_aggregation_function_definition(column_type, &function)?;

            plan::OrderByTarget::Aggregate {
                path: plan_path,
                aggregate: plan::Aggregate::SingleColumn {
                    column,
                    arguments: plan_arguments,
                    field_path,
                    function,
                    result_type: function_definition.result_type.clone(),
                },
            }
        }
        ndc::OrderByTarget::Aggregate {
            path,
            aggregate: ndc::Aggregate::StarCount {},
        } => {
            let (plan_path, _) = plan_for_relationship_path(
                plan_state,
                root_collection_object_type,
                object_type,
                path,
                vec![], // TODO: ENG-1019 propagate requested aggregate to relationship query
            )?;
            plan::OrderByTarget::Aggregate {
                path: plan_path,
                aggregate: plan::Aggregate::StarCount,
            }
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
        field_path,
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
        ndc::Expression::ArrayComparison { column, comparison } => plan_for_array_comparison(
            plan_state,
            root_collection_object_type,
            object_type,
            column,
            comparison,
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
        .find_comparison_operator(comparison_target.target_type(), &operator)?;
    let value_type = match operator_definition {
        plan::ComparisonOperatorDefinition::Equal => {
            let column_type = comparison_target.target_type().clone();
            column_type
        }
        plan::ComparisonOperatorDefinition::In => {
            plan::Type::ArrayOf(Box::new(comparison_target.target_type().clone()))
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

fn plan_for_array_comparison<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    root_collection_object_type: &plan::ObjectType<T::ScalarType>,
    object_type: &plan::ObjectType<T::ScalarType>,
    column: ndc::ComparisonTarget,
    comparison: ndc::ArrayComparison,
) -> Result<plan::Expression<T>> {
    let comparison_target =
        plan_for_comparison_target(plan_state, root_collection_object_type, object_type, column)?;
    let plan_comparison = match comparison {
        ndc::ArrayComparison::Contains { value } => {
            let target_field = find_object_field(object_type, column)?;
            let field_type = target_field.r#type;
            let array_element_type = field_type.array_element_type()?;
            let value = plan_for_comparison_value(
                plan_state,
                root_collection_object_type,
                object_type,
                array_element_type,
                value,
            )?;
            plan::ArrayComparison::Contains { value }
        }
        ndc::ArrayComparison::IsEmpty => plan::ArrayComparison::IsEmpty,
    };
    Ok(plan::Expression::ArrayComparison {
        column: comparison_target,
        comparison: plan_comparison,
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
            arguments,
            field_path,
        } => {
            let requested_columns = vec![name.clone()];
            let object_field =
                get_object_field_by_path(object_type, &name, field_path.as_ref())?.clone();
            let plan_arguments = plan_arguments_from_plan_parameters(
                plan_state,
                &object_field.parameters,
                arguments,
            )?;
            Ok(plan::ComparisonTarget::Column {
                name,
                arguments: plan_arguments,
                field_path,
                field_type: object_field.r#type,
            })
        }
        ndc::ComparisonTarget::Aggregate { .. } => {
            // TODO: ENG-1457 implement query.aggregates.filter_by
            Err(QueryPlanError::NotImplemented(
                "filter by aggregate".to_string(),
            ))
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
        ndc::ComparisonValue::Column {
            path,
            name,
            arguments,
            field_path,
            scope,
        } => {
            let (plan_path, collection_object_type) = plan_for_relationship_path(
                plan_state,
                root_collection_object_type,
                object_type,
                path,
                vec![name.clone()],
            )?;
            let object_field = collection_object_type.get(&name)?;
            let plan_arguments = plan_arguments_from_plan_parameters(
                plan_state,
                &object_field.parameters,
                arguments,
            )?;
            Ok(plan::ComparisonValue::Column {
                path: plan_path,
                name,
                arguments: plan_arguments,
                field_path,
                scope,
            })
        }
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
            field_path,
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

            // TODO: ENG-1457 When we implement query.aggregates.filter_by we'll need to collect aggregates
            // here as well as fields.
            let fields = predicate.as_ref().map(|p| {
                let mut fields = IndexMap::new();
                for comparison_target in p.query_local_comparison_targets() {
                    match comparison_target {
                        plan::ComparisonTarget::Column {
                            name,
                            arguments: _,
                            field_type,
                            ..
                        } => fields.insert(
                            name.clone(),
                            plan::Field::Column {
                                column: name.clone(),
                                fields: None,
                                column_type: field_type.clone(),
                            },
                        ),
                    };
                }
                fields
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

            Ok((in_collection, predicate))
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

            let join_key = plan_state.register_unrelated_join(collection, arguments, join_query)?;

            let in_collection = plan::ExistsInCollection::Unrelated {
                unrelated_collection: join_key,
            };
            Ok((in_collection, predicate))
        }
        ndc::ExistsInCollection::NestedCollection {
            column_name,
            arguments,
            field_path,
        } => {
            let arguments = if arguments.is_empty() {
                Default::default()
            } else {
                Err(QueryPlanError::NotImplemented(
                    "arguments on nested fields".to_string(),
                ))?
            };

            // TODO: To support field arguments here we need a way to look up field parameters (a map of
            // supported argument names to types). When we have that replace the above `arguments`
            // assignment with this one:
            // let arguments = plan_for_arguments(plan_state, parameters, arguments)?;

            let nested_collection_type = find_nested_collection_type(
                root_collection_object_type.clone(),
                &field_path
                    .clone()
                    .into_iter()
                    .chain(once(column_name.clone()))
                    .collect_vec(),
            )?;

            let in_collection = plan::ExistsInCollection::NestedCollection {
                column_name,
                arguments,
                field_path,
            };

            let predicate = predicate
                .map(|expression| {
                    plan_for_expression(
                        &mut nested_state,
                        root_collection_object_type,
                        &nested_collection_type,
                        *expression,
                    )
                })
                .transpose()?;

            Ok((in_collection, predicate))
        }
        ExistsInCollection::NestedScalarCollection {
            column_name,
            arguments,
            field_path,
        } => todo!(),
    }?;

    Ok(plan::Expression::Exists {
        in_collection,
        predicate: predicate.map(Box::new),
    })
}
