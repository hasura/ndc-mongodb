use std::{
    collections::{hash_map::Entry, HashMap},
    str::FromStr as _,
};

use itertools::Itertools as _;
use mongodb::bson::{Bson, Decimal128, Document};
use mongodb_support::BsonScalarType;
use ndc_models::{FieldName, ObjectTypeName};
use nonempty::{nonempty, NonEmpty};

use crate::native_query::{
    aggregation_expression::infer_type_from_aggregation_expression,
    error::{Error, Result},
    pipeline_type_context::PipelineTypeContext,
    type_constraint::{ObjectTypeConstraint, TypeConstraint},
};

enum Mode {
    Exclusion,
    Inclusion,
}

// $project has two distinct behaviors:
//
// Exclusion mode: if every value in the projection document is `false` or `0` then the output
// preserves fields from the input except for fields that are specifically excluded. The special
// value `$$REMOVE` **cannot** be used in this mode.
//
// Inclusion (replace) mode: if any value in the projection document specifies a field for
// inclusion, replaces the value of an input field with a new value, adds a new field with a new
// value, or removes a field with the special value `$$REMOVE` then output excludes input fields
// that are not specified. The output is composed solely of fields specified in the projection
// document, plus `_id` unless `_id` is specifically excluded. Values of `false` or `0` are not
// allowed in this mode except to suppress `_id`.
//
// TODO: This implementation does not fully account for uses of $$REMOVE. It does correctly select
// inclusion mode if $$REMOVE is used. A complete implementation would infer a nullable type for
// a projection that conditionally resolves to $$REMOVE.
pub fn infer_type_from_project_stage(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    projection: &Document,
) -> Result<TypeConstraint> {
    let mode = if projection.values().all(is_false_or_zero) {
        Mode::Exclusion
    } else {
        Mode::Inclusion
    };
    match mode {
        Mode::Exclusion => exclusion_projection_type(context, desired_object_type_name, projection),
        Mode::Inclusion => inclusion_projection_type(context, desired_object_type_name, projection),
    }
}

fn exclusion_projection_type(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    projection: &Document,
) -> Result<TypeConstraint> {
    // Projection keys can be dot-separated paths to nested fields. In this case a single
    // object-type output field might be specified by multiple project keys. We collect sets of
    // each top-level key (the first component of a dot-separated path), and then merge
    // constraints.
    let mut specifications: HashMap<FieldName, ProjectionTree<()>> = Default::default();

    for (field_name, _) in projection {
        let path = field_name.split(".").map(|s| s.into()).collect_vec();
        ProjectionTree::insert_specification(&mut specifications, &path, ())?;
    }

    let input_type = context.get_input_document_type()?;
    Ok(projection_tree_into_field_overrides(
        input_type,
        desired_object_type_name,
        specifications,
    ))
}

fn projection_tree_into_field_overrides(
    input_type: TypeConstraint,
    desired_object_type_name: &str,
    specifications: HashMap<FieldName, ProjectionTree<()>>,
) -> TypeConstraint {
    let overrides = specifications
        .into_iter()
        .map(|(name, spec)| {
            let field_override = match spec {
                ProjectionTree::Object(sub_specs) => {
                    let original_field_type = TypeConstraint::FieldOf {
                        target_type: Box::new(input_type.clone()),
                        path: nonempty![name.clone()],
                    };
                    Some(projection_tree_into_field_overrides(
                        original_field_type,
                        &format!("{desired_object_type_name}_{name}"),
                        sub_specs,
                    ))
                }
                ProjectionTree::Field(_) => None,
            };
            (name, field_override)
        })
        .collect();

    TypeConstraint::WithFieldOverrides {
        augmented_object_type_name: desired_object_type_name.into(),
        target_type: Box::new(input_type),
        fields: overrides,
    }
}

fn inclusion_projection_type(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    projection: &Document,
) -> Result<TypeConstraint> {
    let input_type = context.get_input_document_type()?;

    // Projection keys can be dot-separated paths to nested fields. In this case a single
    // object-type output field might be specified by multiple project keys. We collect sets of
    // each top-level key (the first component of a dot-separated path), and then merge
    // constraints.
    let mut specifications: HashMap<FieldName, ProjectionTree<TypeConstraint>> = Default::default();

    let added_fields = projection
        .iter()
        .filter(|(_, spec)| !is_false_or_zero(spec));

    for (field_name, spec) in added_fields {
        let path = field_name.split(".").map(|s| s.into()).collect_vec();
        let projected_type = if is_true_or_one(spec) {
            TypeConstraint::FieldOf {
                target_type: Box::new(input_type.clone()),
                path: NonEmpty::from_slice(&path).ok_or_else(|| {
                    Error::Other("key in $project stage is an empty string".to_string())
                })?,
            }
        } else {
            let desired_object_type_name = format!("{desired_object_type_name}_{field_name}");
            infer_type_from_aggregation_expression(
                context,
                &desired_object_type_name,
                None,
                spec.clone(),
            )?
        };
        ProjectionTree::insert_specification(&mut specifications, &path, projected_type)?;
    }

    let specifies_id = projection.keys().any(|k| k == "_id");
    if !specifies_id {
        ProjectionTree::insert_specification(
            &mut specifications,
            &["_id".into()],
            TypeConstraint::Scalar(BsonScalarType::ObjectId),
        )?;
    }

    let object_type_name =
        projection_tree_into_object_type(context, desired_object_type_name, specifications);

    Ok(TypeConstraint::Object(object_type_name))
}

