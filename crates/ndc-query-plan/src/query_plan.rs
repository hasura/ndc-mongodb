use std::collections::BTreeMap;
use std::fmt::Debug;

use indexmap::IndexMap;
use ndc_models::{Argument, OrderBy, Relationship, RelationshipArgument, UnaryComparisonOperator};
use nonempty::NonEmpty;

use crate::Type;

pub trait ConnectorTypes: Clone {
    type ScalarType: Clone + Debug + PartialEq;
    type BinaryOperatorType: Clone + Debug + PartialEq;

    /// Get the specific scalar type for this connector by name if the given name is a scalar type
    /// name. (This method will also be called for object type names in which case it should return
    /// `None`.)
    fn lookup_scalar_type(type_name: &str) -> Option<Self::ScalarType>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueryPlan<T: ConnectorTypes> {
    pub collection: String,
    pub query: Query<T>,
    pub arguments: BTreeMap<String, Argument>,
    pub variables: Option<Vec<BTreeMap<String, serde_json::Value>>>,

    // TODO: type for unrelated collection
    pub unrelated_collections: BTreeMap<String, ()>,
}

pub type VariableSet = BTreeMap<String, serde_json::Value>;

#[derive(Clone, Debug, PartialEq)]
pub struct Query<T: ConnectorTypes> {
    pub aggregates: Option<IndexMap<String, Aggregate<T>>>,
    pub fields: Option<IndexMap<String, Field<T>>>,
    pub limit: Option<u32>,
    pub aggregates_limit: Option<u32>,
    pub offset: Option<u32>,
    pub order_by: Option<OrderBy>,
    pub predicate: Option<Expression<T>>,

    /// Relationships referenced by fields and expressions in this query or sub-query. Does not
    /// include relationships in sub-queries nested under this one.
    pub relations: BTreeMap<String, (Relationship, Query<T>)>,
}

impl<T: ConnectorTypes> Query<T> {
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
pub enum Aggregate<CollTypes> {
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
        result_type: Type<CollTypes>,
    },
    StarCount {},
}

#[derive(Clone, Debug, PartialEq)]
pub enum Field<T: ConnectorTypes> {
    Column {
        column: String,
        column_type: Type<T::ScalarType>,
    },
    NestedObject {
        column: String,
        query: Box<Query<T>>,
    },
    NestedArray {
        field: Box<Field<T>>,
        limit: Option<u32>,
        offset: Option<u32>,
        predicate: Option<OrderBy>,
    },
    Relationship {
        query: Box<Query<T>>,
        /// The name of the relationship to follow for the subquery
        relationship: String,
        /// Values to be provided to any collection arguments
        arguments: BTreeMap<String, RelationshipArgument>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expression<T: ConnectorTypes> {
    And {
        expressions: Vec<Expression<T>>,
    },
    Or {
        expressions: Vec<Expression<T>>,
    },
    Not {
        expression: Box<Expression<T>>,
    },
    UnaryComparisonOperator {
        column: ComparisonTarget<T>,
        operator: UnaryComparisonOperator,
    },
    BinaryComparisonOperator {
        column: ComparisonTarget<T>,
        operator: T::BinaryOperatorType,
        value: ComparisonValue<T>,
    },
    Exists {
        /// Specifies which collection reference to check for row count. Assumes that the given
        /// reference is an aggregation over the collection that provides the number of rows that
        /// match the predicate that was given in the Exists expression of the
        /// [ndc_models::QueryRequest] that this [QueryPlan] was based on.
        in_collection: ExistsInCollection,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ComparisonTarget<T: ConnectorTypes> {
    Column {
        /// The name of the column
        name: ColumnSelector,
        column_type: Type<T::ScalarType>,
        /// Any relationships to traverse to reach this column. These are translated from
        /// [ndc_models::PathElement] values in the [ndc_models::QueryRequest] to names of relation
        /// fields for the [QueryPlan].
        path: Vec<String>,
    },
    RootCollectionColumn {
        /// The name of the column
        name: ColumnSelector,
        column_type: Type<T::ScalarType>,
    },
}

/// When referencing a column value we may want to reference a field inside a nested object. In
/// that case the [ColumnSelector::Path] variant is used. If the entire column value is referenced
/// then [ColumnSelector::Column] is used.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ColumnSelector {
    Path(NonEmpty<String>),
    Column(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ComparisonValue<T: ConnectorTypes> {
    Column {
        column: ComparisonTarget<T>,
    },
    Scalar {
        value: serde_json::Value,
        value_type: Type<T::ScalarType>,
    },
    Variable {
        name: String,
        variable_type: Type<T::ScalarType>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExistsInCollection {
    Related {
        /// Key of the relation in the [Query] joins map. Relationships are scoped to the sub-query
        /// that defines the relation source.
        relationship: String,
    },
    Unrelated {
        /// Key of the relation in the [QueryPlan] joins map. Unrelated collections are not scoped
        /// to a sub-query, instead they are given in the root [QueryPlan].
        unrelated_collection: String,
    },
}
