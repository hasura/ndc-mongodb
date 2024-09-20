use std::{borrow::Cow, collections::BTreeMap};

use configuration::{schema::ObjectType, Configuration};
use mongodb::bson::Document;
use mongodb_support::aggregate::{Pipeline, Stage};
use ndc_models::{CollectionName, ObjectTypeName};

use crate::introspection::{sampling::make_object_type, type_unification::unify_object_types};

use super::{
    error::{Error, Result},
    helpers::find_collection_object_type,
};

type NamedObjectType = configuration::WithName<ObjectTypeName, ObjectType>;
type ObjectTypes = BTreeMap<ObjectTypeName, ObjectType>;

#[derive(Clone, Debug)]
pub struct PipelineTypeContext<'a> {
    configuration: &'a Configuration,

    type_name_root: Cow<'a, str>, // TODO: should this be in here?

    /// Document type for inputs to the pipeline stage being evaluated. At the start of the
    /// pipeline this is the document type for the input collection, if there is one.
    input_doc_type: Option<Constraint<ObjectTypeName>>,

    /// Object types defined in the process of type inference. [self.input_doc_type] may refer to
    /// to a type here, or in [self.configuration.object_types]
    object_types: ObjectTypes,

    warnings: Vec<Error>,
}

impl PipelineTypeContext<'_> {
    pub fn new<'a>(
        configuration: &'a Configuration,
        input_collection_document_type: Option<ObjectTypeName>,
        type_name_root: &'a str,
    ) -> PipelineTypeContext<'a> {
        PipelineTypeContext {
            configuration,
            type_name_root: type_name_root.into(),
            input_doc_type: input_collection_document_type.map(Constraint::Type),
            object_types: Default::default(),
            warnings: Default::default(),
        }
    }

    pub fn result_doc_type(&self) -> Result<Constraint<ObjectTypeName>> {
        self.input_doc_type.clone().ok_or(Error::IncompletePipeline)
    }

    pub fn object_types(&self) -> ObjectTypes {
        // TODO: prune
        self.object_types.clone()
    }

    fn unique_type_name(&self) -> ObjectTypeName {
        self.type_name_root.as_ref().into() // TODO: make sure the name is unique
    }

    fn set_stage_doc_type(
        self,
        type_name: ObjectTypeName,
        object_types: ObjectTypes,
    ) -> Self {
        Self {
            configuration: self.configuration,
            type_name_root: self.type_name_root,
            input_doc_type: Some(Constraint::Type(type_name)),
            object_types, // TODO: merge or replace?
            warnings: self.warnings,
        }
    }

    fn unknown_stage_doc_type(self, warning: Error) -> Self {
        Self {
            configuration: self.configuration,
            type_name_root: self.type_name_root,
            input_doc_type: Some(Constraint::InsufficientContext),
            object_types: Default::default(),
            warnings: {
                let mut warnings = self.warnings;
                warnings.push(warning);
                warnings
            },
        }
    }

    pub fn warnings(&self) -> &[Error] {
        &self.warnings
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Constraint<T> {
    /// The variable appears in a context with a specific type, and this is it.
    Type(T),

    /// There may be a type constraint, but there is not sufficient information to determine what
    /// it is.
    InsufficientContext,
}

pub fn infer_result_type<'a>(
    configuration: &'a Configuration,
    type_name_root: &'a str,
    input_collection: Option<&CollectionName>,
    pipeline: &Pipeline,
) -> Result<PipelineTypeContext<'a>> {
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
    Ok(context)
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
        Stage::Lookup {
            from,
            local_field,
            foreign_field,
            r#let,
            pipeline,
            r#as,
        } => todo!(),
        Stage::Skip(_) => context,
        Stage::Group {
            key_expression,
            accumulators,
        } => todo!(),
        Stage::Facet(_) => todo!(),
        Stage::Count(_) => todo!(),
        Stage::ReplaceWith(_) => todo!(),
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
fn prune_object_types(
    type_name: &ObjectTypeName,
    accumulated_object_types: Vec<NamedObjectType>,
) -> Vec<NamedObjectType> {
    accumulated_object_types // TODO: more complete pruning
}

#[cfg(test)]
mod tests {
    use configuration::schema::{ObjectField, ObjectType, Type};
    use mongodb::bson::doc;
    use mongodb_support::{
        aggregate::{Pipeline, Stage},
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
        let pipeline_context =
            infer_result_type(&config, "documents".into(), None, &pipeline).unwrap();
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
        let actual = pipeline_context.object_types().clone();
        assert_eq!(actual, expected);
        Ok(())
    }
}
