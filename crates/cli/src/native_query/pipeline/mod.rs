mod match_stage;

use std::{collections::BTreeMap, iter::once};

use configuration::Configuration;
use mongodb::bson::{Bson, Document};
use mongodb_support::{
    aggregate::{Accumulator, Pipeline, Stage},
    BsonScalarType,
};
use ndc_models::{CollectionName, FieldName, ObjectTypeName};

use super::{
    aggregation_expression::{
        self, infer_type_from_aggregation_expression, infer_type_from_reference_shorthand,
    },
    error::{Error, Result},
    helpers::find_collection_object_type,
    pipeline_type_context::{PipelineTypeContext, PipelineTypes},
    reference_shorthand::{parse_reference_shorthand, Reference},
    type_constraint::{ObjectTypeConstraint, TypeConstraint},
};

pub fn infer_pipeline_types(
    configuration: &Configuration,
    // If we have to define a new object type, use this name
    desired_object_type_name: &str,
    input_collection: Option<&CollectionName>,
    pipeline: &Pipeline,
) -> Result<PipelineTypes> {
    if pipeline.is_empty() {
        return Err(Error::EmptyPipeline);
    }

    let collection_doc_type = input_collection
        .map(|collection_name| find_collection_object_type(configuration, collection_name))
        .transpose()?;

    let mut context = PipelineTypeContext::new(configuration, collection_doc_type);

    let object_type_name = context.unique_type_name(desired_object_type_name);

    for (stage_index, stage) in pipeline.iter().enumerate() {
        if let Some(output_type) =
            infer_stage_output_type(&mut context, desired_object_type_name, stage_index, stage)?
        {
            context.set_stage_doc_type(output_type);
        };
    }

    // Try to set the desired type name for the overall pipeline output
    let last_stage_type = context.get_input_document_type()?;
    if let TypeConstraint::Object(stage_type_name) = last_stage_type {
        if let Some(object_type) = context.get_object_type(&stage_type_name) {
            context.insert_object_type(object_type_name.clone(), object_type.into_owned());
            context.set_stage_doc_type(TypeConstraint::Object(object_type_name))
        }
    }

    context.into_types()
}

