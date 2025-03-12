mod helpers;
mod plan_for_arguments;
mod plan_for_expression;
mod plan_for_grouping;
pub mod plan_for_mutation_request;
mod plan_for_relationship;
pub mod query_context;
pub mod query_plan_error;
mod query_plan_state;
pub mod type_annotated_field;
mod unify_relationship_references;

#[cfg(test)]
mod plan_test_helpers;
#[cfg(test)]
mod tests;

use crate::{self as plan, type_annotated_field, QueryPlan, Scope};
use indexmap::IndexMap;
use itertools::Itertools;
use ndc_models::{self as ndc, QueryRequest};
use plan_for_relationship::plan_for_relationship_path;
use query_plan_state::QueryPlanInfo;

use self::{
    helpers::{find_object_field, get_object_field_by_path},
    plan_for_arguments::{plan_arguments_from_plan_parameters, plan_for_arguments},
    plan_for_expression::plan_for_expression,
    plan_for_grouping::plan_for_grouping,
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

    let aggregates = query
        .aggregates
        .map(|aggregates| plan_for_aggregates(&mut plan_state, collection_object_type, aggregates))
        .transpose()?;
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

    let groups = query
        .groups
        .map(|grouping| {
            plan_for_grouping(
                &mut plan_state,
                root_collection_object_type,
                collection_object_type,
                grouping,
            )
        })
        .transpose()?;

    Ok(plan::Query {
        aggregates,
        fields,
        order_by,
        limit,
        offset,
        predicate,
        groups,
        relationships: plan_state.into_relationships(),
        scope: None,
    })
}

fn plan_for_aggregates<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    collection_object_type: &plan::ObjectType<T::ScalarType>,
    ndc_aggregates: IndexMap<ndc::FieldName, ndc::Aggregate>,
) -> Result<IndexMap<ndc::FieldName, plan::Aggregate<T>>> {
    ndc_aggregates
        .into_iter()
        .map(|(name, aggregate)| {
            Ok((
                name,
                plan_for_aggregate(plan_state, collection_object_type, aggregate)?,
            ))
        })
        .collect()
}

fn plan_for_aggregate<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
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
            let object_field = collection_object_type.get(&column)?;
            let plan_arguments = plan_arguments_from_plan_parameters(
                plan_state,
                &object_field.parameters,
                arguments,
            )?;
            Ok(plan::Aggregate::ColumnCount {
                column,
                arguments: plan_arguments,
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
            let nested_object_field =
                get_object_field_by_path(collection_object_type, &column, field_path.as_deref())?;
            let column_type = &nested_object_field.r#type;
            let object_field = collection_object_type.get(&column)?;
            let plan_arguments = plan_arguments_from_plan_parameters(
                plan_state,
                &object_field.parameters,
                arguments,
            )?;
            let (function, definition) = plan_state
                .context
                .find_aggregation_function_definition(column_type, &function)?;
            Ok(plan::Aggregate::SingleColumn {
                column,
                column_type: column_type.clone(),
                arguments: plan_arguments,
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
                vec![name.clone()],
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

            let object_field = find_object_field(&collection_object_type, &column)?;
            let column_type = &object_field.r#type;
            let (function, function_definition) = plan_state
                .context
                .find_aggregation_function_definition(column_type, &function)?;

            plan::OrderByTarget::Aggregate {
                path: plan_path,
                aggregate: plan::Aggregate::SingleColumn {
                    column,
                    column_type: column_type.clone(),
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
