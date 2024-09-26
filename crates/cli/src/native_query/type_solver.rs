use std::collections::{BTreeMap, HashMap, HashSet};

use configuration::schema::Type;
use mongodb_support::BsonScalarType;
use ndc_models::ObjectTypeName;

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

// fn types_are_solved(
//     context: &PipelineTypeContext<'_>,
//     type_variables: HashMap<TypeVariable, HashSet<TypeConstraint>>,
// ) -> bool {
// }

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
        (C::ExtendedJSON, _) | (_, C::ExtendedJSON) => C::ExtendedJSON,
        (C::Scalar(a), C::Scalar(b)) => solve_scalar(a, b),

        (C::Nullable(a), b) => C::Nullable(Box::new(simplify_constraint_pair(
            object_types,
            type_variables,
            *a,
            b,
        ))),
        (a, C::Nullable(b)) => C::Nullable(Box::new(simplify_constraint_pair(
            object_types,
            type_variables,
            a,
            *b,
        ))),

        (C::Variable(a), b) => todo!(),

        (TypeConstraint::Scalar(_), TypeConstraint::Variable(_)) => todo!(),
        (TypeConstraint::Scalar(_), TypeConstraint::ElementOf(_)) => todo!(),
        (TypeConstraint::Scalar(_), TypeConstraint::FieldOf { target_type, path }) => todo!(),
        (
            TypeConstraint::Scalar(_),
            TypeConstraint::WithFieldOverrides {
                target_type,
                fields,
            },
        ) => todo!(),
        (TypeConstraint::Object(_), TypeConstraint::Scalar(_)) => todo!(),
        (TypeConstraint::Object(_), TypeConstraint::Object(_)) => todo!(),
        (TypeConstraint::Object(_), TypeConstraint::ArrayOf(_)) => todo!(),
        (TypeConstraint::Object(_), TypeConstraint::Nullable(_)) => todo!(),
        (TypeConstraint::Object(_), TypeConstraint::Predicate { object_type_name }) => todo!(),
        (TypeConstraint::Object(_), TypeConstraint::Variable(_)) => todo!(),
        (TypeConstraint::Object(_), TypeConstraint::ElementOf(_)) => todo!(),
        (TypeConstraint::Object(_), TypeConstraint::FieldOf { target_type, path }) => todo!(),
        (
            TypeConstraint::Object(_),
            TypeConstraint::WithFieldOverrides {
                target_type,
                fields,
            },
        ) => todo!(),
        (TypeConstraint::ArrayOf(_), TypeConstraint::Scalar(_)) => todo!(),
        (TypeConstraint::ArrayOf(_), TypeConstraint::Object(_)) => todo!(),
        (TypeConstraint::ArrayOf(_), TypeConstraint::ArrayOf(_)) => todo!(),
        (TypeConstraint::ArrayOf(_), TypeConstraint::Nullable(_)) => todo!(),
        (TypeConstraint::ArrayOf(_), TypeConstraint::Predicate { object_type_name }) => todo!(),
        (TypeConstraint::ArrayOf(_), TypeConstraint::Variable(_)) => todo!(),
        (TypeConstraint::ArrayOf(_), TypeConstraint::ElementOf(_)) => todo!(),
        (TypeConstraint::ArrayOf(_), TypeConstraint::FieldOf { target_type, path }) => todo!(),
        (
            TypeConstraint::ArrayOf(_),
            TypeConstraint::WithFieldOverrides {
                target_type,
                fields,
            },
        ) => todo!(),
        (TypeConstraint::Nullable(_), TypeConstraint::Scalar(_)) => todo!(),
        (TypeConstraint::Nullable(_), TypeConstraint::Object(_)) => todo!(),
        (TypeConstraint::Nullable(_), TypeConstraint::ArrayOf(_)) => todo!(),
        (TypeConstraint::Nullable(_), TypeConstraint::Nullable(_)) => todo!(),
        (TypeConstraint::Nullable(_), TypeConstraint::Predicate { object_type_name }) => todo!(),
        (TypeConstraint::Nullable(_), TypeConstraint::Variable(_)) => todo!(),
        (TypeConstraint::Nullable(_), TypeConstraint::ElementOf(_)) => todo!(),
        (TypeConstraint::Nullable(_), TypeConstraint::FieldOf { target_type, path }) => todo!(),
        (
            TypeConstraint::Nullable(_),
            TypeConstraint::WithFieldOverrides {
                target_type,
                fields,
            },
        ) => todo!(),
        (TypeConstraint::Predicate { object_type_name }, TypeConstraint::Scalar(_)) => todo!(),
        (TypeConstraint::Predicate { object_type_name }, TypeConstraint::Object(_)) => todo!(),
        (TypeConstraint::Predicate { object_type_name }, TypeConstraint::ArrayOf(_)) => todo!(),
        (TypeConstraint::Predicate { object_type_name }, TypeConstraint::Nullable(_)) => todo!(),
        (
            TypeConstraint::Predicate { object_type_name },
            TypeConstraint::Predicate { object_type_name },
        ) => todo!(),
        (TypeConstraint::Predicate { object_type_name }, TypeConstraint::Variable(_)) => todo!(),
        (TypeConstraint::Predicate { object_type_name }, TypeConstraint::ElementOf(_)) => todo!(),
        (
            TypeConstraint::Predicate { object_type_name },
            TypeConstraint::FieldOf { target_type, path },
        ) => todo!(),
        (
            TypeConstraint::Predicate { object_type_name },
            TypeConstraint::WithFieldOverrides {
                target_type,
                fields,
            },
        ) => todo!(),
        (TypeConstraint::Variable(_), TypeConstraint::Scalar(_)) => todo!(),
        (TypeConstraint::Variable(_), TypeConstraint::Object(_)) => todo!(),
        (TypeConstraint::Variable(_), TypeConstraint::ArrayOf(_)) => todo!(),
        (TypeConstraint::Variable(_), TypeConstraint::Nullable(_)) => todo!(),
        (TypeConstraint::Variable(_), TypeConstraint::Predicate { object_type_name }) => todo!(),
        (TypeConstraint::Variable(_), TypeConstraint::Variable(_)) => todo!(),
        (TypeConstraint::Variable(_), TypeConstraint::ElementOf(_)) => todo!(),
        (TypeConstraint::Variable(_), TypeConstraint::FieldOf { target_type, path }) => todo!(),
        (
            TypeConstraint::Variable(_),
            TypeConstraint::WithFieldOverrides {
                target_type,
                fields,
            },
        ) => todo!(),
        (TypeConstraint::ElementOf(_), TypeConstraint::Scalar(_)) => todo!(),
        (TypeConstraint::ElementOf(_), TypeConstraint::Object(_)) => todo!(),
        (TypeConstraint::ElementOf(_), TypeConstraint::ArrayOf(_)) => todo!(),
        (TypeConstraint::ElementOf(_), TypeConstraint::Nullable(_)) => todo!(),
        (TypeConstraint::ElementOf(_), TypeConstraint::Predicate { object_type_name }) => todo!(),
        (TypeConstraint::ElementOf(_), TypeConstraint::Variable(_)) => todo!(),
        (TypeConstraint::ElementOf(_), TypeConstraint::ElementOf(_)) => todo!(),
        (TypeConstraint::ElementOf(_), TypeConstraint::FieldOf { target_type, path }) => todo!(),
        (
            TypeConstraint::ElementOf(_),
            TypeConstraint::WithFieldOverrides {
                target_type,
                fields,
            },
        ) => todo!(),
        (TypeConstraint::FieldOf { target_type, path }, TypeConstraint::Scalar(_)) => todo!(),
        (TypeConstraint::FieldOf { target_type, path }, TypeConstraint::Object(_)) => todo!(),
        (TypeConstraint::FieldOf { target_type, path }, TypeConstraint::ArrayOf(_)) => todo!(),
        (TypeConstraint::FieldOf { target_type, path }, TypeConstraint::Nullable(_)) => todo!(),
        (
            TypeConstraint::FieldOf { target_type, path },
            TypeConstraint::Predicate { object_type_name },
        ) => todo!(),
        (TypeConstraint::FieldOf { target_type, path }, TypeConstraint::Variable(_)) => todo!(),
        (TypeConstraint::FieldOf { target_type, path }, TypeConstraint::ElementOf(_)) => todo!(),
        (
            TypeConstraint::FieldOf { target_type, path },
            TypeConstraint::FieldOf { target_type, path },
        ) => todo!(),
        (
            TypeConstraint::FieldOf { target_type, path },
            TypeConstraint::WithFieldOverrides {
                target_type,
                fields,
            },
        ) => todo!(),
        (
            TypeConstraint::WithFieldOverrides {
                target_type,
                fields,
            },
            TypeConstraint::Scalar(_),
        ) => todo!(),
        (
            TypeConstraint::WithFieldOverrides {
                target_type,
                fields,
            },
            TypeConstraint::Object(_),
        ) => todo!(),
        (
            TypeConstraint::WithFieldOverrides {
                target_type,
                fields,
            },
            TypeConstraint::ArrayOf(_),
        ) => todo!(),
        (
            TypeConstraint::WithFieldOverrides {
                target_type,
                fields,
            },
            TypeConstraint::Nullable(_),
        ) => todo!(),
        (
            TypeConstraint::WithFieldOverrides {
                target_type,
                fields,
            },
            TypeConstraint::Predicate { object_type_name },
        ) => todo!(),
        (
            TypeConstraint::WithFieldOverrides {
                target_type,
                fields,
            },
            TypeConstraint::Variable(_),
        ) => todo!(),
        (
            TypeConstraint::WithFieldOverrides {
                target_type,
                fields,
            },
            TypeConstraint::ElementOf(_),
        ) => todo!(),
        (
            TypeConstraint::WithFieldOverrides {
                target_type,
                fields,
            },
            TypeConstraint::FieldOf { target_type, path },
        ) => todo!(),
        (
            TypeConstraint::WithFieldOverrides {
                target_type,
                fields,
            },
            TypeConstraint::WithFieldOverrides {
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