fn infer_stage_output_type(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    stage_index: usize,
    stage: &Stage,
) -> Result<Option<TypeConstraint>> {
    let output_type = match stage {
        Stage::AddFields(_) => todo!("add fields stage"),
        Stage::Documents(docs) => {
            let doc_constraints = docs
                .iter()
                .map(|doc| {
                    infer_type_from_aggregation_expression(
                        context,
                        &format!("{desired_object_type_name}_documents"),
                        doc.into(),
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            let type_variable = context.new_type_variable(doc_constraints);
            Some(TypeConstraint::Variable(type_variable))
        }
        Stage::Match(match_doc) => {
            match_stage::check_match_doc_for_parameters(
                context,
                &format!("{desired_object_type_name}_match"),
                match_doc.clone(),
            )?;
            None
        }
        Stage::Sort(_) => None,
        Stage::Limit(_) => None,
        Stage::Lookup { .. } => todo!("lookup stage"),
        Stage::Skip(_) => None,
        Stage::Group {
            key_expression,
            accumulators,
        } => {
            let object_type_name = infer_type_from_group_stage(
                context,
                &format!("{desired_object_type_name}_group"),
                key_expression,
                accumulators,
            )?;
            Some(TypeConstraint::Object(object_type_name))
        }
        Stage::Facet(_) => todo!("facet stage"),
        Stage::Count(_) => todo!("count stage"),
        Stage::ReplaceWith(selection) => {
            let selection: &Document = selection.into();
            Some(
                aggregation_expression::infer_type_from_aggregation_expression(
                    context,
                    &format!("{desired_object_type_name}_replaceWith"),
                    selection.clone().into(),
                )?,
            )
        }
        Stage::Unwind {
            path,
            include_array_index,
            preserve_null_and_empty_arrays,
        } => Some(infer_type_from_unwind_stage(
            context,
            &format!("{desired_object_type_name}_unwind"),
            path,
            include_array_index.as_deref(),
            *preserve_null_and_empty_arrays,
        )?),
        Stage::Other(doc) => {
            context.add_warning(Error::UnknownAggregationStage {
                stage_index,
                stage: doc.clone(),
            });
            // We don't know what the type is here so we represent it with an unconstrained type
            // variable.
            let type_variable = context.new_type_variable([]);
            Some(TypeConstraint::Variable(type_variable))
        }
    };
    Ok(output_type)
}

fn infer_type_from_group_stage(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    key_expression: &Bson,
    accumulators: &BTreeMap<String, Accumulator>,
) -> Result<ObjectTypeName> {
    let group_key_expression_type = infer_type_from_aggregation_expression(
        context,
        &format!("{desired_object_type_name}_id"),
        key_expression.clone(),
    )?;

    let group_expression_field: (FieldName, TypeConstraint) =
        ("_id".into(), group_key_expression_type.clone());

    let accumulator_fields = accumulators.iter().map(|(key, accumulator)| {
        let accumulator_type = match accumulator {
            Accumulator::Count => TypeConstraint::Scalar(BsonScalarType::Int),
            Accumulator::Min(expr) => infer_type_from_aggregation_expression(
                context,
                &format!("{desired_object_type_name}_min"),
                expr.clone(),
            )?,
            Accumulator::Max(expr) => infer_type_from_aggregation_expression(
                context,
                &format!("{desired_object_type_name}_min"),
                expr.clone(),
            )?,
            Accumulator::Push(expr) => {
                let t = infer_type_from_aggregation_expression(
                    context,
                    &format!("{desired_object_type_name}_push"),
                    expr.clone(),
                )?;
                TypeConstraint::ArrayOf(Box::new(t))
            }
            Accumulator::Avg(expr) => {
                let t = infer_type_from_aggregation_expression(
                    context,
                    &format!("{desired_object_type_name}_avg"),
                    expr.clone(),
                )?;
                match t {
                    TypeConstraint::ExtendedJSON => t,
                    TypeConstraint::Scalar(scalar_type) if scalar_type.is_numeric() => t,
                    _ => TypeConstraint::Nullable(Box::new(TypeConstraint::Scalar(
                        BsonScalarType::Int,
                    ))),
                }
            }
            Accumulator::Sum(expr) => {
                let t = infer_type_from_aggregation_expression(
                    context,
                    &format!("{desired_object_type_name}_push"),
                    expr.clone(),
                )?;
                match t {
                    TypeConstraint::ExtendedJSON => t,
                    TypeConstraint::Scalar(scalar_type) if scalar_type.is_numeric() => t,
                    _ => TypeConstraint::Scalar(BsonScalarType::Int),
                }
            }
        };
        Ok::<_, Error>((key.clone().into(), accumulator_type))
    });

    let fields = once(Ok(group_expression_field))
        .chain(accumulator_fields)
        .collect::<Result<_>>()?;
    let object_type = ObjectTypeConstraint { fields };
    let object_type_name = context.unique_type_name(desired_object_type_name);
    context.insert_object_type(object_type_name.clone(), object_type);
    Ok(object_type_name)
}

fn infer_type_from_unwind_stage(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    path: &str,
    include_array_index: Option<&str>,
    _preserve_null_and_empty_arrays: Option<bool>,
) -> Result<TypeConstraint> {
    let field_to_unwind = parse_reference_shorthand(path)?;
    let Reference::InputDocumentField { name, nested_path } = field_to_unwind else {
        return Err(Error::ExpectedStringPath(path.into()));
    };
    let field_type = infer_type_from_reference_shorthand(context, path)?;

    let mut unwind_stage_object_type = ObjectTypeConstraint {
        fields: Default::default(),
    };
    if let Some(index_field_name) = include_array_index {
        unwind_stage_object_type.fields.insert(
            index_field_name.into(),
            TypeConstraint::Scalar(BsonScalarType::Long),
        );
    }

    // If `path` includes a nested_path then the type for the unwound field will be nested
    // objects
    fn build_nested_types(
        context: &mut PipelineTypeContext<'_>,
        ultimate_field_type: TypeConstraint,
        parent_object_type: &mut ObjectTypeConstraint,
        desired_object_type_name: &str,
        field_name: FieldName,
        mut rest: impl Iterator<Item = FieldName>,
    ) {
        match rest.next() {
            Some(next_field_name) => {
                let object_type_name = context.unique_type_name(desired_object_type_name);
                let mut object_type = ObjectTypeConstraint {
                    fields: Default::default(),
                };
                build_nested_types(
                    context,
                    ultimate_field_type,
                    &mut object_type,
                    &format!("{desired_object_type_name}_{next_field_name}"),
                    next_field_name,
                    rest,
                );
                context.insert_object_type(object_type_name.clone(), object_type);
                parent_object_type
                    .fields
                    .insert(field_name, TypeConstraint::Object(object_type_name));
            }
            None => {
                parent_object_type
                    .fields
                    .insert(field_name, ultimate_field_type);
            }
        }
    }
    build_nested_types(
        context,
        TypeConstraint::ElementOf(Box::new(field_type)),
        &mut unwind_stage_object_type,
        desired_object_type_name,
        name,
        nested_path.into_iter(),
    );

    // let object_type_name = context.unique_type_name(desired_object_type_name);
    // context.insert_object_type(object_type_name.clone(), unwind_stage_object_type);

    // We just inferred an object type for the fields that are **added** by the unwind stage. To
    // get the full output type the added fields must be merged with fields from the output of the
    // previous stage.
    Ok(TypeConstraint::WithFieldOverrides {
        augmented_object_type_name: format!("{desired_object_type_name}_unwind").into(),
        target_type: Box::new(context.get_input_document_type()?.clone()),
        fields: unwind_stage_object_type.fields,
    })
}

#[cfg(test)]
mod tests {
    use configuration::schema::{ObjectField, ObjectType, Type};
    use mongodb::bson::doc;
    use mongodb_support::{
        aggregate::{Pipeline, Selection, Stage},
        BsonScalarType,
    };
    use nonempty::nonempty;
    use pretty_assertions::assert_eq;
    use test_helpers::configuration::mflix_config;

    use crate::native_query::{
        pipeline_type_context::PipelineTypeContext,
        type_constraint::{ObjectTypeConstraint, TypeConstraint, TypeVariable},
    };

    use super::{infer_pipeline_types, infer_type_from_unwind_stage};

    type Result<T> = anyhow::Result<T>;

    #[test]
    fn infers_type_from_documents_stage() -> Result<()> {
        let pipeline = Pipeline::new(vec![Stage::Documents(vec![
            doc! { "foo": 1 },
            doc! { "bar": 2 },
        ])]);
        let config = mflix_config();
        let pipeline_types = infer_pipeline_types(&config, "documents", None, &pipeline).unwrap();
        let expected = [(
            "documents_documents_2".into(),
            ObjectType {
                fields: [
                    (
                        "foo".into(),
                        ObjectField {
                            r#type: Type::Nullable(Box::new(Type::Scalar(BsonScalarType::Int))),
                            description: None,
                        },
                    ),
                    (
                        "bar".into(),
                        ObjectField {
                            r#type: Type::Nullable(Box::new(Type::Scalar(BsonScalarType::Int))),
                            description: None,
                        },
                    ),
                ]
                .into(),
                description: None,
            },
        )]
        .into();
        let actual = pipeline_types.object_types;
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn infers_type_from_replace_with_stage() -> Result<()> {
        let pipeline = Pipeline::new(vec![Stage::ReplaceWith(Selection::new(doc! {
            "selected_title": "$title"
        }))]);
        let config = mflix_config();
        let pipeline_types =
            infer_pipeline_types(&config, "movies", Some(&("movies".into())), &pipeline).unwrap();
        let expected = [(
            "movies_replaceWith".into(),
            ObjectType {
                fields: [(
                    "selected_title".into(),
                    ObjectField {
                        r#type: Type::Scalar(BsonScalarType::String),
                        description: None,
                    },
                )]
                .into(),
                description: None,
            },
        )]
        .into();
        let actual = pipeline_types.object_types;
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn infers_type_from_unwind_stage() -> Result<()> {
        let config = mflix_config();
        let mut context = PipelineTypeContext::new(&config, None);
        context.insert_object_type(
            "words_doc".into(),
            ObjectTypeConstraint {
                fields: [(
                    "words".into(),
                    TypeConstraint::ArrayOf(Box::new(TypeConstraint::Scalar(
                        BsonScalarType::String,
                    ))),
                )]
                .into(),
            },
        );
        context.set_stage_doc_type(TypeConstraint::Object("words_doc".into()));

        let inferred_type = infer_type_from_unwind_stage(
            &mut context,
            "unwind_stage",
            "$words",
            Some("idx"),
            Some(false),
        )?;

        assert_eq!(
            inferred_type,
            TypeConstraint::WithFieldOverrides {
                augmented_object_type_name: "unwind_stage_unwind".into(),
                target_type: Box::new(TypeConstraint::Variable(TypeVariable::new(0))),
                fields: [
                    ("idx".into(), TypeConstraint::Scalar(BsonScalarType::Long)),
                    (
                        "words".into(),
                        TypeConstraint::ElementOf(Box::new(TypeConstraint::FieldOf {
                            target_type: Box::new(TypeConstraint::Variable(TypeVariable::new(0))),
                            path: nonempty!["words".into()],
                        }))
                    )
                ]
                .into(),
            }
        );
        Ok(())
    }
}
