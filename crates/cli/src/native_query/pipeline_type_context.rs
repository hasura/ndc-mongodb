#![allow(dead_code)]

use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, HashSet},
};

use configuration::{schema::ObjectType, Configuration};
use ndc_models::ObjectTypeName;

use super::{
    error::{Error, Result},
    type_constraint::{ObjectTypeConstraint, TypeConstraint, TypeVariable},
};

/// Information exported from [PipelineTypeContext] after type inference is complete.
#[derive(Clone, Debug)]
pub struct PipelineTypes {
    pub result_document_type: ObjectTypeName,
    pub object_types: BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    pub warnings: Vec<Error>,
}

impl<'a> TryFrom<PipelineTypeContext<'a>> for PipelineTypes {
    type Error = Error;

    fn try_from(context: PipelineTypeContext<'a>) -> Result<Self> {
        Ok(Self {
            result_document_type: context.get_input_document_type()?,
            object_types: context.object_types.clone(),
            warnings: context.warnings,
        })
    }
}

#[derive(Clone, Debug)]
pub struct PipelineTypeContext<'a> {
    configuration: &'a Configuration,

    /// Document type for inputs to the pipeline stage being evaluated. At the start of the
    /// pipeline this is the document type for the input collection, if there is one.
    input_doc_type: Option<TypeConstraint>,

    /// Object types defined in the process of type inference. [self.input_doc_type] may refer to
    /// to a type here, or in [self.configuration.object_types]
    object_types: BTreeMap<ObjectTypeName, ObjectTypeConstraint>,

    type_variables: HashMap<TypeVariable, HashSet<TypeConstraint>>,
    next_type_variable: u32,

    warnings: Vec<Error>,
}

impl PipelineTypeContext<'_> {
    pub fn new(
        configuration: &Configuration,
        input_collection_document_type: Option<ObjectTypeName>,
    ) -> PipelineTypeContext<'_> {
        let mut context = PipelineTypeContext {
            configuration,
            input_doc_type: None,
            object_types: Default::default(),
            type_variables: Default::default(),
            next_type_variable: 0,
            warnings: Default::default(),
        };

        if let Some(type_name) = input_collection_document_type {
            context.set_stage_doc_type(type_name)
        }

        context
    }

    pub fn new_type_variable(
        &mut self,
        constraints: impl IntoIterator<Item = TypeConstraint>,
    ) -> TypeVariable {
        let variable = TypeVariable::new(self.next_type_variable);
        self.next_type_variable += 1;
        self.type_variables
            .insert(variable, constraints.into_iter().collect());
        variable
    }

    pub fn set_type_variable_constraint(
        &mut self,
        variable: TypeVariable,
        constraint: TypeConstraint,
    ) {
        let entry = self
            .type_variables
            .get_mut(&variable)
            .expect("unknown type variable");
        entry.insert(constraint);
    }

    pub fn insert_object_type(&mut self, name: ObjectTypeName, object_type: ObjectTypeConstraint) {
        self.object_types.insert(name, object_type);
    }

    pub fn unique_type_name(&self, desired_type_name: &str) -> ObjectTypeName {
        let mut counter = 0;
        let mut type_name: ObjectTypeName = desired_type_name.into();
        while self.configuration.object_types.contains_key(&type_name)
            || self.object_types.contains_key(&type_name)
        {
            counter += 1;
            type_name = format!("{desired_type_name}_{counter}").into();
        }
        type_name
    }

    pub fn set_stage_doc_type(&mut self, doc_type: TypeConstraint) {
        self.input_doc_type = Some(doc_type);
    }

    pub fn add_warning(&mut self, warning: Error) {
        self.warnings.push(warning);
    }

    // pub fn set_unknown_stage_doc_type(&mut self, warning: Error) {
    //     let type_variable = self.new_type_variable([]);
    //     self.input_doc_type = Some(TypeConstraint::Variable(type_variable));
    //     self.warnings.push(warning);
    // }

    pub fn get_object_type(&self, name: &ObjectTypeName) -> Option<Cow<'_, ObjectTypeConstraint>> {
        if let Some(object_type) = self.configuration.object_types.get(name) {
            let schema_object_type = object_type.clone().into();
            return Some(Cow::Owned(schema_object_type));
        }
        if let Some(object_type) = self.object_types.get(name) {
            return Some(Cow::Borrowed(object_type));
        }
        None
    }

    pub fn get_input_document_type(&self) -> Result<&TypeConstraint> {
        self.input_doc_type.as_ref().ok_or(Error::IncompletePipeline)
    }

    // /// Get the input document type for the next stage. Forces to a concrete type, and returns an
    // /// error if a concrete type cannot be inferred.
    // pub fn get_input_document_type_name(&self) -> Result<&ObjectTypeName> {
    //     match self
    //         .input_doc_type
    //         .and_then(|var| self.type_variables.get(&var))
    //     {
    //         None => Err(Error::IncompletePipeline),
    //         Some(constraints) => {
    //             let len = constraints.len();
    //             let first_constraint = constraints.iter().next();
    //             if let (1, Some(TypeConstraint::Object(t))) = (len, first_constraint) {
    //                 Ok(t)
    //             } else {
    //                 Err(Error::UnableToInferResultType)
    //             }
    //         }
    //     }
    // }

    // pub fn get_input_document_type(&self) -> Result<Cow<'_, ObjectType>> {
    //     let document_type_name = self.get_input_document_type_name()?;
    //     Ok(self
    //         .get_object_type(&document_type_name)
    //         .expect("if we have an input document type name we should have the object type"))
    // }
}
