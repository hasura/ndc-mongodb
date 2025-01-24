use ndc_models::{self as ndc};

use crate::{self as plan, ConnectorTypes, QueryContext, QueryPlanError};

use super::{
    helpers::get_object_field_by_path, plan_for_aggregate, plan_for_aggregates,
    plan_for_arguments::plan_arguments_from_plan_parameters,
    plan_for_relationship::plan_for_relationship_path, query_plan_state::QueryPlanState,
};

type Result<T> = std::result::Result<T, QueryPlanError>;

pub fn plan_for_grouping<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    root_collection_object_type: &plan::ObjectType<T::ScalarType>,
    collection_object_type: &plan::ObjectType<T::ScalarType>,
    grouping: ndc::Grouping,
) -> Result<plan::Grouping<T>> {
    let dimensions = grouping
        .dimensions
        .into_iter()
        .map(|d| {
            plan_for_dimension(
                plan_state,
                root_collection_object_type,
                collection_object_type,
                d,
            )
        })
        .collect::<Result<_>>()?;

    let aggregates = plan_for_aggregates(
        plan_state,
        collection_object_type,
        grouping
            .aggregates
            .into_iter()
            .map(|(key, aggregate)| (key.into(), aggregate))
            .collect(),
    )?;

    let predicate = grouping
        .predicate
        .map(|predicate| plan_for_group_expression(plan_state, collection_object_type, predicate))
        .transpose()?;

    let plan_grouping = plan::Grouping {
        dimensions,
        aggregates,
        predicate,
        order_by: (),
        limit: (),
        offset: (),
    };
    Ok(plan_grouping)
}

fn plan_for_dimension<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    root_collection_object_type: &plan::ObjectType<T::ScalarType>,
    collection_object_type: &plan::ObjectType<T::ScalarType>,
    dimension: ndc::Dimension,
) -> Result<plan::Dimension<T>> {
    let plan_dimension = match dimension {
        ndc_models::Dimension::Column {
            path,
            column_name,
            arguments,
            field_path,
        } => {
            let (relationship_path, collection_type) = plan_for_relationship_path(
                plan_state,
                root_collection_object_type,
                collection_object_type,
                path,
                vec![column_name.clone()],
            )?;

            let plan_arguments = plan_arguments_from_plan_parameters(
                plan_state,
                &collection_type.get(&column_name)?.parameters,
                arguments,
            )?;

            let object_field =
                get_object_field_by_path(&collection_type, &column_name, field_path.as_deref())?
                    .clone();

            plan::Dimension::Column {
                path: relationship_path,
                column_name,
                arguments: plan_arguments,
                field_path,
                field_type: object_field.r#type,
            }
        }
    };
    Ok(plan_dimension)
}

fn plan_for_group_expression<T: QueryContext>(
    plan_state: &mut QueryPlanState<T>,
    object_type: &plan::ObjectType<T::ScalarType>,
    expression: ndc::GroupExpression,
) -> Result<plan::GroupExpression<T>> {
    match expression {
        ndc::GroupExpression::And { expressions } => Ok(plan::GroupExpression::And {
            expressions: expressions
                .into_iter()
                .map(|expr| plan_for_group_expression(plan_state, object_type, expr))
                .collect::<Result<_>>()?,
        }),
        ndc::GroupExpression::Or { expressions } => Ok(plan::GroupExpression::Or {
            expressions: expressions
                .into_iter()
                .map(|expr| plan_for_group_expression(plan_state, object_type, expr))
                .collect::<Result<_>>()?,
        }),
        ndc::GroupExpression::Not { expression } => Ok(plan::GroupExpression::Not {
            expression: Box::new(plan_for_group_expression(
                plan_state,
                object_type,
                *expression,
            )?),
        }),
        ndc::GroupExpression::UnaryComparisonOperator { target, operator } => {
            Ok(plan::GroupExpression::UnaryComparisonOperator {
                target: plan_for_group_comparison_target(plan_state, object_type, target)?,
                operator,
            })
        }
        ndc::GroupExpression::BinaryComparisonOperator {
            target,
            operator,
            value,
        } => {
            let target = plan_for_group_comparison_target(plan_state, object_type, target)?;
            let (operator, operator_definition) = plan_state
                .context
                .find_comparison_operator(&target.result_type(), &operator)?;
            let value_type = operator_definition.argument_type(&target.result_type());
            Ok(plan::GroupExpression::BinaryComparisonOperator {
                target,
                operator,
                value: plan_for_group_comparison_value(plan_state, value_type, value)?,
            })
        }
    }
}

fn plan_for_group_comparison_target<T: QueryContext>(
    plan_state: &mut QueryPlanState<T>,
    object_type: &plan::ObjectType<T::ScalarType>,
    target: ndc::GroupComparisonTarget,
) -> Result<plan::GroupComparisonTarget<T>> {
    let plan_target = match target {
        // TODO: Do we expect the target aggregate to correspond to one of the grouping aggregates?
        // Or can it be independent?
        ndc::GroupComparisonTarget::Aggregate { aggregate } => {
            let target_aggregate = plan_for_aggregate(plan_state, object_type, aggregate)?;
            plan::GroupComparisonTarget::Aggregate {
                aggregate: target_aggregate,
            }
        }
    };
    Ok(plan_target)
}

fn plan_for_group_comparison_value<T: QueryContext>(
    plan_state: &mut QueryPlanState<T>,
    expected_type: plan::Type<T::ScalarType>,
    value: ndc::GroupComparisonValue,
) -> Result<plan::GroupComparisonValue<T>> {
    match value {
        ndc::GroupComparisonValue::Scalar { value } => Ok(plan::GroupComparisonValue::Scalar {
            value,
            value_type: expected_type,
        }),
        ndc::GroupComparisonValue::Variable { name } => {
            plan_state.register_variable_use(&name, expected_type.clone());
            Ok(plan::GroupComparisonValue::Variable {
                name,
                variable_type: expected_type,
            })
        }
    }
}
