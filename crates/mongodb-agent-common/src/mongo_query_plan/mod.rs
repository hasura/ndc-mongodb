use std::borrow::Cow;

use configuration::{Configuration, MongoScalarType};
use ndc_query_plan::ConnectorTypes;

use crate::{comparison_function::ComparisonFunction, scalar_types_capabilities::SCALAR_TYPES};

pub use ndc_query_plan::{
    ColumnSelector, Nullable, OrderBy, OrderByTarget, NON_NULLABLE, NULLABLE,
};

#[derive(Clone, Debug)]
pub struct MongoConnectorTypes {}

impl ConnectorTypes for MongoConnectorTypes {
    type ScalarType = MongoScalarType;
    type BinaryOperatorType = ComparisonFunction;

    fn lookup_scalar_type(type_name: &str) -> Option<Self::ScalarType> {
        type_name.try_into().ok()
    }
}

pub type Aggregate = ndc_query_plan::Aggregate<MongoConnectorTypes>;
pub type ComparisonTarget = ndc_query_plan::ComparisonTarget<MongoConnectorTypes>;
pub type ComparisonValue = ndc_query_plan::ComparisonValue<MongoConnectorTypes>;
pub type ExistsInCollection = ndc_query_plan::ExistsInCollection;
pub type Expression = ndc_query_plan::Expression<MongoConnectorTypes>;
pub type Field = ndc_query_plan::Field<MongoConnectorTypes>;
pub type ObjectType = ndc_query_plan::ObjectType<MongoScalarType>;
pub type Query = ndc_query_plan::Query<MongoConnectorTypes>;
pub type QueryContext<'a> = ndc_query_plan::QueryContext<'a, MongoConnectorTypes>;
pub type QueryPlan = ndc_query_plan::QueryPlan<MongoConnectorTypes>;
pub type Relationship = ndc_query_plan::Relationship<MongoConnectorTypes>;
pub type Relationships = ndc_query_plan::Relationships<MongoConnectorTypes>;
pub type Type = ndc_query_plan::Type<MongoScalarType>;

/// Produce a query context from the connector configuration to direct query request processing
pub fn get_query_context(configuration: &Configuration) -> QueryContext<'_> {
    QueryContext {
        collections: Cow::Borrowed(&configuration.collections),
        functions: Cow::Borrowed(&configuration.functions),
        object_types: Cow::Borrowed(&configuration.object_types),
        scalar_types: Cow::Borrowed(&SCALAR_TYPES),
        phantom: std::marker::PhantomData,
    }
}