fn projection_tree_into_object_type(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    specifications: HashMap<FieldName, ProjectionTree<TypeConstraint>>,
) -> ObjectTypeName {
    let fields = specifications
        .into_iter()
        .map(|(field_name, spec)| {
            let field_type = match spec {
                ProjectionTree::Field(field_type) => field_type,
                ProjectionTree::Object(sub_specs) => {
                    let desired_object_type_name =
                        format!("{desired_object_type_name}_{field_name}");
                    let nested_object_name = projection_tree_into_object_type(
                        context,
                        &desired_object_type_name,
                        sub_specs,
                    );
                    TypeConstraint::Object(nested_object_name)
                }
            };
            (field_name, field_type)
        })
        .collect();
    let object_type = ObjectTypeConstraint { fields };
    let object_type_name = context.unique_type_name(desired_object_type_name);
    context.insert_object_type(object_type_name.clone(), object_type);
    object_type_name
}

enum ProjectionTree<T> {
    Object(HashMap<FieldName, ProjectionTree<T>>),
    Field(T),
}

impl<T> ProjectionTree<T> {
    fn insert_specification(
        specifications: &mut HashMap<FieldName, ProjectionTree<T>>,
        path: &[FieldName],
        field_type: T,
    ) -> Result<()> {
        match path {
            [] => Err(Error::Other(
                "invalid $project: a projection key is an empty string".into(),
            ))?,
            [field_name] => {
                let maybe_old_value =
                    specifications.insert(field_name.clone(), ProjectionTree::Field(field_type));
                if maybe_old_value.is_some() {
                    Err(path_collision_error(path))?;
                };
            }
            [first_field_name, rest @ ..] => {
                let entry = specifications.entry(first_field_name.clone());
                match entry {
                    Entry::Occupied(mut e) => match e.get_mut() {
                        ProjectionTree::Object(sub_specs) => {
                            Self::insert_specification(sub_specs, rest, field_type)?;
                        }
                        ProjectionTree::Field(_) => Err(path_collision_error(path))?,
                    },
                    Entry::Vacant(entry) => {
                        let mut sub_specs = Default::default();
                        Self::insert_specification(&mut sub_specs, rest, field_type)?;
                        entry.insert(ProjectionTree::Object(sub_specs));
                    }
                };
            }
        }
        Ok(())
    }
}

// Experimentation confirms that a zero value of any numeric type is interpreted as suppression of
// a field.
fn is_false_or_zero(x: &Bson) -> bool {
    let decimal_zero = Decimal128::from_str("0").expect("parse 0 as decimal");
    matches!(
        x,
        Bson::Boolean(false) | Bson::Int32(0) | Bson::Int64(0) | Bson::Double(0.0)
    ) || x == &Bson::Decimal128(decimal_zero)
}

fn is_true_or_one(x: &Bson) -> bool {
    let decimal_one = Decimal128::from_str("1").expect("parse 1 as decimal");
    matches!(
        x,
        Bson::Boolean(true) | Bson::Int32(1) | Bson::Int64(1) | Bson::Double(1.0)
    ) || x == &Bson::Decimal128(decimal_one)
}

