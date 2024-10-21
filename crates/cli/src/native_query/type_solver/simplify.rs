use std::collections::{BTreeMap, HashMap, HashSet};

use configuration::schema::Type;
use itertools::Itertools;
use mongodb_support::BsonScalarType;
use ndc_models::ObjectTypeName;

use crate::introspection::type_unification::is_supertype;

use crate::native_query::{
    error::{Error, Result},
    pipeline_type_context::PipelineTypeContext,
    type_constraint::{ObjectTypeConstraint, TypeConstraint, TypeVariable},
};

use TypeConstraint as C;

type Simplified<T> = std::result::Result<T, (T, T)>;

// /// Result of an attempt to simplify two things into one thing.
// #[derive(Clone, Debug)]
// enum Simplified<T> {
//     /// Two values were successfully unified into one
//     One(T),
//
//     /// Simplification is not possible, we got both values back
//     Both((T, T)),
// }
//
// impl<T> Simplified<T> {
//     fn map<F, U>(self, mut f: F) -> Simplified<U>
//     where
//         F: FnMut(T) -> U,
//     {
//         match self {
//             Simplified::One(x) => Simplified::One(f(x)),
//             Simplified::Both((x, y)) => Simplified::Both((f(x), f(y))),
//         }
//     }
// }

// Attempts to reduce the number of type constraints from the input by combining redundant
// constraints, and by merging constraints into more specific ones where possible. This is
// guaranteed to produce a list that is equal or smaller in length compared to the input.
pub fn simplify_constraints(
    object_types: &BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    constraints: impl IntoIterator<Item = TypeConstraint>,
) -> HashSet<TypeConstraint> {
    constraints
        .into_iter()
        .coalesce(|constraint_a, constraint_b| {
            simplify_constraint_pair(object_types, constraint_a, constraint_b)
        })
        .collect()
}

fn simplify_constraint_pair(
    // context: PipelineTypeContext<'_>,
    // variable: TypeVariable,
    // constraints: HashSet<TypeConstraint>,
    object_types: &BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    // type_variables: &HashMap<TypeVariable, HashSet<TypeConstraint>>,
    a: TypeConstraint,
    b: TypeConstraint,
) -> Simplified<TypeConstraint> {
    match (a, b) {
        (C::ExtendedJSON, _) | (_, C::ExtendedJSON) => Ok(C::ExtendedJSON),
        (C::Scalar(a), C::Scalar(b)) => solve_scalar(a, b),

        (C::Nullable(a), b) => {
            simplify_constraint_pair(object_types, /*type_variables,*/ *a, b)
                .map(|constraint| C::Nullable(Box::new(constraint)))
        }
        (a, C::Nullable(b)) => {
            simplify_constraint_pair(object_types, /*type_variables,*/ a, *b)
                .map(|constraint| C::Nullable(Box::new(constraint)))
        }

        (C::Variable(a), b) => todo!(), // return list of constraints for a with b appended?

        (C::Scalar(_), C::Variable(_)) => todo!(),
        (C::Scalar(_), C::ElementOf(_)) => todo!(),
        (C::Scalar(_), C::FieldOf { target_type, path }) => todo!(),
        (
            C::Scalar(_),
            C::WithFieldOverrides {
                target_type,
                fields,
                ..
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
                ..
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
                ..
            },
        ) => todo!(),
        (C::Predicate { object_type_name }, C::Scalar(_)) => todo!(),
        (C::Predicate { object_type_name }, C::Object(_)) => todo!(),
        (C::Predicate { object_type_name }, C::ArrayOf(_)) => todo!(),
        (C::Predicate { object_type_name }, C::Nullable(_)) => todo!(),
        (
            C::Predicate {
                object_type_name: a,
            },
            C::Predicate {
                object_type_name: b,
            },
        ) => todo!(),
        (C::Predicate { object_type_name }, C::Variable(_)) => todo!(),
        (C::Predicate { object_type_name }, C::ElementOf(_)) => todo!(),
        (C::Predicate { object_type_name }, C::FieldOf { target_type, path }) => todo!(),
        (
            C::Predicate { object_type_name },
            C::WithFieldOverrides {
                target_type,
                fields,
                ..
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
                ..
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
                ..
            },
        ) => todo!(),
        (C::FieldOf { target_type, path }, C::Scalar(_)) => todo!(),
        (C::FieldOf { target_type, path }, C::Object(_)) => todo!(),
        (C::FieldOf { target_type, path }, C::ArrayOf(_)) => todo!(),
        (C::FieldOf { target_type, path }, C::Nullable(_)) => todo!(),
        (C::FieldOf { target_type, path }, C::Predicate { object_type_name }) => todo!(),
        (C::FieldOf { target_type, path }, C::Variable(_)) => todo!(),
        (C::FieldOf { target_type, path }, C::ElementOf(_)) => todo!(),
        (
            C::FieldOf {
                target_type: target_type_a,
                path: path_a,
            },
            C::FieldOf {
                target_type: target_type_b,
                path: path_b,
            },
        ) => todo!(),
        // (
        //     C::FieldOf { target_type, path },
        //     C::WithFieldOverrides {
        //         target_type,
        //         fields,
        //         ..
        //     },
        // ) => todo!(),
        (
            C::WithFieldOverrides {
                target_type,
                fields,
                ..
            },
            C::Scalar(_),
        ) => todo!(),
        (
            C::WithFieldOverrides {
                target_type,
                fields,
                ..
            },
            C::Object(_),
        ) => todo!(),
        (
            C::WithFieldOverrides {
                target_type,
                fields,
                ..
            },
            C::ArrayOf(_),
        ) => todo!(),
        (
            C::WithFieldOverrides {
                target_type,
                fields,
                ..
            },
            C::Nullable(_),
        ) => todo!(),
        (
            C::WithFieldOverrides {
                target_type,
                fields,
                ..
            },
            C::Predicate { object_type_name },
        ) => todo!(),
        (
            C::WithFieldOverrides {
                target_type,
                fields,
                ..
            },
            C::Variable(_),
        ) => todo!(),
        (
            C::WithFieldOverrides {
                target_type,
                fields,
                ..
            },
            C::ElementOf(_),
        ) => todo!(),
        (
            C::WithFieldOverrides {
                target_type: target_type_a,
                fields,
                ..
            },
            C::FieldOf {
                target_type: target_type_b,
                path,
            },
        ) => todo!(),
        (
            C::WithFieldOverrides {
                target_type: target_type_a,
                fields: fields_a,
                ..
            },
            C::WithFieldOverrides {
                target_type: target_type_b,
                fields: fields_b,
                ..
            },
        ) => todo!(),
        _ => todo!("other simplify branch"),
    }
}

fn solve_scalar(a: BsonScalarType, b: BsonScalarType) -> Simplified<TypeConstraint> {
    if a == b || is_supertype(&a, &b) {
        Ok(C::Scalar(a))
    } else if is_supertype(&b, &a) {
        Ok(C::Scalar(b))
    } else {
        Err((C::Scalar(a), C::Scalar(b)))
    }
}
