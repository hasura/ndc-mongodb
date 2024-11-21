use std::collections::{BTreeMap, BTreeSet, HashMap};

use configuration::Configuration;
use itertools::Itertools as _;
use mongodb_support::align::try_align;
use mongodb_support::BsonScalarType;
use ndc_models::{FieldName, ObjectTypeName};
use nonempty::NonEmpty;

use crate::introspection::type_unification::is_supertype;

use crate::native_query::helpers::get_object_field_type;
use crate::native_query::type_constraint::Variance;
use crate::native_query::{
    error::Error,
    type_constraint::{ObjectTypeConstraint, TypeConstraint, TypeVariable},
};

use TypeConstraint as C;

type Simplified<T> = std::result::Result<T, (T, T)>;

struct SimplifyContext<'a> {
    configuration: &'a Configuration,
    substitutions: &'a HashMap<TypeVariable, BTreeSet<TypeConstraint>>,
    object_type_constraints: &'a mut BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    errors: &'a mut Vec<Error>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OnMismatch {
    ReportError,
    Ignore,
}

// Attempts to reduce the number of type constraints from the input by combining redundant
// constraints, merging constraints into more specific ones where possible, and applying
// accumulated variable substitutions.
pub fn simplify_constraints(
    configuration: &Configuration,
    substitutions: &HashMap<TypeVariable, BTreeSet<TypeConstraint>>,
    object_type_constraints: &mut BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    variable: Option<TypeVariable>,
    constraints: impl IntoIterator<Item = TypeConstraint>,
) -> Result<BTreeSet<TypeConstraint>, Vec<Error>> {
    let mut errors = vec![];
    let mut context = SimplifyContext {
        configuration,
        substitutions,
        object_type_constraints,
        errors: &mut errors,
    };
    let constraints =
        simplify_constraints_internal(&mut context, variable, OnMismatch::ReportError, constraints);
    if errors.is_empty() {
        Ok(constraints)
    } else {
        Err(errors)
    }
}

fn simplify_constraints_internal(
    context: &mut SimplifyContext,
    variable: Option<TypeVariable>,
    on_mismatch: OnMismatch, // don't emit errors when trying to simplify within a union or oneof
    constraints: impl IntoIterator<Item = TypeConstraint>,
) -> BTreeSet<TypeConstraint> {
    let constraints: BTreeSet<_> = constraints
        .into_iter()
        .flat_map(|constraint| simplify_single_constraint(context, variable, constraint))
        .collect();

    constraints
        .into_iter()
        .coalesce(|constraint_a, constraint_b| {
            simplify_constraint_pair(context, variable, on_mismatch, constraint_a, constraint_b)
        })
        .collect()
}

fn simplify_single_constraint(
    context: &mut SimplifyContext,
    variable: Option<TypeVariable>,
    constraint: TypeConstraint,
) -> Vec<TypeConstraint> {
    match constraint {
        C::Variable(v) if Some(v) == variable => vec![],

        C::Variable(v) => match context.substitutions.get(&v) {
            Some(constraints) => constraints.iter().cloned().collect(),
            None => vec![C::Variable(v)],
        },

        C::FieldOf { target_type, path } => {
            let object_type = simplify_single_constraint(context, variable, *target_type.clone());
            if object_type.len() == 1 {
                let object_type = object_type.into_iter().next().unwrap();
                match expand_field_of(context, object_type, path.clone()) {
                    Ok(Some(t)) => return t,
                    Ok(None) => (),
                    Err(e) => context.errors.push(e),
                }
            }
            vec![C::FieldOf { target_type, path }]
        }

        C::Union(constraints) => {
            let simplified_constraints =
                simplify_constraints_internal(context, variable, OnMismatch::Ignore, constraints);
            vec![C::Union(simplified_constraints)]
        }

        C::OneOf(constraints) => {
            let simplified_constraints =
                simplify_constraints_internal(context, variable, OnMismatch::Ignore, constraints);
            vec![C::OneOf(simplified_constraints)]
        }

        _ => vec![constraint],
    }
}

