mod helpers;
pub mod query_context;
pub mod query_plan_error;
mod query_plan_state;
pub mod type_annotated_field;
mod unify_relationship_references;

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

                    let relationship_query = plan::Query {
                        predicate: predicate.clone(),
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

                    let join_key =
                        plan_state.register_unrelated_join(collection, arguments, join_query);

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
    use ndc_models::{self as ndc, OrderByTarget, OrderDirection, RelationshipType};
    use ndc_test_helpers::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use crate::{
        self as plan,
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
                    .fields([relation_field!("class_name" => "school_classes", query()
                        .fields([
                            relation_field!("student_name" => "class_students")
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
                                            relations: [
                                                path_element("school_classes"),
                                                path_element("class_department"),
                                            ],
                                        ),
                                        column_value!(
                                            "math_department_id",
                                            relations: [path_element("school_directory")],
                                        ),
                                    ))
                                    .into(),
                                path_element("class_students").into(),
                                path_element("student_advisor").into(),
                            ],
                        },
                    }])
                    // The `And` layer checks that we properly recursive into Expressions
                    .predicate(and([ndc::Expression::Exists {
                        in_collection: related!("existence_check"),
                        predicate: None,
                    }])),
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
                        predicate: None,
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
                                predicate: None,
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
                    predicate: Some(Box::new(plan::Expression::And {
                        expressions: vec![
                            plan::Expression::BinaryComparisonOperator {
                                column: plan::ComparisonTarget::Column {
                                    name: "author_id".into(),
                                    field_path: Default::default(),
                                    column_type: plan::Type::Scalar(
                                        plan_test_helpers::ScalarType::Int,
                                    ),
                                    path: Default::default(),
                                },
                                operator: plan_test_helpers::ComparisonOperator::Equal,
                                value: plan::ComparisonValue::Column {
                                    column: plan::ComparisonTarget::RootCollectionColumn {
                                        name: "id".into(),
                                        field_path: Default::default(),
                                        column_type: plan::Type::Scalar(
                                            plan_test_helpers::ScalarType::Int,
                                        ),
                                    },
                                },
                            },
                            plan::Expression::BinaryComparisonOperator {
                                column: plan::ComparisonTarget::Column {
                                    name: "title".into(),
                                    field_path: Default::default(),
                                    column_type: plan::Type::Scalar(
                                        plan_test_helpers::ScalarType::String,
                                    ),
                                    path: Default::default(),
                                },
                                operator: plan_test_helpers::ComparisonOperator::Regex,
                                value: plan::ComparisonValue::Scalar {
                                    value: json!("Functional.*"),
                                    value_type: plan::Type::Scalar(
                                        plan_test_helpers::ScalarType::String,
                                    ),
                                },
                            },
                        ],
                    })),
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
                            "articles" => "author_articles",
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
                    predicate: Some(Box::new(plan::Expression::BinaryComparisonOperator {
                        column: plan::ComparisonTarget::Column {
                            name: "title".into(),
                            field_path: Default::default(),
                            column_type: plan::Type::Scalar(plan_test_helpers::ScalarType::String),
                            path: Default::default(),
                        },
                        operator: plan_test_helpers::ComparisonOperator::Regex,
                        value: plan::ComparisonValue::Scalar {
                            value: "Functional.*".into(),
                            value_type: plan::Type::Scalar(plan_test_helpers::ScalarType::String),
                        },
                    })),
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

    #[test]
    fn translates_predicate_referencing_field_of_related_collection() -> anyhow::Result<()> {
        let query_context = make_nested_schema();
        let request = query_request()
            .collection("appearances")
            .relationships([("author", relationship("authors", [("authorId", "id")]))])
            .query(
                query()
                    .fields([relation_field!("presenter" => "author", query().fields([
                        field!("name"),
                    ]))])
                    .predicate(not(is_null(
                        target!("name", relations: [path_element("author")]),
                    ))),
            )
            .into();
        let query_plan = plan_for_query_request(&query_context, request)?;

        let expected = QueryPlan {
            collection: "appearances".into(),
            query: plan::Query {
                predicate: Some(plan::Expression::Not {
                    expression: Box::new(plan::Expression::UnaryComparisonOperator {
                        column: plan::ComparisonTarget::Column {
                            name: "name".into(),
                            field_path: None,
                            column_type: plan::Type::Scalar(plan_test_helpers::ScalarType::String),
                            path: vec!["author".into()],
                        },
                        operator: ndc_models::UnaryComparisonOperator::IsNull,
                    }),
                }),
                fields: Some(
                    [(
                        "presenter".into(),
                        plan::Field::Relationship {
                            relationship: "author".into(),
                            aggregates: None,
                            fields: Some(
                                [(
                                    "name".into(),
                                    plan::Field::Column {
                                        column: "name".into(),
                                        fields: None,
                                        column_type: plan::Type::Scalar(
                                            plan_test_helpers::ScalarType::String,
                                        ),
                                    },
                                )]
                                .into(),
                            ),
                        },
                    )]
                    .into(),
                ),
                relationships: [(
                    "author".into(),
                    plan::Relationship {
                        column_mapping: [("authorId".into(), "id".into())].into(),
                        relationship_type: RelationshipType::Array,
                        target_collection: "authors".into(),
                        arguments: Default::default(),
                        query: plan::Query {
                            fields: Some(
                                [(
                                    "name".into(),
                                    plan::Field::Column {
                                        column: "name".into(),
                                        fields: None,
                                        column_type: plan::Type::Scalar(
                                            plan_test_helpers::ScalarType::String,
                                        ),
                                    },
                                )]
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
}
