use std::borrow::Cow;

use configuration::{Configuration, MongoScalarType};
use ndc_query_plan::ConnectorTypes;

use crate::{comparison_function::ComparisonFunction, scalar_types_capabilities::SCALAR_TYPES};

pub use ndc_query_plan::{
    ColumnSelector, Nullable, OrderBy, OrderByTarget, NON_NULLABLE, NULLABLE,
};

pub type Aggregate = ndc_query_plan::Aggregate<Configuration>;
pub type ComparisonTarget = ndc_query_plan::ComparisonTarget<Configuration>;
pub type ComparisonValue = ndc_query_plan::ComparisonValue<Configuration>;
pub type ExistsInCollection = ndc_query_plan::ExistsInCollection;
pub type Expression = ndc_query_plan::Expression<Configuration>;
pub type Field = ndc_query_plan::Field<Configuration>;
pub type ObjectType = ndc_query_plan::ObjectType<MongoScalarType>;
pub type Query = ndc_query_plan::Query<Configuration>;
pub type QueryContext<'a> = ndc_query_plan::QueryContext<'a, Configuration>;
pub type QueryPlan = ndc_query_plan::QueryPlan<Configuration>;
pub type Relationship = ndc_query_plan::Relationship<Configuration>;
pub type Relationships = ndc_query_plan::Relationships<Configuration>;
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
