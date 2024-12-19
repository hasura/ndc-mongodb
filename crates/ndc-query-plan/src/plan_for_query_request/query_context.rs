use std::collections::BTreeMap;

use ndc_models as ndc;

use crate::type_system::lookup_object_type;
use crate::{self as plan, inline_object_types};
use crate::{ConnectorTypes, Type};

use super::query_plan_error::QueryPlanError;

type Result<T> = std::result::Result<T, QueryPlanError>;

/// Necessary information to produce a [plan::QueryPlan] from an [ndc::QueryRequest]
pub trait QueryContext: ConnectorTypes {
    /* Required methods */

    /// Get the specific scalar type for this connector by name if the given name is a scalar type
    /// name. (This method will also be called for object type names in which case it should return
    /// `None`.)
    fn lookup_scalar_type(type_name: &ndc::ScalarTypeName) -> Option<Self::ScalarType>;

    fn lookup_aggregation_function(
        &self,
        input_type: &Type<Self::ScalarType>,
        function_name: &ndc::AggregateFunctionName,
    ) -> Result<(Self::AggregateFunction, &ndc::AggregateFunctionDefinition)>;

    fn lookup_comparison_operator(
        &self,
        left_operand_type: &Type<Self::ScalarType>,
        operator_name: &ndc::ComparisonOperatorName,
    ) -> Result<(Self::ComparisonOperator, &ndc::ComparisonOperatorDefinition)>;

    fn collections(&self) -> &BTreeMap<ndc::CollectionName, ndc::CollectionInfo>;
    fn functions(&self) -> &BTreeMap<ndc::FunctionName, (ndc::FunctionInfo, ndc::CollectionInfo)>;
    fn object_types(&self) -> &BTreeMap<ndc::ObjectTypeName, ndc::ObjectType>;
    fn procedures(&self) -> &BTreeMap<ndc::ProcedureName, ndc::ProcedureInfo>;

    /* Provided methods */

    fn find_aggregation_function_definition(
        &self,
        input_type: &Type<Self::ScalarType>,
        function_name: &ndc::AggregateFunctionName,
    ) -> Result<(
        Self::AggregateFunction,
        plan::AggregateFunctionDefinition<Self>,
    )>
    where
        Self: Sized,
    {
        let (func, definition) =
            Self::lookup_aggregation_function(self, input_type, function_name)?;
        Ok((
            func,
            plan::AggregateFunctionDefinition {
                result_type: self.aggregate_function_result_type(definition, input_type)?,
            },
        ))
    }

    fn aggregate_function_result_type(
        &self,
        definition: &ndc::AggregateFunctionDefinition,
        input_type: &plan::Type<Self::ScalarType>,
    ) -> Result<plan::Type<Self::ScalarType>> {
        let t = match definition {
            ndc::AggregateFunctionDefinition::Min => input_type.clone(),
            ndc::AggregateFunctionDefinition::Max => input_type.clone(),
            ndc::AggregateFunctionDefinition::Sum { result_type }
            | ndc::AggregateFunctionDefinition::Average { result_type } => {
                let scalar_type = Self::lookup_scalar_type(result_type)
                    .ok_or_else(|| QueryPlanError::UnknownScalarType(result_type.clone()))?;
                plan::Type::Scalar(scalar_type)
            }
            ndc::AggregateFunctionDefinition::Custom { result_type } => {
                self.ndc_to_plan_type(result_type)?
            }
        };
        Ok(t)
    }

    fn find_comparison_operator(
        &self,
        left_operand_type: &Type<Self::ScalarType>,
        op_name: &ndc::ComparisonOperatorName,
    ) -> Result<(
        Self::ComparisonOperator,
        plan::ComparisonOperatorDefinition<Self>,
    )>
    where
        Self: Sized,
    {
        let (operator, definition) =
            Self::lookup_comparison_operator(self, left_operand_type, op_name)?;
        let plan_def = match definition {
            ndc::ComparisonOperatorDefinition::Equal => plan::ComparisonOperatorDefinition::Equal,
            ndc::ComparisonOperatorDefinition::In => plan::ComparisonOperatorDefinition::In,
            ndc::ComparisonOperatorDefinition::Custom { argument_type } => {
                plan::ComparisonOperatorDefinition::Custom {
                    argument_type: self.ndc_to_plan_type(argument_type)?,
                }
            }
        };
        Ok((operator, plan_def))
    }

    fn find_collection(
        &self,
        collection_name: &ndc::CollectionName,
    ) -> Result<&ndc::CollectionInfo> {
        if let Some(collection) = self.collections().get(collection_name) {
            return Ok(collection);
        }
        if let Some((_, function)) = self.functions().get(collection_name) {
            return Ok(function);
        }

        Err(QueryPlanError::UnknownCollection(
            collection_name.to_string(),
        ))
    }

    fn find_collection_object_type(
        &self,
        collection_name: &ndc::CollectionName,
    ) -> Result<plan::ObjectType<Self::ScalarType>> {
        let collection = self.find_collection(collection_name)?;
        self.find_object_type(&collection.collection_type)
    }

    fn find_object_type<'a>(
        &'a self,
        object_type_name: &'a ndc::ObjectTypeName,
    ) -> Result<plan::ObjectType<Self::ScalarType>> {
        lookup_object_type(
            self.object_types(),
            object_type_name,
            Self::lookup_scalar_type,
        )
    }

    fn find_procedure(&self, procedure_name: &ndc::ProcedureName) -> Result<&ndc::ProcedureInfo> {
        self.procedures()
            .get(procedure_name)
            .ok_or_else(|| QueryPlanError::UnknownProcedure(procedure_name.to_string()))
    }

    fn find_scalar_type(scalar_type_name: &ndc::ScalarTypeName) -> Result<Self::ScalarType> {
        Self::lookup_scalar_type(scalar_type_name)
            .ok_or_else(|| QueryPlanError::UnknownScalarType(scalar_type_name.clone()))
    }

    fn ndc_to_plan_type(&self, ndc_type: &ndc::Type) -> Result<plan::Type<Self::ScalarType>> {
        inline_object_types(self.object_types(), ndc_type, Self::lookup_scalar_type)
    }
}
