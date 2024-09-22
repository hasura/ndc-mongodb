use std::{borrow::Cow, collections::BTreeMap};

use configuration::{
    schema::{ObjectField, ObjectType, Type},
    Configuration,
};
use mongodb::bson::Document;
use mongodb_support::{
    aggregate::{Pipeline, Stage},
    BsonScalarType,
};
use ndc_models::{CollectionName, FieldName, ObjectTypeName};

use crate::introspection::{sampling::make_object_type, type_unification::unify_object_types};

use super::{
    aggregation_expression::{self, infer_type_from_reference_shorthand},
    error::{Error, Result},
    helpers::find_collection_object_type,
    pipeline_type_context::{PipelineTypeContext, PipelineTypes},
    reference_shorthand::{parse_reference_shorthand, Reference},
};

type ObjectTypes = BTreeMap<ObjectTypeName, ObjectType>;

pub fn infer_result_type(
    configuration: &Configuration,
    // If we have to define a new object type, use this name
    desired_object_type_name: &str,
    input_collection: Option<&CollectionName>,
    pipeline: &Pipeline,
) -> Result<PipelineTypes> {
    let collection_doc_type = input_collection
        .map(|collection_name| find_collection_object_type(configuration, collection_name))
        .transpose()?;
    let mut stages = pipeline.iter().enumerate();
    let mut context = PipelineTypeContext::new(configuration, collection_doc_type);
    match stages.next() {
        Some((stage_index, stage)) => infer_result_type_helper(
            &mut context,
            desired_object_type_name,
            stage_index,
            stage,
            stages,
        ),
        None => Err(Error::EmptyPipeline),
    }?;
    context.try_into()
}

