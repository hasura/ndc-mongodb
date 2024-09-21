use std::collections::BTreeMap;

use configuration::{schema::ObjectType, Configuration};
use mongodb::bson::Document;
use mongodb_support::aggregate::{Pipeline, Stage};
use ndc_models::{CollectionName, ObjectTypeName};

use crate::introspection::{sampling::make_object_type, type_unification::unify_object_types};

use super::{
    error::{Error, Result},
    helpers::find_collection_object_type,
    pipeline_type_context::{PipelineTypeContext, PipelineTypes},
};

type ObjectTypes = BTreeMap<ObjectTypeName, ObjectType>;

pub fn infer_result_type(
    configuration: &Configuration,
    type_name_root: &str,
    input_collection: Option<&CollectionName>,
    pipeline: &Pipeline,
) -> Result<PipelineTypes> {
    let collection_doc_type = input_collection
        .map(|collection_name| find_collection_object_type(configuration, collection_name))
        .transpose()?;
    let mut stages = pipeline.iter().enumerate();
    let context = match stages.next() {
        Some((stage_index, stage)) => infer_result_type_helper(
            PipelineTypeContext::new(configuration, collection_doc_type, type_name_root),
            stage_index,
            stage,
            stages,
        ),
        None => Err(Error::EmptyPipeline),
    }?;
    context.try_into()
}

pub fn infer_result_type_helper<'a, 'b>(
    context: PipelineTypeContext<'a>,
    stage_index: usize,
    stage: &Stage,
    mut rest: impl Iterator<Item = (usize, &'b Stage)>,
) -> Result<PipelineTypeContext<'a>> {
    let next_context = match stage {
        Stage::Documents(docs) => {
            let object_type_name = context.unique_type_name();
            let new_object_types = infer_type_from_documents(&object_type_name, docs);
            context.set_stage_doc_type(object_type_name, new_object_types)
        }
        Stage::Match(_) => context,
        Stage::Sort(_) => context,
        Stage::Limit(_) => context,
        Stage::Lookup { .. } => todo!("lookup stage"),
        Stage::Skip(_) => context,
        Stage::Group { .. } => todo!("group stage"),
        Stage::Facet(_) => todo!("facet stage"),
        Stage::Count(_) => todo!("count stage"),
        Stage::ReplaceWith(selection) => {
            let object_type_name = context.unique_type_name();
            let selection: &Document = selection.into();
            todo!()
        }
        Stage::Other(doc) => {
            let warning = Error::UnknownAggregationStage {
                stage_index,
                stage: doc.clone(),
            };
            context.unknown_stage_doc_type(warning)
        }
    };
    match rest.next() {
        Some((next_stage_index, next_stage)) => {
            infer_result_type_helper(next_context, next_stage_index, next_stage, rest)
        }
        None => Ok(next_context),
    }
}

fn infer_type_from_documents(
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

/// Filter object types from [accumulated_object_types] unless they are referenced directly or
/// indirectly by [type_name].
// fn prune_object_types(
//     type_name: &ObjectTypeName,
//     accumulated_object_types: Vec<NamedObjectType>,
// ) -> Vec<NamedObjectType> {
//     accumulated_object_types // TODO: more complete pruning
// }

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
            "documents".into(),
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
