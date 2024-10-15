mod constraint_to_type;

use std::collections::{BTreeMap, HashMap, HashSet};

use configuration::schema::{ObjectType, Type};
use itertools::Itertools;
use mongodb_support::BsonScalarType;
use ndc_models::{FieldName, ObjectTypeName};
use nonempty::NonEmpty;

use crate::introspection::type_unification::is_supertype;

use super::{
    error::{Error, Result},
    pipeline_type_context::PipelineTypeContext,
    type_constraint::{ObjectTypeConstraint, TypeConstraint, TypeVariable},
};

use TypeConstraint as C;

/// Result of an attempt to simplify two things into one thing.
#[derive(Clone, Debug)]
enum Simplified<T> {
    /// Two values were successfully unified into one
    One(T),

    /// Simplification is not possible, we got both values back
    Both((T, T)),
}

impl<T> Simplified<T> {
    fn map<F, U>(self, mut f: F) -> Simplified<U>
    where
        F: FnMut(T) -> U,
    {
        match self {
            Simplified::One(x) => Simplified::One(f(x)),
            Simplified::Both((x, y)) => Simplified::Both((f(x), f(y))),
        }
    }
}

fn unify(
    mut type_variables: HashMap<TypeVariable, HashSet<TypeConstraint>>,
) -> Result<HashMap<TypeVariable, Type>> {
    let variables: Vec<TypeVariable> = type_variables.keys().copied().collect();

    // let mut complexities: HashMap<TypeVariable, usize> = type_variables
    //     .iter()
    //     .map(|(variable, constraints)| {
    //         let complexity = constraints.iter().map(TypeConstraint::complexity).sum();
    //         (*variable, complexity)
    //     })
    //     .collect();

    let mut solutions = HashMap::new();
    let is_solved = |variable| solutions.contains_key(variable);

    loop {
        let least_complex_variable = *type_variables
            .iter()
            .filter(|(v, _)| !is_solved(*v))
            .sorted_unstable_by_key(|(_, constraints)| {
                let complexity: usize = constraints.iter().map(TypeConstraint::complexity).sum();
                complexity
            })
            .next()
            .unwrap()
            .0;
    }

    Ok(solutions)
}

