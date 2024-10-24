use std::collections::{HashMap, HashSet};

use itertools::Either;

use crate::native_query::type_constraint::{TypeConstraint, TypeVariable};

/// Given a type variable that has been reduced to a single type constraint, replace occurrences if
/// the variable in
pub fn substitute(
    type_variables: &mut HashMap<TypeVariable, HashSet<TypeConstraint>>,
    variable: TypeVariable,
    variable_constraints: &HashSet<TypeConstraint>,
) {
    for (v, target_constraints) in type_variables.iter_mut() {
        if *v == variable {
            continue;
        }

        // Replace top-level variable references with the list of constraints assigned to the
        // variable being substituted.
        let mut substituted_constraints: HashSet<TypeConstraint> = target_constraints
            .iter()
            .cloned()
            .flat_map(|target_constraint| match target_constraint {
                TypeConstraint::Variable(v) if v == variable => {
                    Either::Left(variable_constraints.iter().cloned())
                }
                t => Either::Right(std::iter::once(t)),
            })
            .collect();

        // Recursively replace variable references inside each constraint. A [TypeConstraint] can
        // reference at most one other constraint, so we can only do this if the variable being
        // substituted has been reduced to a single constraint.
        if variable_constraints.len() == 1 {
            let variable_constraint = variable_constraints.iter().next().unwrap();
            substituted_constraints = substituted_constraints
                .into_iter()
                .map(|target_constraint| {
                    substitute_in_constraint(variable, variable_constraint, target_constraint)
                })
                .collect();
        }

        *target_constraints = substituted_constraints;
    }
    // substitution_made
}

fn substitute_in_constraint(
    variable: TypeVariable,
    variable_constraint: &TypeConstraint,
    target_constraint: TypeConstraint,
) -> TypeConstraint {
    match target_constraint {
        t @ TypeConstraint::Variable(v) => {
            if v == variable {
                variable_constraint.clone()
            } else {
                t
            }
        }
        t @ TypeConstraint::ExtendedJSON => t,
        t @ TypeConstraint::Scalar(_) => t,
        t @ TypeConstraint::Object(_) => t,
        TypeConstraint::ArrayOf(t) => TypeConstraint::ArrayOf(Box::new(substitute_in_constraint(
            variable,
            variable_constraint,
            *t,
        ))),
        TypeConstraint::Nullable(t) => TypeConstraint::Nullable(Box::new(
            substitute_in_constraint(variable, variable_constraint, *t),
        )),
        t @ TypeConstraint::Predicate { .. } => t,
        TypeConstraint::ElementOf(t) => TypeConstraint::ElementOf(Box::new(
            substitute_in_constraint(variable, variable_constraint, *t),
        )),
        TypeConstraint::FieldOf { target_type, path } => TypeConstraint::FieldOf {
            target_type: Box::new(substitute_in_constraint(
                variable,
                variable_constraint,
                *target_type,
            )),
            path,
        },
        TypeConstraint::WithFieldOverrides {
            augmented_object_type_name,
            target_type,
            fields,
        } => TypeConstraint::WithFieldOverrides {
            augmented_object_type_name,
            target_type: Box::new(substitute_in_constraint(
                variable,
                variable_constraint,
                *target_type,
            )),
            fields,
        },
    }
}
