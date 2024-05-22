use std::collections::BTreeMap;

use lazy_static::lazy_static;
use ndc_models as ndc;

use crate::{ConnectorTypes, QueryContext, QueryPlanError, Type};

#[derive(Clone, Debug, Default)]
pub struct TestContext {
    pub collections: BTreeMap<String, ndc::CollectionInfo>,
    pub functions: BTreeMap<String, (ndc::FunctionInfo, ndc::CollectionInfo)>,
    pub procedures: BTreeMap<String, ndc::ProcedureInfo>,
    pub object_types: BTreeMap<String, ndc::ObjectType>,
}

impl ConnectorTypes for TestContext {
    type AggregateFunction = AggregateFunction;
    type ComparisonOperator = ComparisonOperator;
    type ScalarType = ScalarType;
}

impl QueryContext for TestContext {
    fn lookup_scalar_type(type_name: &str) -> Option<Self::ScalarType> {
        match type_name {
            "Bool" => Some(ScalarType::Bool),
            "Double" => Some(ScalarType::Double),
            "Int" => Some(ScalarType::Int),
            "String" => Some(ScalarType::String),
            _ => None,
        }
    }

    fn lookup_aggregation_function(
        &self,
        _input_type: &Type<Self::ScalarType>,
        function_name: &str,
    ) -> Result<(Self::AggregateFunction, &ndc::AggregateFunctionDefinition), QueryPlanError> {
        let function = match function_name {
            "Average" => Ok(AggregateFunction::Average),
            _ => Err(QueryPlanError::UnknownAggregateFunction {
                aggregate_function: function_name.to_owned(),
            }),
        }?;

        let definition = match &function {
            AggregateFunction::Average => &AVERAGE_DEFINITION,
        };
        Ok((function, definition))
    }

    fn lookup_comparison_operator(
        &self,
        _left_operand_type: &Type<Self::ScalarType>,
        operator_name: &str,
    ) -> Result<(Self::ComparisonOperator, &ndc::ComparisonOperatorDefinition), QueryPlanError>
    where
        Self: Sized,
    {
        let operator = match operator_name {
            "_eq" => Ok(ComparisonOperator::Equal),
            _ => Err(QueryPlanError::UnknownComparisonOperator(
                operator_name.to_owned(),
            )),
        }?;
        let definition = match &operator {
            ComparisonOperator::Equal => &EQUAL_DEFINITION,
        };
        Ok((operator, definition))
    }

    fn collections(&self) -> &BTreeMap<String, ndc::CollectionInfo> {
        &self.collections
    }

    fn functions(&self) -> &BTreeMap<String, (ndc::FunctionInfo, ndc::CollectionInfo)> {
        &self.functions
    }

    fn object_types(&self) -> &BTreeMap<String, ndc::ObjectType> {
        &self.object_types
    }

    fn procedures(&self) -> &BTreeMap<String, ndc::ProcedureInfo> {
        &self.procedures
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AggregateFunction {
    Average,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ComparisonOperator {
    Equal,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScalarType {
    Bool,
    Double,
    Int,
    String,
}

lazy_static! {
    static ref AVERAGE_DEFINITION: ndc::AggregateFunctionDefinition =
        ndc::AggregateFunctionDefinition {
            result_type: ndc::Type::Named {
                name: "Double".to_owned(),
            },
        };
    static ref EQUAL_DEFINITION: ndc::ComparisonOperatorDefinition =
        ndc::ComparisonOperatorDefinition::Custom {
            argument_type: ndc::Type::Named {
                name: "Double".to_owned(),
            },
        };
}
