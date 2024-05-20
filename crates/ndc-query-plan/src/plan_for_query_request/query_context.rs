use std::collections::BTreeMap;

use ndc::ScalarType;
use ndc_models as ndc;

use crate::type_system::lookup_object_type;
use crate::{self as plan, inline_object_types};
use crate::{ConnectorTypes, Type};

use super::query_plan_error::QueryPlanError;

type Result<T> = std::result::Result<T, QueryPlanError>;

/// Necessary information to produce a [plan::QueryPlan] from an [ndc::QueryRequest]
pub trait QueryContext: ConnectorTypes {
    /* Required methods */

    // fn lookup_aggregate_function(
    //     input_type: &Type<Self::ScalarType>,
    //     function_name: &str,
    // ) -> Option<Self::AggregateFunction>;
    //
    // fn lookup_binary_operator(
    //     left_operand_type: &Type<Self::ScalarType>,
    //     op_name: &str,
    // ) -> Option<Self::BinaryOperator>;

    /// Get the specific scalar type for this connector by name if the given name is a scalar type
    /// name. (This method will also be called for object type names in which case it should return
    /// `None`.)
    fn lookup_scalar_type(type_name: &str) -> Option<Self::ScalarType>;

    fn scalar_type_definition(scalar_type: &ScalarType) -> &ndc::ScalarType;

    fn find_aggregation_function_definition(
        &self,
        input_type: &Type<Self::ScalarType>,
        function_name: &str,
    ) -> Result<&plan::AggregateFunctionDefinition<Self>>
    where
        Self: Sized;

    fn find_comparison_operator_definition(
        &self,
        left_operand_type: &Type<Self::ScalarType>,
        operator_name: &str,
    ) -> Result<&plan::ComparisonOperatorDefinition<Self>>
    where
        Self: Sized;

    fn ndc_collections(&self) -> &BTreeMap<String, ndc::CollectionInfo>;
    fn ndc_functions(&self) -> &BTreeMap<String, (ndc::FunctionInfo, ndc::CollectionInfo)>;
    fn ndc_object_types(&self) -> &BTreeMap<String, ndc::ObjectType>;

    /* Provided methods */

    // fn find_aggregate_function(
    //     &self,
    //     input_type: &Type<Self::ScalarType>,
    //     function_name: &str,
    // ) -> Result<&plan::AggregateFunctionDefinition<Self>>
    // where
    //     Self: Sized,
    // {
    //     let fun = Self::lookup_aggregate_function(input_type, function_name).ok_or_else(|| {
    //         QueryPlanError::UnknownAggregateFunction {
    //             aggregate_function: function_name.to_owned(),
    //         }
    //     })?;
    //     let definition = self.aggregate_function_definition(&fun);
    //     Ok((fun, definition))
    // }
    //
    // fn find_binary_operator(
    //     &self,
    //     left_operand_type: &Type<Self::ScalarType>,
    //     op_name: &str,
    // ) -> Result<(
    //     Self::BinaryOperator,
    //     &plan::ComparisonOperatorDefinition<Self>,
    // )>
    // where
    //     Self: Sized,
    // {
    //     let op = Self::lookup_binary_operator(left_operand_type, op_name)
    //         .ok_or_else(|| QueryPlanError::UnknownComparisonOperator(op_name.to_owned()))?;
    //     let definition = self.comparison_operator_definition(&op);
    //     Ok((op, definition))
    // }

    // #[derive(Clone, Debug)]
    // pub struct QueryContext<'a, T: ConnectorTypes> {
    //     pub collections: Cow<'a, BTreeMap<String, ndc::CollectionInfo>>,
    //     pub functions: Cow<'a, BTreeMap<String, (ndc::FunctionInfo, ndc::CollectionInfo)>>,
    //     pub object_types: Cow<'a, BTreeMap<String, ndc::ObjectType>>,
    //     pub scalar_types: Cow<'a, BTreeMap<String, ndc::ScalarType>>, // TODO: probably don't need this
    //     pub phantom: PhantomData<T>,
    // }

    // impl<T: ConnectorTypes> QueryContext<'_, T> {
    fn find_collection(&self, collection_name: &str) -> Result<&ndc::CollectionInfo> {
        if let Some(collection) = self.ndc_collections().get(collection_name) {
            return Ok(collection);
        }
        if let Some((_, function)) = self.ndc_functions().get(collection_name) {
            return Ok(function);
        }

        Err(QueryPlanError::UnknownCollection(
            collection_name.to_string(),
        ))
    }

    fn find_collection_object_type(
        &self,
        collection_name: &str,
    ) -> Result<plan::ObjectType<Self::ScalarType>> {
        let collection = self.find_collection(collection_name)?;
        self.find_object_type(&collection.collection_type)
    }

    fn find_object_type<'a>(
        &'a self,
        object_type_name: &'a str,
    ) -> Result<plan::ObjectType<Self::ScalarType>> {
        lookup_object_type(
            self.ndc_object_types(),
            object_type_name,
            Self::lookup_scalar_type,
        )
    }

    fn find_scalar_type(scalar_type_name: &str) -> Result<Self::ScalarType> {
        Self::lookup_scalar_type(scalar_type_name)
            .ok_or_else(|| QueryPlanError::UnknownScalarType(scalar_type_name.to_owned()))
    }

    // fn find_aggregation_function_definition(
    //     &self,
    //     column_type: &plan::Type<Self::ScalarType>,
    //     function: &str,
    // ) -> Result<&ndc::AggregateFunctionDefinition> {
    //     let scalar_type = Self::find_scalar_type(scalar_type_name)?;
    //     scalar_type
    //         .aggregate_functions
    //         .get(function)
    //         .ok_or_else(|| QueryPlanError::UnknownAggregateFunction {
    //             scalar_type: scalar_type_name.to_string(),
    //             aggregate_function: function.to_string(),
    //         })
    // }

    // fn find_comparison_operator_definition(
    //     &self,
    //     scalar_type_name: &str,
    //     operator: &str,
    // ) -> Result<&ndc::ComparisonOperatorDefinition> {
    //     let scalar_type = self.find_scalar_type(scalar_type_name)?;
    //     scalar_type
    //         .comparison_operators
    //         .get(operator)
    //         .ok_or_else(|| QueryPlanError::UnknownComparisonOperator(operator.to_owned()))
    // }

    fn ndc_to_plan_type(&self, ndc_type: &ndc::Type) -> Result<plan::Type<Self::ScalarType>> {
        todo!("ndc_to_plan_type")
        // inline_object_types(object_types, t, lookup_scalar_type)
    }
}
