#![allow(dead_code)]

use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, HashSet},
};

use configuration::{
    schema::{ObjectType, Type},
    Configuration,
};
use itertools::Itertools as _;
use ndc_models::{ArgumentName, ObjectTypeName};

use super::{
    error::{Error, Result},
    type_constraint::{ObjectTypeConstraint, TypeConstraint, TypeVariable},
    type_solver::unify,
};

/// Information exported from [PipelineTypeContext] after type inference is complete.
#[derive(Clone, Debug)]
pub struct PipelineTypes {
    pub result_document_type: ObjectTypeName,
    pub parameter_types: BTreeMap<ArgumentName, Type>,
    pub object_types: BTreeMap<ObjectTypeName, ObjectType>,
    pub warnings: Vec<Error>,
}

#[derive(Clone, Debug)]
pub struct PipelineTypeContext<'a> {
    configuration: &'a Configuration,

    /// Document type for inputs to the pipeline stage being evaluated. At the start of the
    /// pipeline this is the document type for the input collection, if there is one.
    input_doc_type: Option<TypeVariable>,

    parameter_types: BTreeMap<ArgumentName, TypeVariable>,

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
            parameter_types: Default::default(),
            object_types: Default::default(),
            type_variables: Default::default(),
            next_type_variable: 0,
            warnings: Default::default(),
        };

        if let Some(type_name) = input_collection_document_type {
            context.set_stage_doc_type(TypeConstraint::Object(type_name))
        }

        context
    }

    pub fn into_types(self) -> Result<PipelineTypes> {
        let result_document_type_variable = self.input_doc_type.ok_or(Error::IncompletePipeline)?;
        let required_type_variables = self
            .parameter_types
            .values()
            .copied()
            .chain([result_document_type_variable])
            .collect_vec();

        let mut object_type_constraints = self.object_types;
        let (variable_types, added_object_types) = unify(
            &self.configuration,
            &required_type_variables,
            &mut object_type_constraints,
            self.type_variables,
        )
        .map_err(|err| match err {
            Error::FailedToUnify { unsolved_variables } => Error::UnableToInferTypes {
                could_not_infer_return_type: unsolved_variables
                    .contains(&result_document_type_variable),
                problem_parameter_types: self
                    .parameter_types
                    .iter()
                    .filter_map(|(name, variable)| {
                        if unsolved_variables.contains(variable) {
                            Some(name.clone())
                        } else {
                            None
                        }
                    })
                    .collect(),
            },
            e => e,
        })?;

        let result_document_type = variable_types
            .get(&result_document_type_variable)
            .expect("missing result type variable is missing");
        let result_document_type_name = match result_document_type {
            Type::Object(type_name) => type_name.clone().into(),
            t => Err(Error::ExpectedObject {
                actual_type: t.clone(),
            })?,
        };

        let parameter_types = self
            .parameter_types
            .into_iter()
            .map(|(parameter_name, type_variable)| {
                let param_type = variable_types
                    .get(&type_variable)
                    .expect("parameter type variable is missing");
                (parameter_name, param_type.clone())
            })
            .collect();

        Ok(PipelineTypes {
            result_document_type: result_document_type_name,
            parameter_types,
            object_types: added_object_types,
            warnings: self.warnings,
        })
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
        let variable = self.new_type_variable([doc_type]);
        self.input_doc_type = Some(variable);
    }

    pub fn add_warning(&mut self, warning: Error) {
        self.warnings.push(warning);
    }

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

    pub fn get_input_document_type(&self) -> Result<TypeConstraint> {
        let variable = self
            .input_doc_type
            .as_ref()
            .ok_or(Error::IncompletePipeline)?;
        Ok(TypeConstraint::Variable(*variable))
    }
}
