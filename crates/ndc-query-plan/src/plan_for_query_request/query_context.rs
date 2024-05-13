use std::marker::PhantomData;
use std::{borrow::Cow, collections::BTreeMap};

use ndc_models as ndc;

use crate as plan;
use crate::type_system::lookup_object_type;
use crate::ConnectorTypes;

use super::query_plan_error::QueryPlanError;

type Result<T> = std::result::Result<T, QueryPlanError>;

/// Necessary information to produce a [plan::QueryPlan] from an [ndc::QueryRequest]
#[derive(Clone, Debug)]
pub struct QueryContext<'a, T: ConnectorTypes> {
    pub collections: Cow<'a, BTreeMap<String, ndc::CollectionInfo>>,
    pub functions: Cow<'a, BTreeMap<String, (ndc::FunctionInfo, ndc::CollectionInfo)>>,
    pub object_types: Cow<'a, BTreeMap<String, ndc::ObjectType>>,
    pub scalar_types: Cow<'a, BTreeMap<String, ndc::ScalarType>>, // TODO: probably don't need this
    pub phantom: PhantomData<T>,
}

impl<T: ConnectorTypes> QueryContext<'_, T> {
    pub fn find_collection(&self, collection_name: &str) -> Result<&ndc::CollectionInfo> {
        if let Some(collection) = self.collections.get(collection_name) {
            return Ok(collection);
        }
        if let Some((_, function)) = self.functions.get(collection_name) {
            return Ok(function);
        }

        Err(QueryPlanError::UnknownCollection(
            collection_name.to_string(),
        ))
    }

    pub fn find_collection_object_type(
        &self,
        collection_name: &str,
    ) -> Result<plan::ObjectType<T::ScalarType>> {
        let collection = self.find_collection(collection_name)?;
        self.find_object_type(&collection.collection_type)
    }

    pub fn find_object_type<'a>(
        &'a self,
        object_type_name: &'a str,
    ) -> Result<plan::ObjectType<T::ScalarType>> {
        lookup_object_type::<T>(&self.object_types, object_type_name)
    }

    fn find_scalar_type(&self, scalar_type_name: &str) -> Result<&ndc::ScalarType> {
        self.scalar_types
            .get(scalar_type_name)
            .ok_or_else(|| QueryPlanError::UnknownScalarType(scalar_type_name.to_owned()))
    }

    fn find_aggregation_function_definition(
        &self,
        scalar_type_name: &str,
        function: &str,
    ) -> Result<&ndc::AggregateFunctionDefinition> {
        let scalar_type = self.find_scalar_type(scalar_type_name)?;
        scalar_type
            .aggregate_functions
            .get(function)
            .ok_or_else(|| QueryPlanError::UnknownAggregateFunction {
                scalar_type: scalar_type_name.to_string(),
                aggregate_function: function.to_string(),
            })
    }

    fn find_comparison_operator_definition(
        &self,
        scalar_type_name: &str,
        operator: &str,
    ) -> Result<&ndc::ComparisonOperatorDefinition> {
        let scalar_type = self.find_scalar_type(scalar_type_name)?;
        scalar_type
            .comparison_operators
            .get(operator)
            .ok_or_else(|| QueryPlanError::UnknownComparisonOperator(operator.to_owned()))
    }
}