// TODO: Change the signature to distinguish between cases where we have provable type mismatches,
// vs cases where we cannot combine constraints due to lack of information. Then use that
// information to report type mismatches in more cases, such as when simplifying a scalar type and
// a union in a contravariant or invariant context where the scalar is not compatibly with any
// branch of the union (e.g. `string` and `(decimal | null)`).
fn simplify_constraint_pair(
    context: &mut SimplifyContext,
    variable: Option<TypeVariable>,
    on_mismatch: OnMismatch, // don't emit errors when trying to simplify within a union or oneof
    a: TypeConstraint,
    b: TypeConstraint,
) -> Simplified<TypeConstraint> {
    let variance = variable.map(|v| v.variance).unwrap_or(Variance::Invariant);
    match (a, b) {
        (a, b) if a == b => Ok(a),

        (C::Variable(a), C::Variable(b)) if a == b => Ok(C::Variable(a)),

        (C::ExtendedJSON, _) | (_, C::ExtendedJSON) if variance == Variance::Covariant => {
            Ok(C::ExtendedJSON)
        }
        (C::ExtendedJSON, b) if variance == Variance::Contravariant => Ok(b),
        (a, C::ExtendedJSON) if variance == Variance::Contravariant => Ok(a),

        (C::Scalar(a), C::Scalar(b)) => {
            if let Ok(t) = solve_scalar(variance, a, b) {
                Ok(t)
            } else if variance == Variance::Covariant {
                Ok(C::Union([C::Scalar(a), C::Scalar(b)].into()))
            } else {
                if on_mismatch == OnMismatch::ReportError {
                    context.errors.push(Error::TypeMismatch {
                        context: None,
                        a: C::Scalar(a),
                        b: C::Scalar(b),
                    });
                }
                Err((C::Scalar(a), C::Scalar(b)))
            }
        }

        (C::Union(mut a), C::Union(mut b)) if variance == Variance::Covariant => {
            a.append(&mut b);
            let union = simplify_constraints_internal(context, variable, OnMismatch::Ignore, a);
            Ok(C::Union(union))
        }

        // TODO: Instead of a naive intersection we want to get a common subtype of both unions
        (C::Union(a), C::Union(b)) if variance == Variance::Contravariant => {
            let intersection: BTreeSet<_> = a.intersection(&b).cloned().collect();
            if intersection.is_empty() {
                Err((C::Union(a), C::Union(b)))
            } else if intersection.len() == 1 {
                Ok(intersection.into_iter().next().unwrap())
            } else {
                Ok(C::Union(intersection))
            }
        }

        (C::Union(mut a), b) if variance == Variance::Covariant => {
            a.insert(b);
            let union = simplify_constraints_internal(context, variable, OnMismatch::Ignore, a);
            Ok(C::Union(union))
        }
        (a, b @ C::Union(_)) => simplify_constraint_pair(context, variable, on_mismatch, b, a),

        (C::OneOf(mut a), C::OneOf(mut b)) => {
            a.append(&mut b);
            Ok(C::OneOf(a))
        }

        (C::OneOf(constraints), b) => {
            let matches: BTreeSet<_> = constraints
                .clone()
                .into_iter()
                .filter_map(|c| {
                    match simplify_constraint_pair(
                        context,
                        variable,
                        OnMismatch::Ignore,
                        c,
                        b.clone(),
                    ) {
                        Ok(c) => Some(c),
                        Err(_) => None,
                    }
                })
                .collect();

            if matches.len() == 1 {
                Ok(matches.into_iter().next().unwrap())
            } else if matches.is_empty() {
                Err((C::OneOf(constraints), b))
            } else {
                Ok(C::OneOf(matches))
            }
        }
        (a, b @ C::OneOf(_)) => simplify_constraint_pair(context, variable, on_mismatch, b, a),

        (C::Object(a), C::Object(b)) if a == b => Ok(C::Object(a)),
        (C::Object(a), C::Object(b)) => {
            match merge_object_type_constraints(context, variable, on_mismatch, &a, &b) {
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
        ) if a == b => {
            match merge_object_type_constraints(context, variable, on_mismatch, &a, &b) {
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
            }
        }

        (C::ArrayOf(a), C::ArrayOf(b)) => {
            match simplify_constraint_pair(context, variable, on_mismatch, *a, *b) {
                Ok(ab) => Ok(C::ArrayOf(Box::new(ab))),
                Err((a, b)) => Err((C::ArrayOf(Box::new(a)), C::ArrayOf(Box::new(b)))),
            }
        }

        (a, b) => Err((a, b)),
    }
}

/// Reconciles two scalar type constraints depending on variance of the context. In a covariant
/// context the type of a type variable is determined to be the supertype of the two (if the types
/// overlap). In a covariant context the variable type is the subtype of the two instead.
fn solve_scalar(
    variance: Variance,
    a: BsonScalarType,
    b: BsonScalarType,
) -> Simplified<TypeConstraint> {
    match variance {
        Variance::Covariant => {
            if a == b || is_supertype(&a, &b) {
                Ok(C::Scalar(a))
            } else if is_supertype(&b, &a) {
                Ok(C::Scalar(b))
            } else {
                Err((C::Scalar(a), C::Scalar(b)))
            }
        }
        Variance::Contravariant => {
            if a == b || is_supertype(&a, &b) {
                Ok(C::Scalar(b))
            } else if is_supertype(&b, &a) {
                Ok(C::Scalar(a))
            } else {
                Err((C::Scalar(a), C::Scalar(b)))
            }
        }
        Variance::Invariant => {
            if a == b {
                Ok(C::Scalar(a))
            } else {
                Err((C::Scalar(a), C::Scalar(b)))
            }
        }
    }
}

fn merge_object_type_constraints(
    context: &mut SimplifyContext,
    variable: Option<TypeVariable>,
    on_mismatch: OnMismatch,
    name_a: &ObjectTypeName,
    name_b: &ObjectTypeName,
) -> Option<ObjectTypeName> {
    // Pick from the two input names according to sort order to get a deterministic outcome.
    let preferred_name = if name_a <= name_b { name_a } else { name_b };
    let merged_name = unique_type_name(
        context.configuration,
        context.object_type_constraints,
        preferred_name,
    );

    let a = look_up_object_type_constraint(context, name_a);
    let b = look_up_object_type_constraint(context, name_b);

    let merged_fields_result = try_align(
        a.fields.clone().into_iter().collect(),
        b.fields.clone().into_iter().collect(),
        always_ok(TypeConstraint::make_nullable),
        always_ok(TypeConstraint::make_nullable),
        |field_a, field_b| unify_object_field(context, variable, on_mismatch, field_a, field_b),
    );

    let fields = match merged_fields_result {
        Ok(merged_fields) => merged_fields.into_iter().collect(),
        Err(_) => {
            return None;
        }
    };

    let merged_object_type = ObjectTypeConstraint { fields };
    context
        .object_type_constraints
        .insert(merged_name.clone(), merged_object_type);

    Some(merged_name)
}

fn unify_object_field(
    context: &mut SimplifyContext,
    variable: Option<TypeVariable>,
    on_mismatch: OnMismatch,
    field_type_a: TypeConstraint,
    field_type_b: TypeConstraint,
) -> Result<TypeConstraint, ()> {
    simplify_constraint_pair(context, variable, on_mismatch, field_type_a, field_type_b)
        .map_err(|_| ())
}

fn always_ok<A, B, E, F>(mut f: F) -> impl FnMut(A) -> Result<B, E>
where
    F: FnMut(A) -> B,
{
    move |x| Ok(f(x))
}

fn look_up_object_type_constraint(
    context: &SimplifyContext,
    name: &ObjectTypeName,
) -> ObjectTypeConstraint {
    if let Some(object_type) = context.configuration.object_types.get(name) {
        object_type.clone().into()
    } else if let Some(object_type) = context.object_type_constraints.get(name) {
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

fn expand_field_of(
    context: &mut SimplifyContext,
    object_type: TypeConstraint,
    path: NonEmpty<FieldName>,
) -> Result<Option<Vec<TypeConstraint>>, Error> {
    let field_type = match object_type {
        C::ExtendedJSON => Some(vec![C::ExtendedJSON]),
        C::Object(type_name) => get_object_constraint_field_type(context, &type_name, path)?,
        C::Union(constraints) => {
            let variants: BTreeSet<TypeConstraint> = constraints
                .into_iter()
                .map(|t| {
                    let maybe_expanded = expand_field_of(context, t.clone(), path.clone())?;

                    // TODO: if variant has more than one element that should be interpreted as an
                    // intersection, which we haven't implemented yet
                    Ok(match maybe_expanded {
                        Some(variant) if variant.len() <= 1 => variant,
                        _ => vec![t],
                    })
                })
                .flatten_ok()
                .collect::<Result<_, Error>>()?;
            Some(vec![(C::Union(variants))])
        }
        C::OneOf(constraints) => {
            // The difference between the Union and OneOf cases is that in OneOf we want to prune
            // variants that don't expand, while in Union we want to preserve unexpanded variants.
            let expanded_variants: BTreeSet<TypeConstraint> = constraints
                .into_iter()
                .map(|t| {
                    let maybe_expanded = expand_field_of(context, t, path.clone())?;

                    // TODO: if variant has more than one element that should be interpreted as an
                    // intersection, which we haven't implemented yet
                    Ok(match maybe_expanded {
                        Some(variant) if variant.len() <= 1 => variant,
                        _ => vec![],
                    })
                })
                .flatten_ok()
                .collect::<Result<_, Error>>()?;
            if expanded_variants.len() == 1 {
                Some(vec![expanded_variants.into_iter().next().unwrap()])
            } else if !expanded_variants.is_empty() {
                Some(vec![C::Union(expanded_variants)])
            } else {
                Err(Error::Other(format!(
                    "no variant matched object field path {path:?}"
                )))?
            }
        }
        _ => None,
    };
    Ok(field_type)
}

fn get_object_constraint_field_type(
    context: &mut SimplifyContext,
    object_type_name: &ObjectTypeName,
    path: NonEmpty<FieldName>,
) -> Result<Option<Vec<TypeConstraint>>, Error> {
    if let Some(object_type) = context.configuration.object_types.get(object_type_name) {
        let t = get_object_field_type(
            &context.configuration.object_types,
            object_type_name,
            object_type,
            path,
        )?;
        return Ok(Some(vec![t.clone().into()]));
    }

    let Some(object_type_constraint) = context.object_type_constraints.get(object_type_name) else {
        return Err(Error::UnknownObjectType(object_type_name.to_string()));
    };

    let field_name = path.head;
    let rest = NonEmpty::from_vec(path.tail);

    let field_type = object_type_constraint
        .fields
        .get(&field_name)
        .ok_or_else(|| Error::ObjectMissingField {
            object_type: object_type_name.clone(),
            field_name: field_name.clone(),
        })?
        .clone();

    let field_type = simplify_single_constraint(context, None, field_type);

    match rest {
        None => Ok(Some(field_type)),
        Some(rest) if field_type.len() == 1 => match field_type.into_iter().next().unwrap() {
            C::Object(type_name) => get_object_constraint_field_type(context, &type_name, rest),
            _ => Err(Error::ObjectMissingField {
                object_type: object_type_name.clone(),
                field_name: field_name.clone(),
            }),
        },
        _ if field_type.is_empty() => Err(Error::Other(
            "could not resolve object field to a type".to_string(),
        )),
        _ => Ok(None), // field_type len > 1
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use googletest::prelude::*;
    use mongodb_support::BsonScalarType;
    use nonempty::nonempty;
    use test_helpers::configuration::mflix_config;

    use crate::native_query::{
        error::Error,
        type_constraint::{TypeConstraint, TypeVariable, Variance},
    };

    #[googletest::test]
    fn multiple_identical_scalar_constraints_resolve_one_constraint() {
        expect_eq!(
            super::solve_scalar(
                Variance::Covariant,
                BsonScalarType::String,
                BsonScalarType::String,
            ),
            Ok(TypeConstraint::Scalar(BsonScalarType::String))
        );
        expect_eq!(
            super::solve_scalar(
                Variance::Contravariant,
                BsonScalarType::String,
                BsonScalarType::String,
            ),
            Ok(TypeConstraint::Scalar(BsonScalarType::String))
        );
    }

    #[googletest::test]
    fn multiple_scalar_constraints_resolve_to_supertype_in_covariant_context() {
        expect_eq!(
            super::solve_scalar(
                Variance::Covariant,
                BsonScalarType::Int,
                BsonScalarType::Double,
            ),
            Ok(TypeConstraint::Scalar(BsonScalarType::Double))
        );
    }

    #[googletest::test]
    fn multiple_scalar_constraints_resolve_to_subtype_in_contravariant_context() {
        expect_eq!(
            super::solve_scalar(
                Variance::Contravariant,
                BsonScalarType::Int,
                BsonScalarType::Double,
            ),
            Ok(TypeConstraint::Scalar(BsonScalarType::Int))
        );
    }

    #[googletest::test]
    fn simplifies_field_of() -> Result<()> {
        let config = mflix_config();
        let result = super::simplify_constraints(
            &config,
            &Default::default(),
            &mut Default::default(),
            Some(TypeVariable::new(1, Variance::Covariant)),
            [TypeConstraint::FieldOf {
                target_type: Box::new(TypeConstraint::Object("movies".into())),
                path: nonempty!["title".into()],
            }],
        );
        expect_that!(
            result,
            matches_pattern!(Ok(&BTreeSet::from_iter([TypeConstraint::Scalar(
                BsonScalarType::String
            )])))
        );
        Ok(())
    }

    #[googletest::test]
    fn nullable_union_does_not_error_and_does_not_simplify() -> Result<()> {
        let configuration = mflix_config();
        let result = super::simplify_constraints(
            &configuration,
            &Default::default(),
            &mut Default::default(),
            Some(TypeVariable::new(1, Variance::Contravariant)),
            [TypeConstraint::Union(
                [
                    TypeConstraint::Scalar(BsonScalarType::Int),
                    TypeConstraint::Scalar(BsonScalarType::Null),
                ]
                .into(),
            )],
        );
        expect_that!(
            result,
            ok(eq(&BTreeSet::from([TypeConstraint::Union(
                [
                    TypeConstraint::Scalar(BsonScalarType::Int),
                    TypeConstraint::Scalar(BsonScalarType::Null),
                ]
                .into(),
            )])))
        );
        Ok(())
    }

    #[googletest::test]
    fn emits_error_if_scalar_is_not_compatible_with_any_union_branch() -> Result<()> {
        let configuration = mflix_config();
        let result = super::simplify_constraints(
            &configuration,
            &Default::default(),
            &mut Default::default(),
            Some(TypeVariable::new(1, Variance::Contravariant)),
            [
                TypeConstraint::Scalar(BsonScalarType::Decimal),
                TypeConstraint::Union(
                    [
                        TypeConstraint::Scalar(BsonScalarType::String),
                        TypeConstraint::Scalar(BsonScalarType::Null),
                    ]
                    .into(),
                ),
            ],
        );
        expect_that!(
            result,
            err(elements_are![matches_pattern!(Error::TypeMismatch { .. })])
        );
        Ok(())
    }
}
