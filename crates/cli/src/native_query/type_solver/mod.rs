mod constraint_to_type;
mod simplify;
mod substitute;

use std::collections::{BTreeMap, HashMap, HashSet};

use configuration::{
    schema::{ObjectType, Type},
    Configuration,
};
use itertools::Itertools;
use ndc_models::ObjectTypeName;
use simplify::simplify_constraints;
use substitute::substitute;

use super::{
    error::{Error, Result},
    type_constraint::{ObjectTypeConstraint, TypeConstraint, TypeVariable},
};

use self::constraint_to_type::constraint_to_type;

fn unify(
    configuration: &Configuration,
    object_type_constraints: &mut BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    mut type_variables: HashMap<TypeVariable, HashSet<TypeConstraint>>,
) -> Result<(
    HashMap<TypeVariable, Type>,
    BTreeMap<ObjectTypeName, ObjectType>,
)> {
    let mut added_object_types = BTreeMap::new();
    let mut solutions = HashMap::new();
    // let is_solved = |variable: TypeVariable| solutions.contains_key(&variable);
    fn is_solved(solutions: &HashMap<TypeVariable, Type>, variable: TypeVariable) -> bool {
        solutions.contains_key(&variable)
    }

    loop {
        let prev_type_variables = type_variables.clone();

        // TODO: check for mismatches, e.g. constraint list contains scalar & array

        for (variable, constraints) in type_variables.iter_mut() {
            let simplified =
                simplify_constraints(object_type_constraints, constraints.iter().cloned());
            *constraints = simplified;
        }

        for (variable, constraints) in &type_variables {
            if !is_solved(&solutions, *variable) && constraints.len() == 1 {
                let constraint = constraints.iter().next().unwrap();
                if let Some(solved_type) = constraint_to_type(
                    configuration,
                    &mut added_object_types,
                    object_type_constraints,
                    constraint,
                )? {
                    solutions.insert(*variable, solved_type);
                }
            }
        }

        let variables = type_variables_by_complexity(&type_variables);

        for variable in &variables {
            if let Some(variable_constraints) = type_variables.get(&variable).cloned() {
                substitute(&mut type_variables, *variable, &variable_constraints);
            }
        }

        if type_variables
            .keys()
            .copied()
            .all(|v| is_solved(&solutions, v))
        {
            return Ok((solutions, added_object_types));
        }

        if type_variables == prev_type_variables {
            return Err(Error::FailedToUnify {
                unsolved_variables: variables
                    .into_iter()
                    .filter(|v| !is_solved(&solutions, *v))
                    .collect(),
            });
        }
    }
}

/// List type variables ordered according to increasing complexity of their constraints.
fn type_variables_by_complexity(
    type_variables: &HashMap<TypeVariable, HashSet<TypeConstraint>>,
) -> Vec<TypeVariable> {
    type_variables
        .iter()
        .sorted_unstable_by_key(|(_, constraints)| {
            let complexity: usize = constraints.iter().map(TypeConstraint::complexity).sum();
            complexity
        })
        .map(|(variable, _)| variable)
        .copied()
        .collect_vec()
}

// TODO: Replace occurences of:
//
//     a1 : [ ElementOf(a2) ]
//     b1 : [ FieldOf(b2, path) ]
//
// with:
//
//     a1: [ ]
//     a2: [ ArrayOf(a1) ]
//     b1: [ ]
//     b2: [ Object { path: b1 } ]
//
// fn top_down_substitution() {}

// fn solve_variable(
//     variable: TypeVariable,
//     constraints: &HashSet<TypeConstraint>,
//     type_variables: &HashMap<TypeVariable, HashSet<TypeConstraint>>,
// ) -> TypeConstraint {
//     constraints.iter().fold(None, |accum, next_constraint| {
//         simplify_constraint_pair(object_types, type_variables, accum, next_constraint)
//     })
// }
