use std::collections::BTreeMap;
use std::fmt::Debug;

use derivative::Derivative;
use indexmap::IndexMap;
use ndc_models::{
    Argument, OrderDirection, RelationshipArgument, RelationshipType, UnaryComparisonOperator,
};

use crate::Type;

pub trait ConnectorTypes {
    type ScalarType: Clone + Debug + PartialEq;
    type AggregateFunction: Clone + Debug + PartialEq;
    type ComparisonOperator: Clone + Debug + PartialEq;
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub struct QueryPlan<T: ConnectorTypes> {
    pub collection: String,
    pub query: Query<T>,
    pub arguments: BTreeMap<String, Argument>,
    pub variables: Option<Vec<VariableSet>>,

    // TODO: type for unrelated collection
    pub unrelated_collections: BTreeMap<String, UnrelatedJoin<T>>,
}

impl<T: ConnectorTypes> QueryPlan<T> {
    pub fn has_variables(&self) -> bool {
        self.variables.is_some()
    }
}

pub type VariableSet = BTreeMap<String, serde_json::Value>;
pub type Relationships<T> = BTreeMap<String, Relationship<T>>;

#[derive(Derivative)]
#[derivative(
    Clone(bound = ""),
    Debug(bound = ""),
    Default(bound = ""),
    PartialEq(bound = "")
)]
pub struct Query<T: ConnectorTypes> {
    pub aggregates: Option<IndexMap<String, Aggregate<T>>>,
    pub fields: Option<IndexMap<String, Field<T>>>,
    pub limit: Option<u32>,
    pub aggregates_limit: Option<u32>,
    pub offset: Option<u32>,
    pub order_by: Option<OrderBy<T>>,
    pub predicate: Option<Expression<T>>,

    /// Relationships referenced by fields and expressions in this query or sub-query. Does not
    /// include relationships in sub-queries nested under this one.
    pub relationships: Relationships<T>,
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

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub struct Relationship<T: ConnectorTypes> {
    pub column_mapping: BTreeMap<String, String>,
    pub relationship_type: RelationshipType,
    pub target_collection: String,
    pub arguments: BTreeMap<String, RelationshipArgument>,
    pub query: Query<T>,
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub struct UnrelatedJoin<T: ConnectorTypes> {
    pub target_collection: String,
    pub arguments: BTreeMap<String, RelationshipArgument>,
    pub query: Query<T>,
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub enum Aggregate<T: ConnectorTypes> {
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
        function: T::AggregateFunction,
        result_type: Type<T::ScalarType>,
    },
    StarCount,
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub struct NestedObject<T: ConnectorTypes> {
    pub fields: IndexMap<String, Field<T>>,
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub struct NestedArray<T: ConnectorTypes> {
    pub fields: Box<NestedField<T>>,
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub enum NestedField<T: ConnectorTypes> {
    Object(NestedObject<T>),
    Array(NestedArray<T>),
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub enum Field<T: ConnectorTypes> {
    Column {
        column: String,

        /// When the type of the column is a (possibly-nullable) array or object,
        /// the caller can request a subset of the complete column data,
        /// by specifying fields to fetch here.
        /// If omitted, the column data will be fetched in full.
        fields: Option<NestedField<T>>,

        column_type: Type<T::ScalarType>,
    },
    Relationship {
        /// The name of the relationship to follow for the subquery - this is the key in the
        /// [Query] relationships map in this module, it is **not** the key in the
        /// [ndc::QueryRequest] collection_relationships map.
        relationship: String,
        aggregates: Option<IndexMap<String, Aggregate<T>>>,
        fields: Option<IndexMap<String, Field<T>>>,
    },
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
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
        operator: T::ComparisonOperator,
        value: ComparisonValue<T>,
    },
    Exists {
        in_collection: ExistsInCollection,
        predicate: Option<Box<Expression<T>>>,
    },
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub struct OrderBy<T: ConnectorTypes> {
    /// The elements to order by, in priority order
    pub elements: Vec<OrderByElement<T>>,
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub struct OrderByElement<T: ConnectorTypes> {
    pub order_direction: OrderDirection,
    pub target: OrderByTarget<T>,
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub enum OrderByTarget<T: ConnectorTypes> {
    Column {
        /// The name of the column
        name: String,

        /// Path to a nested field within an object column
        field_path: Option<Vec<String>>,

        /// Any relationships to traverse to reach this column. These are translated from
        /// [ndc_models::OrderByElement] values in the [ndc_models::QueryRequest] to names of relation
        /// fields for the [QueryPlan].
        path: Vec<String>,
    },
    SingleColumnAggregate {
        /// The column to apply the aggregation function to
        column: String,
        /// Single column aggregate function name.
        function: T::AggregateFunction,

        result_type: Type<T::ScalarType>,

        /// Any relationships to traverse to reach this aggregate. These are translated from
        /// [ndc_models::OrderByElement] values in the [ndc_models::QueryRequest] to names of relation
        /// fields for the [QueryPlan].
        path: Vec<String>,
    },
    StarCountAggregate {
        /// Any relationships to traverse to reach this aggregate. These are translated from
        /// [ndc_models::OrderByElement] values in the [ndc_models::QueryRequest] to names of relation
        /// fields for the [QueryPlan].
        path: Vec<String>,
    },
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub enum ComparisonTarget<T: ConnectorTypes> {
    Column {
        /// The name of the column
        name: String,

        /// Path to a nested field within an object column
        field_path: Option<Vec<String>>,

        column_type: Type<T::ScalarType>,

        /// Any relationships to traverse to reach this column. These are translated from
        /// [ndc_models::PathElement] values in the [ndc_models::QueryRequest] to names of relation
        /// fields for the [QueryPlan].
        path: Vec<String>,
    },
    RootCollectionColumn {
        /// The name of the column
        name: String,

        /// Path to a nested field within an object column
        field_path: Option<Vec<String>>,

        column_type: Type<T::ScalarType>,
    },
}

impl<T: ConnectorTypes> ComparisonTarget<T> {
    pub fn relationship_path(&self) -> &[String] {
        match self {
            ComparisonTarget::Column { path, .. } => path,
            ComparisonTarget::RootCollectionColumn { .. } => &[],
        }
    }
}

impl<T: ConnectorTypes> ComparisonTarget<T> {
    pub fn get_column_type(&self) -> &Type<T::ScalarType> {
        match self {
            ComparisonTarget::Column { column_type, .. } => column_type,
            ComparisonTarget::RootCollectionColumn { column_type, .. } => column_type,
        }
    }
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
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

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub struct AggregateFunctionDefinition<T: ConnectorTypes> {
    /// The scalar or object type of the result of this function
    pub result_type: Type<T::ScalarType>,
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub enum ComparisonOperatorDefinition<T: ConnectorTypes> {
    Equal,
    In,
    Custom {
        /// The type of the argument to this operator
        argument_type: Type<T::ScalarType>,
    },
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
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
