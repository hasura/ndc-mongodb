use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, HashSet},
};

use configuration::{
    schema::{ObjectType, Type},
    Configuration,
};
use deriving_via::DerivingVia;
use ndc_models::ObjectTypeName;

use super::error::{Error, Result};

type ObjectTypes = BTreeMap<ObjectTypeName, ObjectType>;

#[derive(DerivingVia)]
#[deriving(Copy, Debug, Eq, Hash)]
pub struct TypeVariable(u32);

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
        Ok(Self {
            result_document_type: context.get_input_document_type_name()?.into(),
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
    input_doc_type: Option<HashSet<Constraint>>,

    /// Object types defined in the process of type inference. [self.input_doc_type] may refer to
    /// to a type here, or in [self.configuration.object_types]
    object_types: ObjectTypes,

    type_variables: HashMap<TypeVariable, HashSet<Constraint>>,
    next_type_variable: u32,

    warnings: Vec<Error>,
}

impl PipelineTypeContext<'_> {
    pub fn new<'a>(
        configuration: &'a Configuration,
        input_collection_document_type: Option<ObjectTypeName>,
    ) -> PipelineTypeContext<'a> {
        PipelineTypeContext {
            configuration,
            input_doc_type: input_collection_document_type.map(|type_name| {
                HashSet::from_iter([Constraint::ConcreteType(Type::Object(
                    type_name.to_string(),
                ))])
            }),
            object_types: Default::default(),
            type_variables: Default::default(),
            next_type_variable: 0,
            warnings: Default::default(),
        }
    }

    pub fn new_type_variable(
        &mut self,
        constraints: impl IntoIterator<Item = Constraint>,
    ) -> TypeVariable {
        let variable = TypeVariable(self.next_type_variable);
        self.next_type_variable += 1;
        self.type_variables
            .insert(variable, constraints.into_iter().collect());
        variable
    }

    pub fn set_type_variable_constraint(&mut self, variable: TypeVariable, constraint: Constraint) {
        let entry = self
            .type_variables
            .get_mut(&variable)
            .expect("unknown type variable");
        entry.insert(constraint);
    }

    pub fn insert_object_type(&mut self, name: ObjectTypeName, object_type: ObjectType) {
        self.object_types.insert(name, object_type);
    }

    pub fn unique_type_name(&self, desired_type_name: &str) -> ObjectTypeName {
        let mut counter = 0;
        let mut type_name: ObjectTypeName = format!("{desired_type_name}").into();
        while self.configuration.object_types.contains_key(&type_name)
            || self.object_types.contains_key(&type_name)
        {
            counter += 1;
            type_name = format!("{desired_type_name}_{counter}").into();
        }
        type_name.into()
    }

    pub fn set_stage_doc_type(&mut self, type_name: ObjectTypeName, mut object_types: ObjectTypes) {
        self.input_doc_type = Some(
            [Constraint::ConcreteType(Type::Object(
                type_name.to_string(),
            ))]
            .into(),
        );
        self.object_types.append(&mut object_types);
    }

    pub fn set_unknown_stage_doc_type(&mut self, warning: Error) {
        self.input_doc_type = Some([].into());
        self.warnings.push(warning);
    }

    pub fn get_object_type(&self, name: &ObjectTypeName) -> Option<Cow<'_, ObjectType>> {
        if let Some(object_type) = self.configuration.object_types.get(name) {
            let schema_object_type = object_type.clone().into();
            return Some(Cow::Owned(schema_object_type));
        }
        if let Some(object_type) = self.object_types.get(name) {
            return Some(Cow::Borrowed(object_type));
        }
        None
    }

    /// Get the input document type for the next stage. Forces to a concrete type, and returns an
    /// error if a concrete type cannot be inferred.
    pub fn get_input_document_type_name(&self) -> Result<&str> {
        match &self.input_doc_type {
            None => Err(Error::IncompletePipeline),
            Some(constraints) => {
                let len = constraints.len();
                let first_constraint = constraints.into_iter().next();
                if let (1, Some(Constraint::ConcreteType(Type::Object(t)))) =
                    (len, first_constraint)
                {
                    Ok(t)
                } else {
                    Err(Error::UnableToInferResultType)
                }
            }
        }
    }

    pub fn get_input_document_type(&self) -> Result<Cow<'_, ObjectType>> {
        let document_type_name = self.get_input_document_type_name()?.into();
        Ok(self
            .get_object_type(&document_type_name)
            .expect("if we have an input document type name we should have the object type"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Constraint {
    /// The variable appears in a context with a specific type, and this is it.
    ConcreteType(Type),

    /// The variable has the same type as another type variable.
    TypeRef(TypeVariable),
}