fn substitute(
    type_variables: &mut HashMap<TypeVariable, HashSet<TypeConstraint>>,
    variable: TypeVariable,
    constraints: HashSet<TypeConstraint>,
) {
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

fn solve_variable(
    variable: TypeVariable,
    constraints: &HashSet<TypeConstraint>,
    type_variables: &HashMap<TypeVariable, HashSet<TypeConstraint>>,
) -> TypeConstraint {
    constraints.iter().fold(None, |accum, next_constraint| {
        simplify_constraint_pair(object_types, type_variables, accum, next_constraint)
    })
}

fn simplify_constraint_pair(
    // context: PipelineTypeContext<'_>,
    // variable: TypeVariable,
    // constraints: HashSet<TypeConstraint>,
    object_types: &BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    type_variables: &HashMap<TypeVariable, HashSet<TypeConstraint>>,
    a: TypeConstraint,
    b: TypeConstraint,
) -> Simplified<TypeConstraint> {
    match (a, b) {
        (C::ExtendedJSON, _) | (_, C::ExtendedJSON) => Simplified::One(C::ExtendedJSON),
        (C::Scalar(a), C::Scalar(b)) => solve_scalar(a, b),

        (C::Nullable(a), b) => simplify_constraint_pair(object_types, type_variables, *a, b)
            .map(|constraint| C::Nullable(Box::new(constraint))),
        (a, C::Nullable(b)) => simplify_constraint_pair(object_types, type_variables, a, *b)
            .map(|constraint| C::Nullable(Box::new(constraint))),

        (C::Variable(a), b) => todo!(), // return list of constraints for a with b appended?

        (C::Scalar(_), C::Variable(_)) => todo!(),
        (C::Scalar(_), C::ElementOf(_)) => todo!(),
        (C::Scalar(_), C::FieldOf { target_type, path }) => todo!(),
        (
            C::Scalar(_),
            C::WithFieldOverrides {
                target_type,
                fields,
            },
        ) => todo!(),
        (C::Object(_), C::Scalar(_)) => todo!(),
        (C::Object(_), C::Object(_)) => todo!(),
        (C::Object(_), C::ArrayOf(_)) => todo!(),
        (C::Object(_), C::Nullable(_)) => todo!(),
        (C::Object(_), C::Predicate { object_type_name }) => todo!(),
        (C::Object(_), C::Variable(_)) => todo!(),
        (C::Object(_), C::ElementOf(_)) => todo!(),
        (C::Object(_), C::FieldOf { target_type, path }) => todo!(),
        (
            C::Object(_),
            C::WithFieldOverrides {
                target_type,
                fields,
            },
        ) => todo!(),
        (C::ArrayOf(_), C::Scalar(_)) => todo!(),
        (C::ArrayOf(_), C::Object(_)) => todo!(),
        (C::ArrayOf(_), C::ArrayOf(_)) => todo!(),
        (C::ArrayOf(_), C::Nullable(_)) => todo!(),
        (C::ArrayOf(_), C::Predicate { object_type_name }) => todo!(),
        (C::ArrayOf(_), C::Variable(_)) => todo!(),
        (C::ArrayOf(_), C::ElementOf(_)) => todo!(),
        (C::ArrayOf(_), C::FieldOf { target_type, path }) => todo!(),
        (
            C::ArrayOf(_),
            C::WithFieldOverrides {
                target_type,
                fields,
            },
        ) => todo!(),
        (C::Predicate { object_type_name }, C::Scalar(_)) => todo!(),
        (C::Predicate { object_type_name }, C::Object(_)) => todo!(),
        (C::Predicate { object_type_name }, C::ArrayOf(_)) => todo!(),
        (C::Predicate { object_type_name }, C::Nullable(_)) => todo!(),
        (C::Predicate { object_type_name }, C::Predicate { object_type_name }) => todo!(),
        (C::Predicate { object_type_name }, C::Variable(_)) => todo!(),
        (C::Predicate { object_type_name }, C::ElementOf(_)) => todo!(),
        (C::Predicate { object_type_name }, C::FieldOf { target_type, path }) => todo!(),
        (
            C::Predicate { object_type_name },
            C::WithFieldOverrides {
                target_type,
                fields,
            },
        ) => todo!(),
        (C::Variable(_), C::Scalar(_)) => todo!(),
        (C::Variable(_), C::Object(_)) => todo!(),
        (C::Variable(_), C::ArrayOf(_)) => todo!(),
        (C::Variable(_), C::Nullable(_)) => todo!(),
        (C::Variable(_), C::Predicate { object_type_name }) => todo!(),
        (C::Variable(_), C::Variable(_)) => todo!(),
        (C::Variable(_), C::ElementOf(_)) => todo!(),
        (C::Variable(_), C::FieldOf { target_type, path }) => todo!(),
        (
            C::Variable(_),
            C::WithFieldOverrides {
                target_type,
                fields,
            },
        ) => todo!(),
        (C::ElementOf(_), C::Scalar(_)) => todo!(),
        (C::ElementOf(_), C::Object(_)) => todo!(),
        (C::ElementOf(_), C::ArrayOf(_)) => todo!(),
        (C::ElementOf(_), C::Nullable(_)) => todo!(),
        (C::ElementOf(_), C::Predicate { object_type_name }) => todo!(),
        (C::ElementOf(_), C::Variable(_)) => todo!(),
        (C::ElementOf(_), C::ElementOf(_)) => todo!(),
        (C::ElementOf(_), C::FieldOf { target_type, path }) => todo!(),
        (
            C::ElementOf(_),
            C::WithFieldOverrides {
                target_type,
                fields,
            },
        ) => todo!(),
        (C::FieldOf { target_type, path }, C::Scalar(_)) => todo!(),
        (C::FieldOf { target_type, path }, C::Object(_)) => todo!(),
        (C::FieldOf { target_type, path }, C::ArrayOf(_)) => todo!(),
        (C::FieldOf { target_type, path }, C::Nullable(_)) => todo!(),
        (C::FieldOf { target_type, path }, C::Predicate { object_type_name }) => todo!(),
        (C::FieldOf { target_type, path }, C::Variable(_)) => todo!(),
        (C::FieldOf { target_type, path }, C::ElementOf(_)) => todo!(),
        (C::FieldOf { target_type, path }, C::FieldOf { target_type, path }) => todo!(),
        (
            C::FieldOf { target_type, path },
            C::WithFieldOverrides {
                target_type,
                fields,
            },
        ) => todo!(),
        (
            C::WithFieldOverrides {
                target_type,
                fields,
            },
            C::Scalar(_),
        ) => todo!(),
        (
            C::WithFieldOverrides {
                target_type,
                fields,
            },
            C::Object(_),
        ) => todo!(),
        (
            C::WithFieldOverrides {
                target_type,
                fields,
            },
            C::ArrayOf(_),
        ) => todo!(),
        (
            C::WithFieldOverrides {
                target_type,
                fields,
            },
            C::Nullable(_),
        ) => todo!(),
        (
            C::WithFieldOverrides {
                target_type,
                fields,
            },
            C::Predicate { object_type_name },
        ) => todo!(),
        (
            C::WithFieldOverrides {
                target_type,
                fields,
            },
            C::Variable(_),
        ) => todo!(),
        (
            C::WithFieldOverrides {
                target_type,
                fields,
            },
            C::ElementOf(_),
        ) => todo!(),
        (
            C::WithFieldOverrides {
                target_type,
                fields,
            },
            C::FieldOf { target_type, path },
        ) => todo!(),
        (
            C::WithFieldOverrides {
                target_type,
                fields,
            },
            C::WithFieldOverrides {
                target_type,
                fields,
            },
        ) => todo!(),
    }
}

fn solve_scalar(a: BsonScalarType, b: BsonScalarType) -> Simplified<TypeConstraint> {
    if a == b || is_supertype(&a, &b) {
        Simplified::One(C::Scalar(a))
    } else if is_supertype(&b, &a) {
        Simplified::One(C::Scalar(b))
    } else {
        Simplified::Both((C::Scalar(a), C::Scalar(b)))
    }
}

