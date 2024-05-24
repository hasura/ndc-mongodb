mod helpers;
pub mod query_context;
pub mod query_plan_error;
mod query_plan_state;
pub mod type_annotated_field;

#[cfg(test)]
mod plan_test_helpers;

use std::collections::VecDeque;

use crate::{self as plan, type_annotated_field, ObjectType, QueryPlan};
use indexmap::IndexMap;
use itertools::Itertools as _;
use ndc::QueryRequest;
use ndc_models as ndc;

use self::{
    helpers::{find_object_field, lookup_relationship},
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
    let collection_object_type = context.find_collection_object_type(&request.collection)?;

    let query = plan_for_query(
        &mut plan_state,
        &collection_object_type,
        &collection_object_type,
        request.query,
    )?;

    let unrelated_collections = plan_state.into_unrelated_collections();

    Ok(QueryPlan {
        collection: request.collection,
        arguments: request.arguments,
        query,
        variables: request.variables,
        unrelated_collections,
    })
}

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
    })
}

fn plan_for_aggregates<T: QueryContext>(
    context: &T,
    collection_object_type: &plan::ObjectType<T::ScalarType>,
    ndc_aggregates: Option<IndexMap<String, ndc::Aggregate>>,
) -> Result<Option<IndexMap<String, plan::Aggregate<T>>>> {
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
        ndc::Aggregate::ColumnCount { column, distinct } => {
            Ok(plan::Aggregate::ColumnCount { column, distinct })
        }
        ndc::Aggregate::SingleColumn { column, function } => {
            let object_type_field_type =
                find_object_field(collection_object_type, column.as_ref())?;
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
    ndc_fields: Option<IndexMap<String, ndc::Field>>,
) -> Result<Option<IndexMap<String, plan::Field<T>>>> {
    let plan_fields: Option<IndexMap<String, plan::Field<T>>> = ndc_fields
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
        ndc::OrderByTarget::Column { name, path } => plan::OrderByTarget::Column {
            name,
            field_path: Default::default(), // TODO: propagate this after ndc-spec update
            path: plan_for_relationship_path(
                plan_state,
                root_collection_object_type,
                object_type,
                path,
            )?
            .0,
        },
        ndc::OrderByTarget::SingleColumnAggregate {
            column,
            function,
            path,
        } => {
            let (plan_path, target_object_type) = plan_for_relationship_path(
                plan_state,
                root_collection_object_type,
                object_type,
                path,
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
) -> Result<(Vec<String>, ObjectType<T::ScalarType>)> {
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

    let vec_deque = plan_for_relationship_path_helper(
        plan_state,
        root_collection_object_type,
        relationship_path,
    )?;
    let aliases = vec_deque.into_iter().collect();

    Ok((aliases, target_object_type))
}

fn plan_for_relationship_path_helper<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    root_collection_object_type: &plan::ObjectType<T::ScalarType>,
    relationship_path: impl IntoIterator<Item = ndc::PathElement>,
) -> Result<VecDeque<String>> {
    let (head, tail) = {
        let mut path_iter = relationship_path.into_iter();
        let head = path_iter.next();
        (head, path_iter)
    };
    if let Some(ndc::PathElement {
        relationship,
        arguments,
        predicate,
    }) = head
    {
        let relationship_def =
            lookup_relationship(plan_state.collection_relationships, &relationship)?;
        let related_collection_type = plan_state
            .context
            .find_collection_object_type(&relationship_def.target_collection)?;
        let mut nested_state = plan_state.state_for_subquery();

        let mut rest_path = plan_for_relationship_path_helper(
            &mut nested_state,
            root_collection_object_type,
            tail,
        )?;

        let nested_relationships = nested_state.into_relationships();

        let relationship_query = plan::Query {
            predicate: predicate
                .map(|p| {
                    plan_for_expression(
                        plan_state,
                        root_collection_object_type,
                        &related_collection_type,
                        *p,
                    )
                })
                .transpose()?,
            relationships: nested_relationships,
            ..Default::default()
        };

        let (relation_key, _) =
            plan_state.register_relationship(relationship, arguments, relationship_query)?;

        rest_path.push_front(relation_key.to_owned());
        Ok(rest_path)
    } else {
        Ok(VecDeque::new())
    }
}

// fn ndc_to_v2_order_by_element(
//     context: &QueryContext,
//     collection_relationships: &BTreeMap<String, ndc::Relationship>,
//     root_collection_object_type: &WithNameRef<plan::ObjectType>,
//     object_type: &WithNameRef<plan::ObjectType>,
//     elem: ndc::OrderByElement,
// ) -> Result<(v2::OrderByElement, HashMap<String, v2::OrderByRelation>)> {
//     let (target, target_path) = match elem.target {
//         ndc::OrderByTarget::Column { name, path } => (
//             v2::OrderByTarget::Column {
//                 column: v2::ColumnSelector::Column(name),
//             },
//             path,
//         ),
//         ndc::OrderByTarget::SingleColumnAggregate {
//             column,
//             function,
//             path,
//         } => {
//             let end_of_relationship_path_object_type = path
//                 .last()
//                 .map(|last_path_element| {
//                     let relationship = lookup_relationship(
//                         collection_relationships,
//                         &last_path_element.relationship,
//                     )?;
//                     context.find_collection_object_type(&relationship.target_collection)
//                 })
//                 .transpose()?;
//             let target_object_type = end_of_relationship_path_object_type
//                 .as_ref()
//                 .unwrap_or(object_type);
//             let object_field = find_object_field(target_object_type, &column)?;
//             let scalar_type_name = get_scalar_type_name(&object_field.r#type)?;
//             let aggregate_function =
//                 context.find_aggregation_function_definition(&scalar_type_name, &function)?;
//             let result_type = type_to_type_name(&aggregate_function.result_type)?;
//             let target = v2::OrderByTarget::SingleColumnAggregate {
//                 column,
//                 function,
//                 result_type,
//             };
//             (target, path)
//         }
//         ndc::OrderByTarget::StarCountAggregate { path } => {
//             (v2::OrderByTarget::StarCountAggregate {}, path)
//         }
//     };
//     let (target_path, relations) = ndc_to_v2_target_path(
//         context,
//         collection_relationships,
//         root_collection_object_type,
//         target_path,
//     )?;
//     let order_by_element = v2::OrderByElement {
//         order_direction: match elem.order_direction {
//             ndc::OrderDirection::Asc => v2::OrderDirection::Asc,
//             ndc::OrderDirection::Desc => v2::OrderDirection::Desc,
//         },
//         target,
//         target_path,
//     };
//     Ok((order_by_element, relations))
// }
//
// fn ndc_to_v2_target_path(
//     context: &QueryContext,
//     collection_relationships: &BTreeMap<String, ndc::Relationship>,
//     root_collection_object_type: &WithNameRef<plan::ObjectType>,
//     path: Vec<ndc::PathElement>,
// ) -> Result<(Vec<String>, HashMap<String, v2::OrderByRelation>)> {
//     let mut v2_path = vec![];
//     let v2_relations = ndc_to_v2_target_path_step::<Vec<_>>(
//         context,
//         collection_relationships,
//         root_collection_object_type,
//         path.into_iter(),
//         &mut v2_path,
//     )?;
//     Ok((v2_path, v2_relations))
// }
//
// fn ndc_to_v2_target_path_step<T: IntoIterator<Item = ndc::PathElement>>(
//     context: &QueryContext,
//     collection_relationships: &BTreeMap<String, ndc::Relationship>,
//     root_collection_object_type: &WithNameRef<plan::ObjectType>,
//     mut path_iter: T::IntoIter,
//     v2_path: &mut Vec<String>,
// ) -> Result<HashMap<String, v2::OrderByRelation>> {
//     let mut v2_relations = HashMap::new();
//
//     if let Some(path_element) = path_iter.next() {
//         v2_path.push(path_element.relationship.clone());
//
//         let where_expr = path_element
//             .predicate
//             .map(|expression| {
//                 let ndc_relationship =
//                     lookup_relationship(collection_relationships, &path_element.relationship)?;
//                 let target_object_type =
//                     context.find_collection_object_type(&ndc_relationship.target_collection)?;
//                 let v2_expression = ndc_to_v2_expression(
//                     context,
//                     collection_relationships,
//                     root_collection_object_type,
//                     &target_object_type,
//                     *expression,
//                 )?;
//                 Ok(Box::new(v2_expression))
//             })
//             .transpose()?;
//
//         let subrelations = ndc_to_v2_target_path_step::<T>(
//             context,
//             collection_relationships,
//             root_collection_object_type,
//             path_iter,
//             v2_path,
//         )?;
//
//         v2_relations.insert(
//             path_element.relationship,
//             v2::OrderByRelation {
//                 r#where: where_expr,
//                 subrelations,
//             },
//         );
//     }
//
//     Ok(v2_relations)
// }
//
// /// Like v2, a ndc QueryRequest has a map of Relationships. Unlike v2, ndc does not indicate the
// /// source collection for each relationship. Instead we are supposed to keep track of the "current"
// /// collection so that when we hit a Field that refers to a Relationship we infer that the source
// /// is the "current" collection. This means that to produce a v2 Relationship mapping we need to
// /// traverse the query here.
// fn ndc_to_v2_relationships(
//     query_request: &ndc::QueryRequest,
// ) -> Result<Vec<v2::TableRelationships>> {
//     // This only captures relationships that are referenced by a Field or an OrderBy in the query.
//     // We might record a relationship more than once, but we are recording to maps so that doesn't
//     // matter. We might capture the same relationship multiple times with different source
//     // collections, but that is by design.
//     let relationships_by_source_and_name: Vec<(Vec<String>, (String, v2::Relationship))> =
//         query_traversal(query_request)
//             .filter_map_ok(|TraversalStep { collection, node }| match node {
//                 Node::Field {
//                     field:
//                         ndc::Field::Relationship {
//                             relationship,
//                             arguments,
//                             ..
//                         },
//                     ..
//                 } => Some((collection, relationship, arguments)),
//                 Node::ExistsInCollection(ndc::ExistsInCollection::Related {
//                     relationship,
//                     arguments,
//                 }) => Some((collection, relationship, arguments)),
//                 Node::PathElement(ndc::PathElement {
//                     relationship,
//                     arguments,
//                     ..
//                 }) => Some((collection, relationship, arguments)),
//                 _ => None,
//             })
//             .map_ok(|(collection_name, relationship_name, arguments)| {
//                 let ndc_relationship = lookup_relationship(
//                     &query_request.collection_relationships,
//                     relationship_name,
//                 )?;
//
//                 // TODO: Functions (native queries) may be referenced multiple times in a query
//                 // request with different arguments. To accommodate that we will need to record
//                 // separate v2 relations for each reference with different names. In the current
//                 // implementation one set of arguments will override arguments to all occurrences of
//                 // a given function. MDB-106
//                 let v2_relationship = v2::Relationship {
//                     column_mapping: v2::ColumnMapping(
//                         ndc_relationship
//                             .column_mapping
//                             .iter()
//                             .map(|(source_col, target_col)| {
//                                 (
//                                     ColumnSelector::Column(source_col.clone()),
//                                     ColumnSelector::Column(target_col.clone()),
//                                 )
//                             })
//                             .collect(),
//                     ),
//                     relationship_type: match ndc_relationship.relationship_type {
//                         ndc::RelationshipType::Object => v2::RelationshipType::Object,
//                         ndc::RelationshipType::Array => v2::RelationshipType::Array,
//                     },
//                     target: v2::Target::TTable {
//                         name: vec![ndc_relationship.target_collection.clone()],
//                         arguments: ndc_to_v2_relationship_arguments(arguments.clone()),
//                     },
//                 };
//
//                 Ok((
//                     vec![collection_name.to_owned()], // put in vec to match v2 namespaced format
//                     (relationship_name.clone(), v2_relationship),
//                 )) as Result<_>
//             })
//             // The previous step produced Result<Result<_>,_> values. Flatten them to Result<_,_>.
//             // We can't use the flatten() Iterator method because that loses the outer Result errors.
//             .map(|result| match result {
//                 Ok(Ok(v)) => Ok(v),
//                 Ok(Err(e)) => Err(e),
//                 Err(e) => Err(e),
//             })
//             .collect::<Result<_, _>>()?;
//
//     let grouped_by_source: HashMap<Vec<String>, Vec<(String, v2::Relationship)>> =
//         relationships_by_source_and_name
//             .into_iter()
//             .into_group_map();
//
//     let v2_relationships = grouped_by_source
//         .into_iter()
//         .map(|(source_table, relationships)| v2::TableRelationships {
//             source_table,
//             relationships: relationships.into_iter().collect(),
//         })
//         .collect();
//
//     Ok(v2_relationships)
// }
//
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
                operator: match operator {
                    ndc::UnaryComparisonOperator::IsNull => ndc::UnaryComparisonOperator::IsNull,
                },
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
        } => {
            let mut nested_state = plan_state.state_for_subquery();

            let in_collection = match in_collection {
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

                    let relationship_query = plan::Query {
                        limit: Some(1),
                        predicate,
                        relationships: nested_state.into_relationships(),
                        ..Default::default()
                    };

                    let (relationship_key, _) = plan_state.register_relationship(
                        relationship,
                        arguments,
                        relationship_query,
                    )?;

                    let in_collection = plan::ExistsInCollection::Related {
                        relationship: relationship_key.to_owned(),
                    };

                    Ok(in_collection)
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
                        limit: Some(1),
                        predicate,
                        relationships: nested_state.into_relationships(),
                        ..Default::default()
                    };

                    let join_key =
                        plan_state.register_unrelated_join(collection, arguments, join_query);

                    let in_collection = plan::ExistsInCollection::Unrelated {
                        unrelated_collection: join_key,
                    };
                    Ok(in_collection)
                }
            }?;

            Ok(plan::Expression::Exists { in_collection })
        }
    }
}

