use std::{borrow::Cow, collections::BTreeMap};

use configuration::{schema::ObjectType, Configuration};
use ndc_models::ObjectTypeName;

use super::error::{Error, Result};

type ObjectTypes = BTreeMap<ObjectTypeName, ObjectType>;

/// Information exported from [PipelineTypeContext] after type inference is complete.
#[derive(Clone, Debug)]
pub struct PipelineTypes {
    pub result_document_type: ObjectTypeName,
    pub object_types: BTreeMap<ObjectTypeName, ObjectType>,
    pub warnings: Vec<Error>,
}

impl<'a> TryFrom<PipelineTypeContext<'a>> for PipelineTypes {
    type Error = Error;

    fn try_from(context: PipelineTypeContext<'a>) -> Result<Self> {
        let result_document_type = match context.input_doc_type {
            None => Err(Error::IncompletePipeline),
            Some(Constraint::Type(t)) => Ok(t),
            Some(Constraint::InsufficientContext) => Err(Error::UnableToInferResultType),
        }?;
        Ok(Self {
            result_document_type,
            object_types: context.object_types.clone(),
            warnings: context.warnings,
        })
    }
}

#[derive(Clone, Debug)]
pub struct PipelineTypeContext<'a> {
    configuration: &'a Configuration,

    pub type_name_root: Cow<'a, str>, // TODO: should this be in here?

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

    pub fn insert_object_type(&mut self, name: ObjectTypeName, object_type: ObjectType) {
        self.object_types.insert(name, object_type);
    }

    pub fn unique_type_name(&self) -> ObjectTypeName {
        self.type_name_root.as_ref().into() // TODO: make sure the name is unique
    }

    pub fn set_stage_doc_type(self, type_name: ObjectTypeName, object_types: ObjectTypes) -> Self {
        Self {
            configuration: self.configuration,
            type_name_root: self.type_name_root,
            input_doc_type: Some(Constraint::Type(type_name)),
            object_types, // TODO: merge or replace?
            warnings: self.warnings,
        }
    }

    pub fn unknown_stage_doc_type(self, warning: Error) -> Self {
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
