#![allow(warnings)]

use std::collections::{BTreeMap, HashSet};

use configuration::schema::{ObjectType, Type};
use configuration::Configuration;
use itertools::Itertools;
use mongodb_support::align::try_align;
use mongodb_support::BsonScalarType;
use ndc_models::{FieldName, ObjectTypeName};

use crate::introspection::type_unification::is_supertype;

use crate::native_query::type_constraint::Variance;
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
    variance: Variance,
    constraints: impl IntoIterator<Item = TypeConstraint>,
) -> HashSet<TypeConstraint> {
    constraints
        .into_iter()
        .coalesce(|constraint_a, constraint_b| {
            simplify_constraint_pair(
                configuration,
                object_type_constraints,
                variance,
                constraint_a,
                constraint_b,
            )
        })
        .collect()
}

fn simplify_constraint_pair(
    configuration: &Configuration,
    object_type_constraints: &mut BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    variance: Variance,
    a: TypeConstraint,
    b: TypeConstraint,
) -> Simplified<TypeConstraint> {
    match (a, b) {
        (C::ExtendedJSON, _) | (_, C::ExtendedJSON) if variance == Variance::Covariant => {
            Ok(C::ExtendedJSON)
        }
        (C::ExtendedJSON, b) if variance == Variance::Contravariant => Ok(b),
        (a, C::ExtendedJSON) if variance == Variance::Contravariant => Ok(a),

        (C::Scalar(a), C::Scalar(b)) => solve_scalar(variance, a, b),

        // TODO: We need to make sure we aren't putting multiple layers of Nullable on constraints
        // - if a and b have mismatched levels of Nullable they won't unify
        (C::Nullable(a), C::Nullable(b)) => {
            simplify_constraint_pair(configuration, object_type_constraints, variance, *a, *b)
                .map(|constraint| C::Nullable(Box::new(constraint)))
        }
        (C::Nullable(a), b) if variance == Variance::Covariant => {
            simplify_constraint_pair(configuration, object_type_constraints, variance, *a, b)
                .map(|constraint| C::Nullable(Box::new(constraint)))
        }
        (a, b @ C::Nullable(_)) => {
            simplify_constraint_pair(configuration, object_type_constraints, variance, b, a)
        }

        (C::Variable(a), C::Variable(b)) if a == b => Ok(C::Variable(a)),

        (C::Object(a), C::Object(b)) if a == b => Ok(C::Object(a)),
        (C::Object(a), C::Object(b)) => {
            match merge_object_type_constraints(
                configuration,
                object_type_constraints,
                variance,
                &a,
                &b,
            ) {
                Some(merged_name) => Ok(C::Object(merged_name)),
                None => Err((C::Object(a), C::Object(b))),
            }
        }

        (
            C::Predicate {
                object_type_name: a,
            },
            C::Predicate {
                object_type_name: b,
            },
        ) if a == b => Ok(C::Predicate {
            object_type_name: a,
        }),
        (
            C::Predicate {
                object_type_name: a,
            },
            C::Predicate {
                object_type_name: b,
            },
        ) if a == b => match merge_object_type_constraints(
            configuration,
            object_type_constraints,
            variance,
            &a,
            &b,
        ) {
            Some(merged_name) => Ok(C::Predicate {
                object_type_name: merged_name,
            }),
            None => Err((
                C::Predicate {
                    object_type_name: a,
                },
                C::Predicate {
                    object_type_name: b,
                },
            )),
        },

        // TODO: We probably want a separate step that swaps ElementOf and FieldOf constraints with
        // constraint of the targeted structure. We might do a similar thing with
        // WithFieldOverrides.

        // (C::ElementOf(a), b) => {
        //     if let TypeConstraint::ArrayOf(elem_type) = *a {
        //         simplify_constraint_pair(
        //             configuration,
        //             object_type_constraints,
        //             variance,
        //             *elem_type,
        //             b,
        //         )
        //     } else {
        //         Err((C::ElementOf(a), b))
        //     }
        // }
        //
        // (C::FieldOf { target_type, path }, b) => {
        //     if let TypeConstraint::Object(type_name) = *target_type {
        //         let object_type = object_type_constraints
        //     } else {
        //         Err((C::FieldOf { target_type, path }, b))
        //     }
        // }

        // (
        //     C::Object(_),
        //     C::WithFieldOverrides {
        //         target_type,
        //         fields,
        //         ..
        //     },
        // ) => todo!(),
        (C::ArrayOf(a), C::ArrayOf(b)) => {
            match simplify_constraint_pair(configuration, object_type_constraints, variance, *a, *b)
            {
                Ok(ab) => Ok(C::ArrayOf(Box::new(ab))),
                Err((a, b)) => Err((C::ArrayOf(Box::new(a)), C::ArrayOf(Box::new(b)))),
            }
        }

        (a, b) => Err((a, b)),
    }
}

fn solve_scalar(
    variance: Variance,
    a: BsonScalarType,
    b: BsonScalarType,
) -> Simplified<TypeConstraint> {
    if variance == Variance::Contravariant {
        return solve_scalar(Variance::Covariant, b, a);
    }

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
    variance: Variance,
    name_a: &ObjectTypeName,
    name_b: &ObjectTypeName,
) -> Option<ObjectTypeName> {
    // Pick from the two input names according to sort order to get a deterministic outcome.
    let preferred_name = if name_a <= name_b { name_a } else { name_b };
    let merged_name = unique_type_name(configuration, object_type_constraints, preferred_name);

    let a = look_up_object_type_constraint(configuration, object_type_constraints, name_a);
    let b = look_up_object_type_constraint(configuration, object_type_constraints, name_b);

    let merged_fields_result = try_align(
        a.fields.clone().into_iter().collect(),
        b.fields.clone().into_iter().collect(),
        always_ok(TypeConstraint::make_nullable),
        always_ok(TypeConstraint::make_nullable),
        |field_a, field_b| {
            unify_object_field(
                configuration,
                object_type_constraints,
                variance,
                field_a,
                field_b,
            )
        },
    );

    let fields = match merged_fields_result {
        Ok(merged_fields) => merged_fields.into_iter().collect(),
        Err(_) => {
            return None;
        }
    };

    let merged_object_type = ObjectTypeConstraint { fields };
    object_type_constraints.insert(merged_name.clone(), merged_object_type);

    Some(merged_name)
}

fn unify_object_field(
    configuration: &Configuration,
    object_type_constraints: &mut BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    variance: Variance,
    field_type_a: TypeConstraint,
    field_type_b: TypeConstraint,
) -> Result<TypeConstraint, ()> {
    simplify_constraint_pair(
        configuration,
        object_type_constraints,
        variance,
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