fn path_collision_error(path: impl IntoIterator<Item = impl std::fmt::Display>) -> Error {
    Error::Other(format!(
        "invalid $project: path collision at {}",
        path.into_iter().join(".")
    ))
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;
    use mongodb_support::BsonScalarType;
    use nonempty::nonempty;
    use pretty_assertions::assert_eq;
    use test_helpers::configuration::mflix_config;

    use crate::native_query::{
        pipeline_type_context::PipelineTypeContext,
        type_constraint::{ObjectTypeConstraint, TypeConstraint},
    };

    #[test]
    fn infers_type_of_projection_in_inclusion_mode() -> anyhow::Result<()> {
        let config = mflix_config();
        let mut context = PipelineTypeContext::new(&config, None);
        let input_type = context.set_stage_doc_type(TypeConstraint::Object("movies".into()));

        let input = doc! {
            "title": 1,
            "tomatoes.critic.rating": true,
            "tomatoes.critic.meter": true,
            "tomatoes.lastUpdated": true,
            "releaseDate": "$released",
        };

        let inferred_type =
            super::infer_type_from_project_stage(&mut context, "Movie_project", &input)?;

        assert_eq!(
            inferred_type,
            TypeConstraint::Object("Movie_project".into())
        );

        let object_types = context.object_types();
        let expected_object_types = [
            (
                "Movie_project".into(),
                ObjectTypeConstraint {
                    fields: [
                        (
                            "_id".into(),
                            TypeConstraint::Scalar(BsonScalarType::ObjectId),
                        ),
                        (
                            "title".into(),
                            TypeConstraint::FieldOf {
                                target_type: Box::new(input_type.clone()),
                                path: nonempty!["title".into()],
                            },
                        ),
                        (
                            "tomatoes".into(),
                            TypeConstraint::Object("Movie_project_tomatoes".into()),
                        ),
                        (
                            "releaseDate".into(),
                            TypeConstraint::FieldOf {
                                target_type: Box::new(input_type.clone()),
                                path: nonempty!["released".into()],
                            },
                        ),
                    ]
                    .into(),
                },
            ),
            (
                "Movie_project_tomatoes".into(),
                ObjectTypeConstraint {
                    fields: [
                        (
                            "critic".into(),
                            TypeConstraint::Object("Movie_project_tomatoes_critic".into()),
                        ),
                        (
                            "lastUpdated".into(),
                            TypeConstraint::FieldOf {
                                target_type: Box::new(input_type.clone()),
                                path: nonempty!["tomatoes".into(), "lastUpdated".into()],
                            },
                        ),
                    ]
                    .into(),
                },
            ),
            (
                "Movie_project_tomatoes_critic".into(),
                ObjectTypeConstraint {
                    fields: [
                        (
                            "rating".into(),
                            TypeConstraint::FieldOf {
                                target_type: Box::new(input_type.clone()),
                                path: nonempty![
                                    "tomatoes".into(),
                                    "critic".into(),
                                    "rating".into()
                                ],
                            },
                        ),
                        (
                            "meter".into(),
                            TypeConstraint::FieldOf {
                                target_type: Box::new(input_type.clone()),
                                path: nonempty!["tomatoes".into(), "critic".into(), "meter".into()],
                            },
                        ),
                    ]
                    .into(),
                },
            ),
        ]
        .into();

        assert_eq!(object_types, &expected_object_types);

        Ok(())
    }

    #[test]
    fn infers_type_of_projection_in_exclusion_mode() -> anyhow::Result<()> {
        let config = mflix_config();
        let mut context = PipelineTypeContext::new(&config, None);
        let input_type = context.set_stage_doc_type(TypeConstraint::Object("movies".into()));

        let input = doc! {
            "title": 0,
            "tomatoes.critic.rating": false,
            "tomatoes.critic.meter": false,
            "tomatoes.lastUpdated": false,
        };

        let inferred_type =
            super::infer_type_from_project_stage(&mut context, "Movie_project", &input)?;

        assert_eq!(
            inferred_type,
            TypeConstraint::WithFieldOverrides {
                augmented_object_type_name: "Movie_project".into(),
                target_type: Box::new(input_type.clone()),
                fields: [
                    ("title".into(), None),
                    (
                        "tomatoes".into(),
                        Some(TypeConstraint::WithFieldOverrides {
                            augmented_object_type_name: "Movie_project_tomatoes".into(),
                            target_type: Box::new(TypeConstraint::FieldOf {
                                target_type: Box::new(input_type.clone()),
                                path: nonempty!["tomatoes".into()],
                            }),
                            fields: [
                                ("lastUpdated".into(), None),
                                (
                                    "critic".into(),
                                    Some(TypeConstraint::WithFieldOverrides {
                                        augmented_object_type_name: "Movie_project_tomatoes_critic"
                                            .into(),
                                        target_type: Box::new(TypeConstraint::FieldOf {
                                            target_type: Box::new(TypeConstraint::FieldOf {
                                                target_type: Box::new(input_type.clone()),
                                                path: nonempty!["tomatoes".into()],
                                            }),
                                            path: nonempty!["critic".into()],
                                        }),
                                        fields: [("rating".into(), None), ("meter".into(), None),]
                                            .into(),
                                    })
                                )
                            ]
                            .into(),
                        })
                    ),
                ]
                .into(),
            }
        );

        Ok(())
    }
}
