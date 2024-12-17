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
    let arguments =
        plan_for_arguments_generic(plan_state, parameters, arguments, plan_for_argument)?;

    for argument in arguments.values() {
        if let plan::Argument::Variable {
            name,
            argument_type,
        } = argument
        {
            plan_state.register_variable_use(name, argument_type.clone())
        }
    }

    Ok(arguments)
}

/// Convert maps of [serde_json::Value] values to maps of [plan::MutationProcedureArgument]
pub fn plan_for_mutation_procedure_arguments<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    parameters: &BTreeMap<ndc::ArgumentName, ndc::ArgumentInfo>,
    arguments: BTreeMap<ndc::ArgumentName, serde_json::Value>,
) -> Result<BTreeMap<ndc::ArgumentName, plan::MutationProcedureArgument<T>>> {
    plan_for_arguments_generic(
        plan_state,
        parameters,
        arguments,
        plan_for_mutation_procedure_argument,
    )
}

/// Convert maps of [ndc::RelationshipArgument] values to maps of [plan::RelationshipArgument]
pub fn plan_for_relationship_arguments<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    parameters: &BTreeMap<ndc::ArgumentName, ndc::ArgumentInfo>,
    arguments: BTreeMap<ndc::ArgumentName, ndc::RelationshipArgument>,
) -> Result<BTreeMap<ndc::ArgumentName, plan::RelationshipArgument<T>>> {
    let arguments = plan_for_arguments_generic(
        plan_state,
        parameters,
        arguments,
        plan_for_relationship_argument,
    )?;

    for argument in arguments.values() {
        if let plan::RelationshipArgument::Variable {
            name,
            argument_type,
        } = argument
        {
            plan_state.register_variable_use(name, argument_type.clone())
        }
    }

    Ok(arguments)
}

/// Create a map of plan arguments when we already have plan types for parameters.
pub fn plan_arguments_from_plan_parameters<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    parameters: &BTreeMap<ndc::ArgumentName, plan::Type<T::ScalarType>>,
    arguments: BTreeMap<ndc::ArgumentName, ndc::Argument>,
) -> Result<BTreeMap<ndc::ArgumentName, plan::Argument<T>>> {
    let arguments = plan_for_arguments_generic(
        plan_state,
        parameters,
        arguments,
        |_plan_state, plan_type, argument| match argument {
            ndc::Argument::Variable { name } => Ok(plan::Argument::Variable {
                name,
                argument_type: plan_type.clone(),
            }),
            ndc::Argument::Literal { value } => Ok(plan::Argument::Literal {
                value,
                argument_type: plan_type.clone(),
            }),
        },
    )?;

    for argument in arguments.values() {
        if let plan::Argument::Variable {
            name,
            argument_type,
        } = argument
        {
            plan_state.register_variable_use(name, argument_type.clone())
        }
    }

    Ok(arguments)
}

fn plan_for_argument<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    argument_info: &ndc::ArgumentInfo,
    argument: ndc::Argument,
) -> Result<plan::Argument<T>> {
    match argument {
        ndc::Argument::Variable { name } => Ok(plan::Argument::Variable {
            name,
            argument_type: plan_state
                .context
                .ndc_to_plan_type(&argument_info.argument_type)?,
        }),
        ndc::Argument::Literal { value } => match &argument_info.argument_type {
            ndc::Type::Predicate { object_type_name } => Ok(plan::Argument::Predicate {
                expression: plan_for_predicate(plan_state, object_type_name, value)?,
            }),
            t => Ok(plan::Argument::Literal {
                value,
                argument_type: plan_state.context.ndc_to_plan_type(t)?,
            }),
        },
    }
}

fn plan_for_mutation_procedure_argument<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    argument_info: &ndc::ArgumentInfo,
    value: serde_json::Value,
) -> Result<plan::MutationProcedureArgument<T>> {
    match &argument_info.argument_type {
        ndc::Type::Predicate { object_type_name } => {
            Ok(plan::MutationProcedureArgument::Predicate {
                expression: plan_for_predicate(plan_state, object_type_name, value)?,
            })
        }
        t => Ok(plan::MutationProcedureArgument::Literal {
            value,
            argument_type: plan_state.context.ndc_to_plan_type(t)?,
        }),
    }
}

fn plan_for_relationship_argument<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    argument_info: &ndc::ArgumentInfo,
    argument: ndc::RelationshipArgument,
) -> Result<plan::RelationshipArgument<T>> {
    let argument_type = &argument_info.argument_type;
    match argument {
        ndc::RelationshipArgument::Variable { name } => Ok(plan::RelationshipArgument::Variable {
            name,
            argument_type: plan_state.context.ndc_to_plan_type(argument_type)?,
        }),
        ndc::RelationshipArgument::Column { name } => Ok(plan::RelationshipArgument::Column {
            name,
            argument_type: plan_state.context.ndc_to_plan_type(argument_type)?,
        }),
        ndc::RelationshipArgument::Literal { value } => match argument_type {
            ndc::Type::Predicate { object_type_name } => {
                Ok(plan::RelationshipArgument::Predicate {
                    expression: plan_for_predicate(plan_state, object_type_name, value)?,
                })
            }
            t => Ok(plan::RelationshipArgument::Literal {
                value,
                argument_type: plan_state.context.ndc_to_plan_type(t)?,
            }),
        },
    }
}

fn plan_for_predicate<T: QueryContext>(
    plan_state: &mut QueryPlanState<'_, T>,
    object_type_name: &ndc::ObjectTypeName,
    value: serde_json::Value,
) -> Result<plan::Expression<T>> {
    let object_type = plan_state.context.find_object_type(object_type_name)?;
    let ndc_expression = serde_json::from_value::<ndc::Expression>(value)
        .map_err(QueryPlanError::ErrorParsingPredicate)?;
    plan_for_expression(plan_state, &object_type, &object_type, ndc_expression)
}

/// Convert maps of [ndc::Argument] or [ndc::RelationshipArgument] values to [plan::Argument] or
/// [plan::RelationshipArgument] respectively.
fn plan_for_arguments_generic<T: QueryContext, Parameter, NdcArgument, PlanArgument, F>(
    plan_state: &mut QueryPlanState<'_, T>,
    parameters: &BTreeMap<ndc::ArgumentName, Parameter>,
    mut arguments: BTreeMap<ndc::ArgumentName, NdcArgument>,
    convert_argument: F,
) -> Result<BTreeMap<ndc::ArgumentName, PlanArgument>>
where
    F: Fn(&mut QueryPlanState<'_, T>, &Parameter, NdcArgument) -> Result<PlanArgument>,
{
    validate_no_excess_arguments(parameters, &arguments)?;

    let (arguments, missing): (
        Vec<(ndc::ArgumentName, NdcArgument, &Parameter)>,
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
            match convert_argument(plan_state, argument_info, argument) {
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

pub fn validate_no_excess_arguments<T, Parameter>(
    parameters: &BTreeMap<ndc::ArgumentName, Parameter>,
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
