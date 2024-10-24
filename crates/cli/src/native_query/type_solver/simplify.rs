#![allow(warnings)]

use std::collections::{BTreeMap, HashSet};

use configuration::schema::{ObjectType, Type};
use configuration::Configuration;
use itertools::Itertools;
use mongodb_support::align::try_align;
use mongodb_support::BsonScalarType;
use ndc_models::{FieldName, ObjectTypeName};

use crate::introspection::type_unification::is_supertype;

use crate::native_query::{
    error::Error,
    pipeline_type_context::PipelineTypeContext,
    type_constraint::{ObjectTypeConstraint, TypeConstraint, TypeVariable},
};

use TypeConstraint as C;

type Simplified<T> = std::result::Result<T, (T, T)>;

// Attempts to reduce the number of type constraints from the input by combining redundant
// constraints, and by merging constraints into more specific ones where possible. This is
// guaranteed to produce a list that is equal or smaller in length compared to the input.
pub fn simplify_constraints(
    configuration: &Configuration,
    object_type_constraints: &mut BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    constraints: impl IntoIterator<Item = TypeConstraint>,
) -> HashSet<TypeConstraint> {
    constraints
        .into_iter()
        .coalesce(|constraint_a, constraint_b| {
            simplify_constraint_pair(
                configuration,
                object_type_constraints,
                constraint_a,
                constraint_b,
            )
        })
        .collect()
}

fn simplify_constraint_pair(
    configuration: &Configuration,
    object_type_constraints: &mut BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    a: TypeConstraint,
    b: TypeConstraint,
) -> Simplified<TypeConstraint> {
    match (a, b) {
        (C::ExtendedJSON, _) | (_, C::ExtendedJSON) => Ok(C::ExtendedJSON),
        (C::Scalar(a), C::Scalar(b)) => solve_scalar(a, b),

        (C::Nullable(a), b) => {
            simplify_constraint_pair(
                configuration,
                object_type_constraints,
                /*type_variables,*/ *a,
                b,
            )
            .map(|constraint| C::Nullable(Box::new(constraint)))
        }
        (a, b @ C::Nullable(_)) => {
            simplify_constraint_pair(configuration, object_type_constraints, b, a)
        }

        (C::Variable(a), C::Variable(b)) if a == b => Ok(C::Variable(a)),

        // (C::Scalar(_), C::Variable(_)) => todo!(),
        // (C::Scalar(_), C::ElementOf(_)) => todo!(),
        (C::Scalar(_), C::FieldOf { target_type, path }) => todo!(),
        (
            C::Scalar(_),
            C::WithFieldOverrides {
                target_type,
                fields,
                ..
            },
        ) => todo!(),
        // (C::Object(_), C::Scalar(_)) => todo!(),
        (C::Object(a), C::Object(b)) => {
            merge_object_type_constraints(configuration, object_type_constraints, a, b)
        }
        // (C::Object(_), C::ArrayOf(_)) => todo!(),
        // (C::Object(_), C::Nullable(_)) => todo!(),
        // (C::Object(_), C::Predicate { object_type_name }) => todo!(),
        // (C::Object(_), C::Variable(_)) => todo!(),
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
        // (C::ArrayOf(_), C::Scalar(_)) => todo!(),
        // (C::ArrayOf(_), C::Object(_)) => todo!(),
        // (C::ArrayOf(_), C::ArrayOf(_)) => todo!(),
        // (C::ArrayOf(_), C::Nullable(_)) => todo!(),
        // (C::ArrayOf(_), C::Predicate { object_type_name }) => todo!(),
        // (C::ArrayOf(_), C::Variable(_)) => todo!(),
        // (C::ArrayOf(_), C::ElementOf(_)) => todo!(),
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

fn merge_object_type_constraints(
    configuration: &Configuration,
    object_type_constraints: &mut BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    name_a: ObjectTypeName,
    name_b: ObjectTypeName,
) -> Simplified<TypeConstraint> {
    // Pick from the two input names according to sort order to get a deterministic outcome.
    let preferred_name = if name_a <= name_b { &name_a } else { &name_b };
    let merged_name = unique_type_name(configuration, object_type_constraints, preferred_name);

    let a = look_up_object_type_constraint(configuration, object_type_constraints, &name_a);
    let b = look_up_object_type_constraint(configuration, object_type_constraints, &name_b);

    let merged_fields_result = try_align(
        a.fields.clone().into_iter().collect(),
        b.fields.clone().into_iter().collect(),
        always_ok(TypeConstraint::make_nullable),
        always_ok(TypeConstraint::make_nullable),
        |field_a, field_b| {
            unify_object_field(configuration, object_type_constraints, field_a, field_b)
        },
    );

    let fields = match merged_fields_result {
        Ok(merged_fields) => merged_fields.into_iter().collect(),
        Err(_) => {
            return Err((
                TypeConstraint::Object(name_a),
                TypeConstraint::Object(name_b),
            ))
        }
    };

    let merged_object_type = ObjectTypeConstraint { fields };
    object_type_constraints.insert(merged_name.clone(), merged_object_type);

    Ok(TypeConstraint::Object(merged_name))
}

fn unify_object_field(
    configuration: &Configuration,
    object_type_constraints: &mut BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    field_type_a: TypeConstraint,
    field_type_b: TypeConstraint,
) -> Result<TypeConstraint, ()> {
    simplify_constraint_pair(
        configuration,
        object_type_constraints,
        field_type_a,
        field_type_b,
    )
    .map_err(|_| ())
}

fn always_ok<A, B, E, F>(mut f: F) -> impl FnMut(A) -> Result<B, E>
where
    F: FnMut(A) -> B,
{
    move |x| Ok(f(x))
}

fn look_up_object_type_constraint(
    configuration: &Configuration,
    object_type_constraints: &BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    name: &ObjectTypeName,
) -> ObjectTypeConstraint {
    if let Some(object_type) = configuration.object_types.get(name) {
        object_type.clone().into()
    } else if let Some(object_type) = object_type_constraints.get(name) {
        object_type.clone()
    } else {
        unreachable!("look_up_object_type_constraint")
    }
}

fn unique_type_name(
    configuration: &Configuration,
    object_type_constraints: &mut BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    desired_name: &ObjectTypeName,
) -> ObjectTypeName {
    let mut counter = 0;
    let mut type_name = desired_name.clone();
    while configuration.object_types.contains_key(&type_name)
        || object_type_constraints.contains_key(&type_name)
    {
        counter += 1;
        type_name = format!("{desired_name}_{counter}").into();
    }
    type_name
}