fn plan_for_binary_comparison<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    root_collection_object_type: &plan::ObjectType<T::ScalarType>,
    object_type: &plan::ObjectType<T::ScalarType>,
    column: ndc::ComparisonTarget,
    operator: String,
    value: ndc::ComparisonValue,
) -> Result<plan::Expression<T>> {
    let comparison_target =
        plan_for_comparison_target(plan_state, root_collection_object_type, object_type, column)?;
    let (operator, operator_definition) = plan_state
        .context
        .find_comparison_operator(comparison_target.get_column_type(), &operator)?;
    let value_type = match operator_definition {
        plan::ComparisonOperatorDefinition::Equal => comparison_target.get_column_type().clone(),
        plan::ComparisonOperatorDefinition::In => {
            plan::Type::ArrayOf(Box::new(comparison_target.get_column_type().clone()))
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
        ndc::ComparisonTarget::Column { name, path } => {
            let (path, target_object_type) = plan_for_relationship_path(
                plan_state,
                root_collection_object_type,
                object_type,
                path,
            )?;
            let column_type = find_object_field(&target_object_type, &name)?.clone();
            Ok(plan::ComparisonTarget::Column {
                name,
                field_path: Default::default(), // TODO: propagate this after ndc-spec update
                path,
                column_type,
            })
        }
        ndc::ComparisonTarget::RootCollectionColumn { name } => {
            let column_type = find_object_field(root_collection_object_type, &name)?.clone();
            Ok(plan::ComparisonTarget::RootCollectionColumn {
                name,
                field_path: Default::default(), // TODO: propagate this after ndc-spec update
                column_type,
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
        ndc::ComparisonValue::Variable { name } => Ok(plan::ComparisonValue::Variable {
            name,
            variable_type: expected_type,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use ndc_models::{
        self as ndc, CollectionInfo, OrderByTarget, OrderDirection, RelationshipType,
    };
    use ndc_test_helpers::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use crate::{
        self as plan, inline_object_types,
        plan_for_query_request::plan_test_helpers::{
            self, make_flat_schema, make_nested_schema, TestContext,
        },
        query_plan::UnrelatedJoin,
        ExistsInCollection, Expression, Field, OrderBy, Query, QueryContext, QueryPlan,
        Relationship,
    };

    use super::plan_for_query_request;

    #[test]
    fn translates_query_request_relationships() -> Result<(), anyhow::Error> {
        let request = query_request()
            .collection("schools")
            .relationships([
                (
                    "school_classes",
                    relationship("classes", [("_id", "school_id")]),
                ),
                (
                    "class_students",
                    relationship("students", [("_id", "class_id")]),
                ),
                (
                    "class_department",
                    relationship("departments", [("department_id", "_id")]).object_type(),
                ),
                (
                    "school_directory",
                    relationship("directory", [("_id", "school_id")]).object_type(),
                ),
                (
                    "student_advisor",
                    relationship("advisors", [("advisor_id", "_id")]).object_type(),
                ),
                (
                    "existence_check",
                    relationship("some_collection", [("some_id", "_id")]),
                ),
            ])
            .query(
                query()
                    .fields([relation_field!("school_classes" => "class_name", query()
                        .fields([
                            relation_field!("class_students" => "student_name")
                        ])
                    )])
                    .order_by(vec![ndc::OrderByElement {
                        order_direction: OrderDirection::Asc,
                        target: OrderByTarget::Column {
                            name: "advisor_name".to_owned(),
                            path: vec![
                                path_element("school_classes")
                                    .predicate(binop(
                                        "Equal",
                                        target!(
                                            "_id",
                                            [
                                                path_element("school_classes"),
                                                path_element("class_department"),
                                            ],
                                        ),
                                        column_value!(
                                            "math_department_id",
                                            [path_element("school_directory")],
                                        ),
                                    ))
                                    .into(),
                                path_element("class_students").into(),
                                path_element("student_advisor").into(),
                            ],
                        },
                    }])
                    // The `And` layer checks that we properly recursive into Expressions
                    .predicate(and([exists(
                        related!("existence_check"),
                        empty_expression(),
                    )])),
            )
            .into();

        let expected = QueryPlan {
            collection: "schools".to_owned(),
            arguments: Default::default(),
            variables: None,
            unrelated_collections: Default::default(),
            query: Query {
                predicate: Some(Expression::And {
                    expressions: vec![Expression::Exists {
                        in_collection: ExistsInCollection::Related {
                            relationship: "existence_check".into(),
                        },
                    }],
                }),
                order_by: Some(OrderBy {
                    elements: [plan::OrderByElement {
                        order_direction: OrderDirection::Asc,
                        target: plan::OrderByTarget::Column {
                            name: "advisor_name".into(),
                            field_path: Default::default(),
                            path: [
                                "school_classes".into(),
                                "class_students".into(),
                                "student_advisor".into(),
                            ]
                            .into(),
                        },
                    }]
                    .into(),
                }),
                relationships: [
                    (
                        "school_classes".to_owned(),
                        Relationship {
                            column_mapping: [("_id".to_owned(), "school_id".to_owned())].into(),
                            relationship_type: RelationshipType::Array,
                            target_collection: "classes".to_owned(),
                            arguments: Default::default(),
                            query: Query {
                                fields: Some(
                                    [(
                                        "student_name".into(),
                                        plan::Field::Relationship {
                                            relationship: "class_students".into(),
                                            aggregates: None,
                                            fields: None,
                                        },
                                    )]
                                    .into(),
                                ),
                                relationships: [(
                                    "class_students".into(),
                                    plan::Relationship {
                                        target_collection: "students".into(),
                                        column_mapping: [("_id".into(), "class_id".into())].into(),
                                        relationship_type: RelationshipType::Array,
                                        arguments: Default::default(),
                                        query: Default::default(),
                                    },
                                )]
                                .into(),
                                ..Default::default()
                            },
                        },
                    ),
                    (
                        "school_directory".to_owned(),
                        Relationship {
                            target_collection: "directory".to_owned(),
                            column_mapping: [("_id".to_owned(), "school_id".to_owned())].into(),
                            relationship_type: RelationshipType::Object,
                            arguments: Default::default(),
                            query: Query {
                                ..Default::default()
                            },
                        },
                    ),
                    (
                        "existence_check".to_owned(),
                        Relationship {
                            column_mapping: [("some_id".to_owned(), "_id".to_owned())].into(),
                            relationship_type: RelationshipType::Array,
                            target_collection: "some_collection".to_owned(),
                            arguments: Default::default(),
                            query: Query {
                                predicate: Some(plan::Expression::Or {
                                    expressions: vec![],
                                }),
                                limit: Some(1),
                                ..Default::default()
                            },
                        },
                    ),
                ]
                .into(),
                fields: Some(
                    [(
                        "class_name".into(),
                        Field::Relationship {
                            relationship: "school_classes".into(),
                            aggregates: None,
                            fields: Some(
                                [(
                                    "student_name".into(),
                                    Field::Relationship {
                                        relationship: "class_students".into(),
                                        aggregates: None,
                                        fields: None,
                                    },
                                )]
                                .into(),
                            ),
                        },
                    )]
                    .into(),
                ),
                ..Default::default()
            },
        };

        let context = TestContext {
            collections: [
                collection("schools"),
                collection("classes"),
                collection("students"),
                collection("departments"),
                collection("directory"),
                collection("advisors"),
                collection("some_collection"),
            ]
            .into(),
            object_types: [
                (
                    "schools".to_owned(),
                    object_type([("_id", named_type("Int"))]),
                ),
                (
                    "classes".to_owned(),
                    object_type([
                        ("_id", named_type("Int")),
                        ("school_id", named_type("Int")),
                        ("department_id", named_type("Int")),
                    ]),
                ),
                (
                    "students".to_owned(),
                    object_type([
                        ("_id", named_type("Int")),
                        ("class_id", named_type("Int")),
                        ("advisor_id", named_type("Int")),
                        ("student_name", named_type("String")),
                    ]),
                ),
                (
                    "departments".to_owned(),
                    object_type([("_id", named_type("Int"))]),
                ),
                (
                    "directory".to_owned(),
                    object_type([
                        ("_id", named_type("Int")),
                        ("school_id", named_type("Int")),
                        ("math_department_id", named_type("Int")),
                    ]),
                ),
                (
                    "advisors".to_owned(),
                    object_type([
                        ("_id", named_type("Int")),
                        ("advisor_name", named_type("String")),
                    ]),
                ),
                (
                    "some_collection".to_owned(),
                    object_type([("_id", named_type("Int")), ("some_id", named_type("Int"))]),
                ),
            ]
            .into(),
            ..Default::default()
        };

        let query_plan = plan_for_query_request(&context, request)?;

        assert_eq!(query_plan, expected);
        Ok(())
    }

    #[test]
    fn translates_root_column_references() -> Result<(), anyhow::Error> {
        let query_context = make_flat_schema();
        let query = query_request()
            .collection("authors")
            .query(query().fields([field!("last_name")]).predicate(exists(
                unrelated!("articles"),
                and([
                    binop("Equal", target!("author_id"), column_value!(root("id"))),
                    binop("Regex", target!("title"), value!("Functional.*")),
                ]),
            )))
            .into();
        let query_plan = plan_for_query_request(&query_context, query)?;

        let expected = QueryPlan {
            collection: "authors".into(),
            query: plan::Query {
                predicate: Some(plan::Expression::Exists {
                    in_collection: plan::ExistsInCollection::Unrelated {
                        unrelated_collection: "__join_articles_0".into(),
                    },
                }),
                fields: Some(
                    [(
                        "last_name".into(),
                        plan::Field::Column {
                            column: "last_name".into(),
                            fields: None,
                            column_type: plan::Type::Scalar(plan_test_helpers::ScalarType::String),
                        },
                    )]
                    .into(),
                ),
                ..Default::default()
            },
            unrelated_collections: [(
                "__join_articles_0".into(),
                UnrelatedJoin {
                    target_collection: "articles".into(),
                    arguments: Default::default(),
                    query: plan::Query {
                        limit: Some(1),
                        predicate: Some(plan::Expression::And {
                            expressions: vec![
                                plan::Expression::BinaryComparisonOperator {
                                    column: plan::ComparisonTarget::Column {
                                        name: "author_id".into(),
                                        column_type: plan::Type::Scalar(
                                            plan_test_helpers::ScalarType::Int,
                                        ),
                                        field_path: None,
                                        path: vec![],
                                    },
                                    operator: plan_test_helpers::ComparisonOperator::Equal,
                                    value: plan::ComparisonValue::Column {
                                        column: plan::ComparisonTarget::RootCollectionColumn {
                                            name: "id".into(),
                                            column_type: plan::Type::Scalar(
                                                plan_test_helpers::ScalarType::Int,
                                            ),
                                            field_path: None,
                                        },
                                    },
                                },
                                plan::Expression::BinaryComparisonOperator {
                                    column: plan::ComparisonTarget::Column {
                                        name: "title".into(),
                                        column_type: plan::Type::Scalar(
                                            plan_test_helpers::ScalarType::String,
                                        ),
                                        field_path: None,
                                        path: vec![],
                                    },
                                    operator: plan_test_helpers::ComparisonOperator::Regex,
                                    value: plan::ComparisonValue::Scalar {
                                        value: "Functional.*".into(),
                                        value_type: plan::Type::Scalar(
                                            plan_test_helpers::ScalarType::String,
                                        ),
                                    },
                                },
                            ],
                        }),
                        ..Default::default()
                    },
                },
            )]
            .into(),
            arguments: Default::default(),
            variables: Default::default(),
        };

        assert_eq!(query_plan, expected);
        Ok(())
    }

    #[test]
    fn translates_aggregate_selections() -> Result<(), anyhow::Error> {
        let query_context = make_flat_schema();
        let query = query_request()
            .collection("authors")
            .query(query().aggregates([
                star_count_aggregate!("count_star"),
                column_count_aggregate!("count_id" => "last_name", distinct: true),
                column_aggregate!("avg_id" => "id", "Average"),
            ]))
            .into();
        let query_plan = plan_for_query_request(&query_context, query)?;

        let expected = QueryPlan {
            collection: "authors".into(),
            query: plan::Query {
                aggregates: Some(
                    [
                        ("count_star".into(), plan::Aggregate::StarCount),
                        (
                            "count_id".into(),
                            plan::Aggregate::ColumnCount {
                                column: "last_name".into(),
                                distinct: true,
                            },
                        ),
                        (
                            "avg_id".into(),
                            plan::Aggregate::SingleColumn {
                                column: "id".into(),
                                function: plan_test_helpers::AggregateFunction::Average,
                                result_type: plan::Type::Scalar(
                                    plan_test_helpers::ScalarType::Double,
                                ),
                            },
                        ),
                    ]
                    .into(),
                ),
                ..Default::default()
            },
            arguments: Default::default(),
            variables: Default::default(),
            unrelated_collections: Default::default(),
        };

        assert_eq!(query_plan, expected);
        Ok(())
    }

    #[test]
    fn translates_relationships_in_fields_predicates_and_orderings() -> Result<(), anyhow::Error> {
        let query_context = make_flat_schema();
        let query = query_request()
            .collection("authors")
            .query(
                query()
                    .fields([
                        field!("last_name"),
                        relation_field!(
                            "author_articles" => "articles",
                            query().fields([field!("title"), field!("year")])
                        ),
                    ])
                    .predicate(exists(
                        related!("author_articles"),
                        binop("Regex", target!("title"), value!("Functional.*")),
                    ))
                    .order_by(vec![
                        ndc::OrderByElement {
                            order_direction: OrderDirection::Asc,
                            target: OrderByTarget::SingleColumnAggregate {
                                column: "year".into(),
                                function: "Average".into(),
                                path: vec![path_element("author_articles").into()],
                            },
                        },
                        ndc::OrderByElement {
                            order_direction: OrderDirection::Desc,
                            target: OrderByTarget::Column {
                                name: "id".into(),
                                path: vec![],
                            },
                        },
                    ]),
            )
            .relationships([(
                "author_articles",
                relationship("articles", [("id", "author_id")]),
            )])
            .into();
        let query_plan = plan_for_query_request(&query_context, query)?;

        let expected = QueryPlan {
            collection: "authors".into(),
            query: plan::Query {
                predicate: Some(plan::Expression::Exists {
                    in_collection: plan::ExistsInCollection::Related {
                        relationship: "author_articles".into(),
                    },
                }),
                order_by: Some(plan::OrderBy {
                    elements: vec![
                        plan::OrderByElement {
                            order_direction: OrderDirection::Asc,
                            target: plan::OrderByTarget::SingleColumnAggregate {
                                column: "year".into(),
                                function: plan_test_helpers::AggregateFunction::Average,
                                result_type: plan::Type::Scalar(
                                    plan_test_helpers::ScalarType::Double,
                                ),
                                path: vec!["author_articles".into()],
                            },
                        },
                        plan::OrderByElement {
                            order_direction: OrderDirection::Desc,
                            target: plan::OrderByTarget::Column {
                                name: "id".into(),
                                field_path: None,
                                path: vec![],
                            },
                        },
                    ],
                }),
                fields: Some(
                    [
                        (
                            "last_name".into(),
                            plan::Field::Column {
                                column: "last_name".into(),
                                column_type: plan::Type::Scalar(
                                    plan_test_helpers::ScalarType::String,
                                ),
                                fields: None,
                            },
                        ),
                        (
                            "articles".into(),
                            plan::Field::Relationship {
                                relationship: "author_articles".into(),
                                aggregates: None,
                                fields: Some(
                                    [
                                        (
                                            "title".into(),
                                            plan::Field::Column {
                                                column: "title".into(),
                                                column_type: plan::Type::Scalar(
                                                    plan_test_helpers::ScalarType::String,
                                                ),
                                                fields: None,
                                            },
                                        ),
                                        (
                                            "year".into(),
                                            plan::Field::Column {
                                                column: "year".into(),
                                                column_type: plan::Type::Nullable(Box::new(
                                                    plan::Type::Scalar(
                                                        plan_test_helpers::ScalarType::Int,
                                                    ),
                                                )),
                                                fields: None,
                                            },
                                        ),
                                    ]
                                    .into(),
                                ),
                            },
                        ),
                    ]
                    .into(),
                ),
                relationships: [(
                    "author_articles".into(),
                    plan::Relationship {
                        target_collection: "articles".into(),
                        column_mapping: [("id".into(), "author_id".into())].into(),
                        relationship_type: RelationshipType::Array,
                        arguments: Default::default(),
                        query: plan::Query {
                            fields: Some(
                                [
                                    (
                                        "title".into(),
                                        plan::Field::Column {
                                            column: "title".into(),
                                            column_type: plan::Type::Scalar(
                                                plan_test_helpers::ScalarType::String,
                                            ),
                                            fields: None,
                                        },
                                    ),
                                    (
                                        "year".into(),
                                        plan::Field::Column {
                                            column: "year".into(),
                                            column_type: plan::Type::Nullable(Box::new(
                                                plan::Type::Scalar(
                                                    plan_test_helpers::ScalarType::Int,
                                                ),
                                            )),
                                            fields: None,
                                        },
                                    ),
                                ]
                                .into(),
                            ),
                            ..Default::default()
                        },
                    },
                )]
                .into(),
                ..Default::default()
            },
            arguments: Default::default(),
            variables: Default::default(),
            unrelated_collections: Default::default(),
        };

        assert_eq!(query_plan, expected);
        Ok(())
    }

    #[test]
    fn translates_nested_fields() -> Result<(), anyhow::Error> {
        let query_context = make_nested_schema();
        let query_request = query_request()
            .collection("authors")
            .query(query().fields([
                field!("author_address" => "address", object!([field!("address_country" => "country")])),
                field!("author_articles" => "articles", array!(object!([field!("article_title" => "title")]))),
                field!("author_array_of_arrays" => "array_of_arrays", array!(array!(object!([field!("article_title" => "title")]))))
            ]))
            .into();
        let query_plan = plan_for_query_request(&query_context, query_request)?;

        let expected = QueryPlan {
            collection: "authors".into(),
            query: plan::Query {
                fields: Some(
                    [
                        (
                            "author_address".into(),
                            plan::Field::Column {
                                column: "address".into(),
                                column_type: plan::Type::Object(
                                    query_context.find_object_type("Address")?,
                                ),
                                fields: Some(plan::NestedField::Object(plan::NestedObject {
                                    fields: [(
                                        "address_country".into(),
                                        plan::Field::Column {
                                            column: "country".into(),
                                            column_type: plan::Type::Scalar(
                                                plan_test_helpers::ScalarType::String,
                                            ),
                                            fields: None,
                                        },
                                    )]
                                    .into(),
                                })),
                            },
                        ),
                        (
                            "author_articles".into(),
                            plan::Field::Column {
                                column: "articles".into(),
                                column_type: plan::Type::ArrayOf(Box::new(plan::Type::Object(
                                    query_context.find_object_type("Article")?,
                                ))),
                                fields: Some(plan::NestedField::Array(plan::NestedArray {
                                    fields: Box::new(plan::NestedField::Object(
                                        plan::NestedObject {
                                            fields: [(
                                                "article_title".into(),
                                                plan::Field::Column {
                                                    column: "title".into(),
                                                    fields: None,
                                                    column_type: plan::Type::Scalar(
                                                        plan_test_helpers::ScalarType::String,
                                                    ),
                                                },
                                            )]
                                            .into(),
                                        },
                                    )),
                                })),
                            },
                        ),
                        (
                            "author_array_of_arrays".into(),
                            plan::Field::Column {
                                column: "array_of_arrays".into(),
                                fields: Some(plan::NestedField::Array(plan::NestedArray {
                                    fields: Box::new(plan::NestedField::Array(plan::NestedArray {
                                        fields: Box::new(plan::NestedField::Object(
                                            plan::NestedObject {
                                                fields: [(
                                                    "article_title".into(),
                                                    plan::Field::Column {
                                                        column: "title".into(),
                                                        fields: None,
                                                        column_type: plan::Type::Scalar(
                                                            plan_test_helpers::ScalarType::String,
                                                        ),
                                                    },
                                                )]
                                                .into(),
                                            },
                                        )),
                                    })),
                                })),
                                column_type: plan::Type::ArrayOf(Box::new(plan::Type::ArrayOf(
                                    Box::new(plan::Type::Object(
                                        query_context.find_object_type("Article")?,
                                    )),
                                ))),
                            },
                        ),
                    ]
                    .into(),
                ),
                ..Default::default()
            },
            arguments: Default::default(),
            variables: Default::default(),
            unrelated_collections: Default::default(),
        };

        assert_eq!(query_plan, expected);
        Ok(())
    }

    // #[test]
    // fn translates_predicate_referencing_field_of_related_collection() -> anyhow::Result<()> {
    //     let query_context = make_nested_schema();
    //     let request = query_request()
    //         .collection("appearances")
    //         .relationships([("author", relationship("authors", [("authorId", "id")]))])
    //         .query(
    //             query()
    //                 .fields([relation_field!("author" => "presenter", query().fields([
    //                     field!("name"),
    //                 ]))])
    //                 .predicate(not(is_null(target!("name", [path_element("author")])))),
    //         )
    //         .into();
    //     let v2_request = plan_for_query_request(&query_context, request)?;
    //
    //     let expected = v2::query_request()
    //         .target(["appearances"])
    //         .relationships([collection_relationships(
    //             vec!["appearances".into()],
    //             [(
    //                 "author",
    //                 v2::relationship(
    //                     v2::target("author"),
    //                     [(
    //                         dc_api_types::ColumnSelector::Column("authorId".into()),
    //                         dc_api_types::ColumnSelector::Column("id".into()),
    //                     )],
    //                 ),
    //             )],
    //         )])
    //         .query(v2::query().fields([
    //             v2::relation_field!("author" => "presenter", v2::query().fields([
    //                 v2::column!("name": "String")
    //             ])
    //             .predicate(v2::exists("author", v2::not(v2::is_null(v2::compare!("name": "String")))))),
    //         ])).into();
    //
    //     assert_eq!(v2_request, expected);
    //     Ok(())
    // }
}
