use std::collections::BTreeMap;

use configuration::{
    native_mutation::NativeMutation, native_query::NativeQuery, Configuration, MongoScalarType,
};
use mongodb_support::{ExtendedJsonMode, EXTENDED_JSON_TYPE_NAME};
use ndc_models as ndc;
use ndc_query_plan::{ConnectorTypes, QueryContext, QueryPlanError};

use crate::aggregation_function::AggregationFunction;
use crate::comparison_function::ComparisonFunction;
use crate::scalar_types_capabilities::SCALAR_TYPES;

#[derive(Clone, Debug)]
pub struct MongoConfiguration(pub Configuration);

impl MongoConfiguration {
    pub fn extended_json_mode(&self) -> ExtendedJsonMode {
        self.0.options.serialization_options.extended_json_mode
    }

    pub fn native_queries(&self) -> &BTreeMap<ndc::FunctionName, NativeQuery> {
        &self.0.native_queries
    }

    pub fn native_mutations(&self) -> &BTreeMap<ndc::ProcedureName, NativeMutation> {
        &self.0.native_mutations
    }
}

impl ConnectorTypes for MongoConfiguration {
    type AggregateFunction = AggregationFunction;
    type ComparisonOperator = ComparisonFunction;
    type ScalarType = MongoScalarType;

    fn count_aggregate_type() -> ndc_query_plan::Type<Self::ScalarType> {
        ndc_query_plan::Type::Scalar(MongoScalarType::Bson(mongodb_support::BsonScalarType::Long))
    }

    fn string_type() -> ndc_query_plan::Type<Self::ScalarType> {
        ndc_query_plan::Type::Scalar(MongoScalarType::Bson(
            mongodb_support::BsonScalarType::String,
        ))
    }
}

impl QueryContext for MongoConfiguration {
    fn lookup_scalar_type(type_name: &ndc::ScalarTypeName) -> Option<Self::ScalarType> {
        type_name.try_into().ok()
    }

    fn lookup_aggregation_function(
        &self,
        input_type: &Type,
        function_name: &ndc::AggregateFunctionName,
    ) -> Result<(Self::AggregateFunction, &ndc::AggregateFunctionDefinition), QueryPlanError> {
        let function = AggregationFunction::from_graphql_name(function_name.as_str())?;
        let definition = scalar_type_name(input_type)
            .and_then(|name| SCALAR_TYPES.get(name))
            .and_then(|scalar_type_def| scalar_type_def.aggregate_functions.get(function_name))
            .ok_or_else(|| QueryPlanError::UnknownAggregateFunction {
                aggregate_function: function_name.to_owned(),
            })?;
        Ok((function, definition))
    }

    fn lookup_comparison_operator(
        &self,
        left_operand_type: &Type,
        operator_name: &ndc::ComparisonOperatorName,
    ) -> Result<(Self::ComparisonOperator, &ndc::ComparisonOperatorDefinition), QueryPlanError>
    where
        Self: Sized,
    {
        let operator = ComparisonFunction::from_graphql_name(operator_name.as_str())?;
        let definition = scalar_type_name(left_operand_type)
            .and_then(|name| SCALAR_TYPES.get(name))
            .and_then(|scalar_type_def| scalar_type_def.comparison_operators.get(operator_name))
            .ok_or_else(|| QueryPlanError::UnknownComparisonOperator(operator_name.to_owned()))?;
        Ok((operator, definition))
    }

    fn collections(&self) -> &BTreeMap<ndc::CollectionName, ndc::CollectionInfo> {
        &self.0.collections
    }

    fn functions(&self) -> &BTreeMap<ndc::FunctionName, (ndc::FunctionInfo, ndc::CollectionInfo)> {
        &self.0.functions
    }

    fn object_types(&self) -> &BTreeMap<ndc::ObjectTypeName, ndc::ObjectType> {
        &self.0.object_types
    }

    fn procedures(&self) -> &BTreeMap<ndc::ProcedureName, ndc::ProcedureInfo> {
        &self.0.procedures
    }
}

fn scalar_type_name(t: &Type) -> Option<&'static str> {
    match t {
        Type::Scalar(MongoScalarType::Bson(s)) => Some(s.graphql_name()),
        Type::Scalar(MongoScalarType::ExtendedJSON) => Some(EXTENDED_JSON_TYPE_NAME),
        Type::ArrayOf(t) if matches!(**t, Type::Scalar(_) | Type::Nullable(_)) => {
            scalar_type_name(t)
        }
        Type::Nullable(t) => scalar_type_name(t),
        _ => None,
    }
}

pub type Aggregate = ndc_query_plan::Aggregate<MongoConfiguration>;
pub type Argument = ndc_query_plan::Argument<MongoConfiguration>;
pub type Arguments = ndc_query_plan::Arguments<MongoConfiguration>;
pub type ComparisonTarget = ndc_query_plan::ComparisonTarget<MongoConfiguration>;
pub type ComparisonValue = ndc_query_plan::ComparisonValue<MongoConfiguration>;
pub type ExistsInCollection = ndc_query_plan::ExistsInCollection<MongoConfiguration>;
pub type Expression = ndc_query_plan::Expression<MongoConfiguration>;
pub type Field = ndc_query_plan::Field<MongoConfiguration>;
pub type MutationOperation = ndc_query_plan::MutationOperation<MongoConfiguration>;
pub type MutationPlan = ndc_query_plan::MutationPlan<MongoConfiguration>;
pub type MutationProcedureArgument = ndc_query_plan::MutationProcedureArgument<MongoConfiguration>;
pub type NestedField = ndc_query_plan::NestedField<MongoConfiguration>;
pub type NestedArray = ndc_query_plan::NestedArray<MongoConfiguration>;
pub type NestedObject = ndc_query_plan::NestedObject<MongoConfiguration>;
pub type ObjectType = ndc_query_plan::ObjectType<MongoScalarType>;
pub type OrderBy = ndc_query_plan::OrderBy<MongoConfiguration>;
pub type OrderByTarget = ndc_query_plan::OrderByTarget<MongoConfiguration>;
pub type Query = ndc_query_plan::Query<MongoConfiguration>;
pub type QueryPlan = ndc_query_plan::QueryPlan<MongoConfiguration>;
pub type Relationship = ndc_query_plan::Relationship<MongoConfiguration>;
pub type Relationships = ndc_query_plan::Relationships<MongoConfiguration>;
pub type Type = ndc_query_plan::Type<MongoScalarType>;
pub type VariableTypes = ndc_query_plan::VariableTypes<MongoScalarType>;
