mod constraint_to_type;
mod simplify;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use configuration::{
    schema::{ObjectType, Type},
    Configuration,
};
use itertools::Itertools;
use ndc_models::ObjectTypeName;
use simplify::simplify_constraints;

use super::{
    error::{Error, Result},
    type_constraint::{ObjectTypeConstraint, TypeConstraint, TypeVariable},
};

use self::constraint_to_type::constraint_to_type;

pub fn unify(
    configuration: &Configuration,
    required_type_variables: &[TypeVariable],
    object_type_constraints: &mut BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    type_variables: HashMap<TypeVariable, BTreeSet<TypeConstraint>>,
) -> Result<(
    HashMap<TypeVariable, Type>,
    BTreeMap<ObjectTypeName, ObjectType>,
)> {
    let mut added_object_types = BTreeMap::new();
    let mut solutions = HashMap::new();
    let mut substitutions = HashMap::new();
    fn is_solved(solutions: &HashMap<TypeVariable, Type>, variable: TypeVariable) -> bool {
        solutions.contains_key(&variable)
    }

    #[cfg(test)]
    println!("begin unify:\n  type_variables: {type_variables:?}\n  object_type_constraints: {object_type_constraints:?}\n");

    loop {
        let prev_type_variables = type_variables.clone();
        let prev_solutions = solutions.clone();
        let prev_substitutions = substitutions.clone();

        // TODO: check for mismatches, e.g. constraint list contains scalar & array ENG-1252

        for (variable, constraints) in type_variables.iter() {
            if is_solved(&solutions, *variable) {
                continue;
            }

            let simplified = simplify_constraints(
                configuration,
                &substitutions,
                object_type_constraints,
                *variable,
                constraints.iter().cloned(),
            );
            #[cfg(test)]
            if simplified != *constraints {
                println!("simplified {variable}: {constraints:?} -> {simplified:?}");
            }
            if simplified.len() == 1 {
                let constraint = simplified.iter().next().unwrap();
                if let Some(solved_type) = constraint_to_type(
                    configuration,
                    &solutions,
                    &mut added_object_types,
                    object_type_constraints,
                    constraint,
                )? {
                    #[cfg(test)]
                    println!("solved {variable}: {solved_type:?}");
                    solutions.insert(*variable, solved_type.clone());
                    substitutions.insert(*variable, [solved_type.into()].into());
                }
            }
        }

        #[cfg(test)]
        println!("added_object_types: {added_object_types:?}\n");

        let variables = type_variables_by_complexity(&type_variables);
        if let Some(v) = variables.iter().find(|v| !substitutions.contains_key(*v)) {
            // TODO: We should do some recursion to substitute variable references within
            // substituted constraints to existing substitutions.
            substitutions.insert(*v, type_variables[v].clone());
        }

        if required_type_variables
            .iter()
            .copied()
            .all(|v| is_solved(&solutions, v))
        {
            return Ok((solutions, added_object_types));
        }

        if type_variables == prev_type_variables
            && solutions == prev_solutions
            && substitutions == prev_substitutions
        {
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
    type_variables: &HashMap<TypeVariable, BTreeSet<TypeConstraint>>,
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

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use configuration::schema::{ObjectField, ObjectType, Type};
    use googletest::prelude::*;
    use mongodb_support::BsonScalarType;
    use nonempty::nonempty;
    use pretty_assertions::assert_eq;
    use test_helpers::configuration::mflix_config;

    use crate::native_query::type_constraint::{
        ObjectTypeConstraint, TypeConstraint, TypeVariable, Variance,
    };

    use super::unify;

    use TypeConstraint as C;

    #[test]
    fn solves_object_type() -> Result<()> {
        let configuration = mflix_config();
        let type_variable = TypeVariable::new(0, Variance::Covariant);
        let required_type_variables = [type_variable];
        let mut object_type_constraints = Default::default();

        let type_variables = [(
            type_variable,
            [TypeConstraint::Object("movies".into())].into(),
        )]
        .into();

        let (solved_variables, _) = unify(
            &configuration,
            &required_type_variables,
            &mut object_type_constraints,
            type_variables,
        )?;

        assert_eq!(
            solved_variables,
            [(type_variable, Type::Object("movies".into()))].into()
        );

        Ok(())
    }

    #[test]
    fn solves_added_object_type_based_on_object_type_constraint() -> Result<()> {
        let configuration = mflix_config();
        let type_variable = TypeVariable::new(0, Variance::Covariant);
        let required_type_variables = [type_variable];

        let mut object_type_constraints = [(
            "new_object_type".into(),
            ObjectTypeConstraint {
                fields: [("foo".into(), TypeConstraint::Scalar(BsonScalarType::Int))].into(),
            },
        )]
        .into();

        let type_variables = [(
            type_variable,
            [TypeConstraint::Object("new_object_type".into())].into(),
        )]
        .into();

        let (solved_variables, added_object_types) = unify(
            &configuration,
            &required_type_variables,
            &mut object_type_constraints,
            type_variables,
        )?;

        assert_eq!(
            solved_variables,
            [(type_variable, Type::Object("new_object_type".into()))].into()
        );
        assert_eq!(
            added_object_types,
            [(
                "new_object_type".into(),
                ObjectType {
                    fields: [(
                        "foo".into(),
                        ObjectField {
                            r#type: Type::Scalar(BsonScalarType::Int),
                            description: None
                        }
                    )]
                    .into(),
                    description: None
                }
            )]
            .into(),
        );

        Ok(())
    }

    #[test]
    fn produces_object_type_based_on_field_type_of_another_object_type() -> Result<()> {
        let configuration = mflix_config();
        let var0 = TypeVariable::new(0, Variance::Covariant);
        let var1 = TypeVariable::new(1, Variance::Covariant);
        let required_type_variables = [var0, var1];

        let mut object_type_constraints = [(
            "movies_selection_stage0".into(),
            ObjectTypeConstraint {
                fields: [(
                    "selected_title".into(),
                    TypeConstraint::FieldOf {
                        target_type: Box::new(TypeConstraint::Variable(var0)),
                        path: nonempty!["title".into()],
                    },
                )]
                .into(),
            },
        )]
        .into();

        let type_variables = [
            (var0, [TypeConstraint::Object("movies".into())].into()),
            (
                var1,
                [TypeConstraint::Object("movies_selection_stage0".into())].into(),
            ),
        ]
        .into();

        let (solved_variables, added_object_types) = unify(
            &configuration,
            &required_type_variables,
            &mut object_type_constraints,
            type_variables,
        )?;

        assert_eq!(
            solved_variables.get(&var1),
            Some(&Type::Object("movies_selection_stage0".into()))
        );
        assert_eq!(
            added_object_types.get("movies_selection_stage0"),
            Some(&ObjectType {
                fields: [(
                    "selected_title".into(),
                    ObjectField {
                        r#type: Type::Scalar(BsonScalarType::String),
                        description: None
                    }
                )]
                .into(),
                description: None
            })
        );

        Ok(())
    }
}