pub fn infer_result_type_helper<'a, 'b>(
    context: &mut PipelineTypeContext<'a>,
    desired_object_type_name: &str,
    stage_index: usize,
    stage: &Stage,
    mut rest: impl Iterator<Item = (usize, &'b Stage)>,
) -> Result<()> {
    match stage {
        Stage::Documents(docs) => {
            let document_type_name =
                context.unique_type_name(&format!("{desired_object_type_name}_documents"));
            let new_object_types = infer_type_from_documents(&document_type_name, docs);
            context.set_stage_doc_type(document_type_name, new_object_types);
        }
        Stage::Match(_) => (),
        Stage::Sort(_) => (),
        Stage::Limit(_) => (),
        Stage::Lookup { .. } => todo!("lookup stage"),
        Stage::Skip(_) => (),
        Stage::Group { .. } => todo!("group stage"),
        Stage::Facet(_) => todo!("facet stage"),
        Stage::Count(_) => todo!("count stage"),
        Stage::ReplaceWith(selection) => {
            let object_type_name = context.unique_type_name(desired_object_type_name);
            let selection: &Document = selection.into();
            aggregation_expression::infer_type_from_document(
                context,
                object_type_name.clone(),
                selection.clone(),
            )?;
            context.set_stage_doc_type(object_type_name, Default::default());
        }
        Stage::Unwind {
            path,
            include_array_index,
            preserve_null_and_empty_arrays,
        } => infer_type_from_unwind_stage(
            context,
            desired_object_type_name,
            path,
            include_array_index.as_deref(),
            *preserve_null_and_empty_arrays,
        )?,
        Stage::Other(doc) => {
            let warning = Error::UnknownAggregationStage {
                stage_index,
                stage: doc.clone(),
            };
            context.set_unknown_stage_doc_type(warning);
        }
    };
    match rest.next() {
        Some((next_stage_index, next_stage)) => infer_result_type_helper(
            context,
            desired_object_type_name,
            next_stage_index,
            next_stage,
            rest,
        ),
        None => Ok(()),
    }
}

pub fn infer_type_from_documents(
    object_type_name: &ObjectTypeName,
    documents: &[Document],
) -> ObjectTypes {
    let mut collected_object_types = vec![];
    for document in documents {
        let object_types = make_object_type(object_type_name, document, false, false);
        collected_object_types = if collected_object_types.is_empty() {
            object_types
        } else {
            unify_object_types(collected_object_types, object_types)
        };
    }
    collected_object_types
        .into_iter()
        .map(|type_with_name| (type_with_name.name, type_with_name.value))
        .collect()
}

fn infer_type_from_unwind_stage(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    path: &str,
    include_array_index: Option<&str>,
    _preserve_null_and_empty_arrays: Option<bool>,
) -> Result<()> {
    let field_to_unwind = parse_reference_shorthand(path)?;
    let Reference::InputDocumentField { name, nested_path } = field_to_unwind else {
        return Err(Error::ExpectedStringPath(path.into()));
    };

    let field_type = infer_type_from_reference_shorthand(context, path)?;
    let Type::ArrayOf(field_element_type) = field_type else {
        return Err(Error::ExpectedArrayReference {
            reference: path.into(),
            referenced_type: field_type,
        });
    };

    let nested_path_iter = nested_path.into_iter();

    let mut doc_type = context.get_input_document_type()?.into_owned();
    if let Some(index_field_name) = include_array_index {
        doc_type.fields.insert(
            index_field_name.into(),
            ObjectField {
                r#type: Type::Scalar(BsonScalarType::Long),
                description: Some(format!("index of unwound array elements in {name}")),
            },
        );
    }

    // If `path` includes a nested_path then the type for the unwound field will be nested
    // objects
    fn build_nested_types(
        context: &mut PipelineTypeContext<'_>,
        ultimate_field_type: Type,
        parent_object_type: &mut ObjectType,
        desired_object_type_name: Cow<'_, str>,
        field_name: FieldName,
        mut rest: impl Iterator<Item = FieldName>,
    ) {
        match rest.next() {
            Some(next_field_name) => {
                let object_type_name = context.unique_type_name(&desired_object_type_name);
                let mut object_type = ObjectType {
                    fields: Default::default(),
                    description: None,
                };
                build_nested_types(
                    context,
                    ultimate_field_type,
                    &mut object_type,
                    format!("{desired_object_type_name}_{next_field_name}").into(),
                    next_field_name,
                    rest,
                );
                context.insert_object_type(object_type_name.clone(), object_type);
                parent_object_type.fields.insert(
                    field_name,
                    ObjectField {
                        r#type: Type::Object(object_type_name.into()),
                        description: None,
                    },
                );
            }
            None => {
                parent_object_type.fields.insert(
                    field_name,
                    ObjectField {
                        r#type: ultimate_field_type,
                        description: None,
                    },
                );
            }
        }
    }
    build_nested_types(
        context,
        *field_element_type,
        &mut doc_type,
        desired_object_type_name.into(),
        name,
        nested_path_iter,
    );

    let object_type_name = context.unique_type_name(desired_object_type_name);
    context.insert_object_type(object_type_name, doc_type);

    Ok(())
}

#[cfg(test)]
mod tests {
    use configuration::schema::{ObjectField, ObjectType, Type};
    use mongodb::bson::doc;
    use mongodb_support::{
        aggregate::{Pipeline, Selection, Stage},
        BsonScalarType,
    };
    use pretty_assertions::assert_eq;
    use test_helpers::configuration::mflix_config;

    use super::infer_result_type;

    type Result<T> = anyhow::Result<T>;

    #[test]
    fn infers_type_from_documents_stage() -> Result<()> {
        let pipeline = Pipeline::new(vec![Stage::Documents(vec![
            doc! { "foo": 1 },
            doc! { "bar": 2 },
        ])]);
        let config = mflix_config();
        let pipeline_types = infer_result_type(&config, "documents", None, &pipeline).unwrap();
        let expected = [(
            "documents_documents".into(),
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
        let pipeline_types = infer_result_type(
            &config,
            "movies_selection",
            Some(&("movies".into())),
            &pipeline,
        )
        .unwrap();
        let expected = [(
            "movies_selection".into(),
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
}
