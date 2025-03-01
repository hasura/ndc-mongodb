use std::iter::once;

use indexmap::IndexMap;
use itertools::Itertools as _;
use ndc_models::{self as ndc, ExistsInCollection};

use crate::{self as plan, QueryContext, QueryPlanError};

use super::{
    helpers::{
        find_nested_collection_object_type, find_nested_collection_type,
        get_object_field_by_path, lookup_relationship,
    },
    plan_for_arguments::plan_arguments_from_plan_parameters,
    plan_for_relationship::plan_for_relationship_path,
    query_plan_state::QueryPlanState,
};

type Result<T> = std::result::Result<T, QueryPlanError>;

pub fn plan_for_expression<T: QueryContext>(
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
                column: plan_for_comparison_target(plan_state, object_type, column)?,
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
    let comparison_target = plan_for_comparison_target(plan_state, object_type, column)?;
    let (operator, operator_definition) = plan_state
        .context
        .find_comparison_operator(comparison_target.target_type(), &operator)?;
    let value_type = operator_definition.argument_type(comparison_target.target_type());
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
    let comparison_target = plan_for_comparison_target(plan_state, object_type, column)?;
    let plan_comparison = match comparison {
        ndc::ArrayComparison::Contains { value } => {
            let array_element_type = comparison_target
                .target_type()
                .clone()
                .into_array_element_type()?;
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
    object_type: &plan::ObjectType<T::ScalarType>,
    target: ndc::ComparisonTarget,
) -> Result<plan::ComparisonTarget<T>> {
    match target {
        ndc::ComparisonTarget::Column {
            name,
            arguments,
            field_path,
        } => {
            let object_field =
                get_object_field_by_path(object_type, &name, field_path.as_deref())?.clone();
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
                field_type: object_field.r#type.clone(),
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
            field_path: _, // TODO: ENG-1490 requires propagating this, probably through the `register_relationship` call
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
                    match comparison_target.into_owned() {
                        plan::ComparisonTarget::Column {
                            name,
                            arguments: _,
                            field_type,
                            ..
                        } => fields.insert(
                            name.clone(),
                            plan::Field::Column {
                                column: name,
                                fields: None,
                                column_type: field_type,
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
            let object_field = root_collection_object_type.get(&column_name)?;
            let plan_arguments = plan_arguments_from_plan_parameters(
                &mut nested_state,
                &object_field.parameters,
                arguments,
            )?;

            let nested_collection_type = find_nested_collection_object_type(
                root_collection_object_type.clone(),
                &field_path
                    .clone()
                    .into_iter()
                    .chain(once(column_name.clone()))
                    .collect_vec(),
            )?;

            let in_collection = plan::ExistsInCollection::NestedCollection {
                column_name,
                arguments: plan_arguments,
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
        } => {
            let object_field = root_collection_object_type.get(&column_name)?;
            let plan_arguments = plan_arguments_from_plan_parameters(
                &mut nested_state,
                &object_field.parameters,
                arguments,
            )?;

            let nested_collection_type = find_nested_collection_type(
                root_collection_object_type.clone(),
                &field_path
                    .clone()
                    .into_iter()
                    .chain(once(column_name.clone()))
                    .collect_vec(),
            )?;

            let virtual_object_type = plan::ObjectType {
                name: None,
                fields: [(
                    "__value".into(),
                    plan::ObjectField {
                        r#type: nested_collection_type,
                        parameters: Default::default(),
                    },
                )]
                .into(),
            };

            let in_collection = plan::ExistsInCollection::NestedScalarCollection {
                column_name,
                arguments: plan_arguments,
                field_path,
            };

            let predicate = predicate
                .map(|expression| {
                    plan_for_expression(
                        &mut nested_state,
                        root_collection_object_type,
                        &virtual_object_type,
                        *expression,
                    )
                })
                .transpose()?;

            Ok((in_collection, predicate))
        }
    }?;

    Ok(plan::Expression::Exists {
        in_collection,
        predicate: predicate.map(Box::new),
    })
}
