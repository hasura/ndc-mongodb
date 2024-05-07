use std::collections::BTreeMap;

use indexmap::IndexMap;
use ndc_models::{Argument, Expression, OrderBy, Relationship, RelationshipArgument};

use crate::Type;

#[derive(Clone, Debug, PartialEq)]
pub struct QueryPlan<ScalarType> {
    pub collection: String,
    pub query: Query<ScalarType>,
    pub arguments: BTreeMap<String, Argument>,
    pub variables: Option<Vec<BTreeMap<String, serde_json::Value>>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Query<ScalarType> {
    pub aggregates: Option<IndexMap<String, Aggregate<ScalarType>>>,
    pub fields: Option<IndexMap<String, Field<ScalarType>>>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub order_by: Option<OrderBy>,
    pub predicate: Option<Expression>,

    /// Relationships referenced by fields and expressions in this query or sub-query. Does not
    /// include relationships in sub-queries nested under this one.
    pub joins: BTreeMap<String, (Relationship, Query<ScalarType>)>,
}

impl<S> Query<S> {
    pub fn has_aggregates(&self) -> bool {
        if let Some(aggregates) = &self.aggregates {
            !aggregates.is_empty()
        } else {
            false
        }
    }

    pub fn has_fields(&self) -> bool {
        if let Some(fields) = &self.fields {
            !fields.is_empty()
        } else {
            false
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NestedObject<ScalarType> {
    pub fields: IndexMap<String, Field<ScalarType>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NestedArray<ScalarType> {
    pub fields: Box<NestedField<ScalarType>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NestedField<ScalarType> {
    Object(NestedObject<ScalarType>),
    Array(NestedArray<ScalarType>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Aggregate<ScalarType> {
    ColumnCount {
        /// The column to apply the count aggregate function to
        column: String,
        /// Whether or not only distinct items should be counted
        distinct: bool,
    },
    SingleColumn {
        /// The column to apply the aggregation function to
        column: String,
        /// Single column aggregate function name.
        function: String,
        result_type: Type<ScalarType>,
    },
    StarCount {},
}

#[derive(Clone, Debug, PartialEq)]
pub enum Field<ScalarType> {
    Column {
        column: String,

        /// When the type of the column is a (possibly-nullable) array or object,
        /// the caller can request a subset of the complete column data,
        /// by specifying fields to fetch here.
        /// If omitted, the column data will be fetched in full.
        fields: Option<NestedField<ScalarType>>,

        /// The type of data queried, as given by the query - not as data exists in the database.
        /// That means that if a query selects a subset of fields from a nested object then this
        /// type should be an object type that includes only the requested fields. If the query
        /// aliases nested object field values to different names than are used in the database
        /// then this type should use the aliased names for those fields.
        requested_type: Type<ScalarType>,
    },
    Relationship {
        query: Box<Query<ScalarType>>,
        /// The name of the relationship to follow for the subquery
        relationship: String,
        /// Values to be provided to any collection arguments
        arguments: BTreeMap<String, RelationshipArgument>,
    },
}
