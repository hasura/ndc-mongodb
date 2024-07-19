use std::collections::BTreeMap;

use crate::{self as plan, QueryContext, QueryPlanError};
use itertools::Itertools as _;
use ndc_models as ndc;

use super::{plan_for_expression, query_plan_state::QueryPlanState};

type Result<T> = std::result::Result<T, QueryPlanError>;

/// Convert maps of [ndc::Argument] values to maps of [plan::Argument]
pub fn plan_for_arguments<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    parameters: &BTreeMap<ndc::ArgumentName, ndc::ArgumentInfo>,
    arguments: BTreeMap<ndc::ArgumentName, ndc::Argument>,
) -> Result<BTreeMap<ndc::ArgumentName, plan::Argument<T>>> {
    plan_for_arguments_generic(plan_state, parameters, arguments, plan_for_argument)
}

/// Convert maps of [ndc::Argument] values to maps of [plan::Argument]
pub fn plan_for_relationship_arguments<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    parameters: &BTreeMap<ndc::ArgumentName, ndc::ArgumentInfo>,
    arguments: BTreeMap<ndc::ArgumentName, ndc::RelationshipArgument>,
) -> Result<BTreeMap<ndc::ArgumentName, plan::RelationshipArgument<T>>> {
    plan_for_arguments_generic(
        plan_state,
        parameters,
        arguments,
        plan_for_relationship_argument,
    )
}

fn plan_for_argument<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    parameter_type: &ndc::Type,
    argument: ndc::Argument,
) -> Result<plan::Argument<T>> {
    match argument {
        ndc::Argument::Variable { name } => Ok(plan::Argument::Variable {
            name,
            argument_type: plan_state.context.ndc_to_plan_type(parameter_type)?,
        }),
        ndc::Argument::Literal { value } => match parameter_type {
            ndc::Type::Predicate { object_type_name } => {
                let object_type = plan_state.context.find_object_type(object_type_name)?;
                let ndc_expression = serde_json::from_value::<ndc::Expression>(value)
                    .map_err(QueryPlanError::ErrorParsingPredicate)?;
                let expression =
                    plan_for_expression(plan_state, &object_type, &object_type, ndc_expression)?;
                Ok(plan::Argument::Predicate { expression })
            }
            t => Ok(plan::Argument::Literal {
                value,
                argument_type: plan_state.context.ndc_to_plan_type(t)?,
            }),
        },
    }
}

fn plan_for_relationship_argument<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    parameter_type: &ndc::Type,
    argument: ndc::RelationshipArgument,
) -> Result<plan::RelationshipArgument<T>> {
    match argument {
        ndc::RelationshipArgument::Variable { name } => Ok(plan::RelationshipArgument::Variable {
            name,
            argument_type: plan_state.context.ndc_to_plan_type(parameter_type)?,
        }),
        ndc::RelationshipArgument::Column { name } => Ok(plan::RelationshipArgument::Column {
            name,
            argument_type: plan_state.context.ndc_to_plan_type(parameter_type)?,
        }),
        ndc::RelationshipArgument::Literal { value } => match parameter_type {
            ndc::Type::Predicate { object_type_name } => {
                let object_type = plan_state.context.find_object_type(object_type_name)?;
                let ndc_expression = serde_json::from_value::<ndc::Expression>(value)
                    .map_err(QueryPlanError::ErrorParsingPredicate)?;
                let expression =
                    plan_for_expression(plan_state, &object_type, &object_type, ndc_expression)?;
                Ok(plan::RelationshipArgument::Predicate { expression })
            }
            t => Ok(plan::RelationshipArgument::Literal {
                value,
                argument_type: plan_state.context.ndc_to_plan_type(t)?,
            }),
        },
    }
}

/// Convert maps of [ndc::Argument] or [ndc::RelationshipArgument] values to [plan::Argument] or
/// [plan::RelationshipArgument] respectively.
fn plan_for_arguments_generic<T: QueryContext, NdcArgument, PlanArgument, F>(
    plan_state: &mut QueryPlanState<'_, T>,
    parameters: &BTreeMap<ndc::ArgumentName, ndc::ArgumentInfo>,
    mut arguments: BTreeMap<ndc::ArgumentName, NdcArgument>,
    convert_argument: F,
) -> Result<BTreeMap<ndc::ArgumentName, PlanArgument>>
where
    F: Fn(&mut QueryPlanState<'_, T>, &ndc::Type, NdcArgument) -> Result<PlanArgument>,
{
    validate_no_excess_arguments(parameters, &arguments)?;

    let (arguments, missing): (
        Vec<(ndc::ArgumentName, NdcArgument, &ndc::ArgumentInfo)>,
        Vec<ndc::ArgumentName>,
    ) = parameters
        .iter()
        .map(|(name, parameter_type)| {
            if let Some((name, argument)) = arguments.remove_entry(name) {
                Ok((name, argument, parameter_type))
            } else {
                Err(name.clone())
            }
        })
        .partition_result();
    if !missing.is_empty() {
        return Err(QueryPlanError::MissingArguments(missing));
    }

    let (resolved, errors): (
        BTreeMap<ndc::ArgumentName, PlanArgument>,
        BTreeMap<ndc::ArgumentName, QueryPlanError>,
    ) = arguments
        .into_iter()
        .map(|(name, argument, argument_info)| {
            match convert_argument(plan_state, &argument_info.argument_type, argument) {
                Ok(argument) => Ok((name, argument)),
                Err(err) => Err((name, err)),
            }
        })
        .partition_result();
    if !errors.is_empty() {
        return Err(QueryPlanError::InvalidArguments(errors));
    }

    Ok(resolved)
}

pub fn validate_no_excess_arguments<T>(
    parameters: &BTreeMap<ndc::ArgumentName, ndc::ArgumentInfo>,
    arguments: &BTreeMap<ndc::ArgumentName, T>,
) -> Result<()> {
    let excess: Vec<ndc::ArgumentName> = arguments
        .iter()
        .filter_map(|(name, _)| {
            let parameter = parameters.get(name);
            match parameter {
                Some(_) => None,
                None => Some(name.clone()),
            }
        })
        .collect();
    if !excess.is_empty() {
        Err(QueryPlanError::ExcessArguments(excess))
    } else {
        Ok(())
    }
}
