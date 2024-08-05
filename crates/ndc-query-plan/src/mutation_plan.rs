use std::collections::BTreeMap;

use derivative::Derivative;
use ndc_models as ndc;

use crate::ConnectorTypes;
use crate::{self as plan, Type};

#[derive(Derivative)]
#[derivative(
    Clone(bound = ""),
    Debug(bound = ""),
    PartialEq(bound = "T::ScalarType: PartialEq")
)]
pub struct MutationPlan<T: ConnectorTypes> {
    /// The mutation operations to perform
    pub operations: Vec<MutationOperation<T>>,
}

#[derive(Derivative)]
#[derivative(
    Clone(bound = ""),
    Debug(bound = ""),
    PartialEq(bound = "T::ScalarType: PartialEq")
)]
pub enum MutationOperation<T: ConnectorTypes> {
    Procedure {
        /// The name of a procedure
        name: ndc::ProcedureName,
        /// Any named procedure arguments
        arguments: BTreeMap<ndc::ArgumentName, MutationProcedureArgument<T>>,
        /// The fields to return from the result, or null to return everything
        fields: Option<plan::NestedField<T>>,
        /// Relationships referenced by fields and expressions in this query or sub-query. Does not
        /// include relationships in sub-queries nested under this one.
        relationships: plan::Relationships<T>,
    },
}

#[derive(Derivative)]
#[derivative(
    Clone(bound = ""),
    Debug(bound = ""),
    PartialEq(bound = "T::ScalarType: PartialEq")
)]
pub enum MutationProcedureArgument<T: ConnectorTypes> {
    /// The argument is provided as a literal value
    Literal {
        value: serde_json::Value,
        argument_type: Type<T::ScalarType>,
    },
    /// The argument was a literal value that has been parsed as an [Expression]
    Predicate { expression: plan::Expression<T> },
}
