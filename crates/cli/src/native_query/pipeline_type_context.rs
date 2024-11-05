#![allow(dead_code)]

use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap},
};

use configuration::{
    schema::{ObjectType, Type},
    Configuration,
};
use itertools::Itertools as _;
use ndc_models::{ArgumentName, ObjectTypeName};

use super::{
    error::{Error, Result},
    helpers::unique_type_name,
    prune_object_types::prune_object_types,
    type_constraint::{ObjectTypeConstraint, TypeConstraint, TypeVariable, Variance},
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

    type_variables: HashMap<TypeVariable, BTreeSet<TypeConstraint>>,
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
            self.configuration,
            &required_type_variables,
            &mut object_type_constraints,
            self.type_variables.clone(),
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
                type_variables: self.type_variables,
                object_type_constraints,
            },
            e => e,
        })?;

        let mut result_document_type = variable_types
            .get(&result_document_type_variable)
            .expect("missing result type variable is missing")
            .clone();

        let mut parameter_types: BTreeMap<ArgumentName, Type> = self
            .parameter_types
            .into_iter()
            .map(|(parameter_name, type_variable)| {
                let param_type = variable_types
                    .get(&type_variable)
                    .expect("parameter type variable is missing");
                (parameter_name, param_type.clone())
            })
            .collect();

        // Prune added object types to remove types that are not referenced by the return type or
        // by parameter types, and therefore don't need to be included in the native query
        // configuration.
        let object_types = {
            let mut reference_types = std::iter::once(&mut result_document_type)
                .chain(parameter_types.values_mut())
                .collect_vec();
            prune_object_types(
                &mut reference_types,
                &self.configuration.object_types,
                added_object_types,
            )?
        };

        let result_document_type_name = match result_document_type {
            Type::Object(type_name) => type_name.clone().into(),
            t => Err(Error::ExpectedObject {
                actual_type: t.clone(),
            })?,
        };

        Ok(PipelineTypes {
            result_document_type: result_document_type_name,
            parameter_types,
            object_types,
            warnings: self.warnings,
        })
    }

    pub fn new_type_variable(
        &mut self,
        variance: Variance,
        constraints: impl IntoIterator<Item = TypeConstraint>,
    ) -> TypeVariable {
        let variable = TypeVariable::new(self.next_type_variable, variance);
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

    /// Add a parameter to be written to the native query configuration. Implicitly registers
    /// a corresponding type variable. If the parameter name has already been registered then
    /// returns a reference to the already-registered type variable.
    pub fn register_parameter(
        &mut self,
        name: ArgumentName,
        constraints: impl IntoIterator<Item = TypeConstraint>,
    ) -> TypeConstraint {
        let variable = if let Some(variable) = self.parameter_types.get(&name) {
            *variable
        } else {
            let variable = self.new_type_variable(Variance::Contravariant, []);
            self.parameter_types.insert(name, variable);
            variable
        };
        for constraint in constraints {
            self.set_type_variable_constraint(variable, constraint)
        }
        TypeConstraint::Variable(variable)
    }

    pub fn unique_type_name(&self, desired_type_name: &str) -> ObjectTypeName {
        unique_type_name(
            &self.configuration.object_types,
            &self.object_types,
            desired_type_name,
        )
    }

    pub fn set_stage_doc_type(&mut self, doc_type: TypeConstraint) {
        let variable = self.new_type_variable(Variance::Covariant, [doc_type]);
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
