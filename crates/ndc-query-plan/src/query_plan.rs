use std::{borrow::Cow, collections::BTreeMap, fmt::Debug, iter};

use derivative::Derivative;
use indexmap::IndexMap;
use itertools::Either;
use ndc_models::{
    self as ndc, ArgumentName, FieldName, OrderDirection, RelationshipType, UnaryComparisonOperator,
};

use crate::{vec_set::VecSet, Type};

pub trait ConnectorTypes {
    type ScalarType: Clone + Debug + PartialEq + Eq;
    type AggregateFunction: Clone + Debug + PartialEq;
    type ComparisonOperator: Clone + Debug + PartialEq;

    /// Result type for count aggregations
    fn count_aggregate_type() -> Type<Self::ScalarType>;

    fn string_type() -> Type<Self::ScalarType>;
}

#[derive(Derivative)]
#[derivative(
    Clone(bound = ""),
    Debug(bound = ""),
    PartialEq(bound = "T::ScalarType: PartialEq")
)]
pub struct QueryPlan<T: ConnectorTypes> {
    pub collection: ndc::CollectionName,
    pub query: Query<T>,
    pub arguments: BTreeMap<ndc::ArgumentName, Argument<T>>,
    pub variables: Option<Vec<VariableSet>>,

    /// Types for values from the `variables` map as inferred by usages in the query request. It is
    /// possible for the same variable to be used in multiple contexts with different types. This
    /// map provides sets of all observed types.
    ///
    /// The observed type may be `None` if the type of a variable use could not be inferred.
    pub variable_types: VariableTypes<T::ScalarType>,

    // TODO: type for unrelated collection
    pub unrelated_collections: BTreeMap<String, UnrelatedJoin<T>>,
}

impl<T: ConnectorTypes> QueryPlan<T> {
    pub fn has_variables(&self) -> bool {
        self.variables.is_some()
    }
}

pub type Arguments<T> = BTreeMap<ndc::ArgumentName, Argument<T>>;
pub type Relationships<T> = BTreeMap<ndc::RelationshipName, Relationship<T>>;
pub type VariableSet = BTreeMap<ndc::VariableName, serde_json::Value>;
pub type VariableTypes<T> = BTreeMap<ndc::VariableName, VecSet<Type<T>>>;

#[derive(Derivative)]
#[derivative(
    Clone(bound = ""),
    Debug(bound = ""),
    Default(bound = ""),
    PartialEq(bound = "")
)]
pub struct Query<T: ConnectorTypes> {
    pub aggregates: Option<IndexMap<ndc::FieldName, Aggregate<T>>>,
    pub fields: Option<IndexMap<ndc::FieldName, Field<T>>>,
    pub limit: Option<u32>,
    pub aggregates_limit: Option<u32>,
    pub offset: Option<u32>,
    pub order_by: Option<OrderBy<T>>,
    pub predicate: Option<Expression<T>>,

    /// Relationships referenced by fields and expressions in this query or sub-query. Does not
    /// include relationships in sub-queries nested under this one.
    pub relationships: Relationships<T>,

    /// Some relationship references may introduce a named "scope" so that other parts of the query
    /// request can reference fields of documents in the related collection. The connector must
    /// introduce a variable, or something similar, for such references.
    pub scope: Option<Scope>,
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
#[derivative(
    Clone(bound = ""),
    Debug(bound = ""),
    PartialEq(bound = "T::ScalarType: PartialEq")
)]
pub enum Argument<T: ConnectorTypes> {
    /// The argument is provided by reference to a variable
    Variable {
        name: ndc::VariableName,
        argument_type: Type<T::ScalarType>,
    },
    /// The argument is provided as a literal value
    Literal {
        value: serde_json::Value,
        argument_type: Type<T::ScalarType>,
    },
    /// The argument was a literal value that has been parsed as an [Expression]
    Predicate { expression: Expression<T> },
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub struct Relationship<T: ConnectorTypes> {
    pub column_mapping: BTreeMap<ndc::FieldName, ndc::FieldName>,
    pub relationship_type: RelationshipType,
    pub target_collection: ndc::CollectionName,
    pub arguments: BTreeMap<ndc::ArgumentName, RelationshipArgument<T>>,
    pub query: Query<T>,
}

#[derive(Derivative)]
#[derivative(
    Clone(bound = ""),
    Debug(bound = ""),
    PartialEq(bound = "T::ScalarType: PartialEq")
)]
pub enum RelationshipArgument<T: ConnectorTypes> {
    /// The argument is provided by reference to a variable
    Variable {
        name: ndc::VariableName,
        argument_type: Type<T::ScalarType>,
    },
    /// The argument is provided as a literal value
    Literal {
        value: serde_json::Value,
        argument_type: Type<T::ScalarType>,
    },
    // The argument is provided based on a column of the source collection
    Column {
        name: ndc::FieldName,
        argument_type: Type<T::ScalarType>,
    },
    /// The argument was a literal value that has been parsed as an [Expression]
    Predicate { expression: Expression<T> },
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub struct UnrelatedJoin<T: ConnectorTypes> {
    pub target_collection: ndc::CollectionName,
    pub arguments: BTreeMap<ndc::ArgumentName, RelationshipArgument<T>>,
    pub query: Query<T>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    Root,
    Named(String),
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub enum Aggregate<T: ConnectorTypes> {
    ColumnCount {
        /// The column to apply the count aggregate function to
        column: ndc::FieldName,
        /// Arguments to satisfy the column specified by 'column'
        arguments: BTreeMap<ArgumentName, Argument<T>>,
        /// Path to a nested field within an object column
        field_path: Option<Vec<FieldName>>,
        /// Whether or not only distinct items should be counted
        distinct: bool,
    },
    SingleColumn {
        /// The column to apply the aggregation function to
        column: ndc::FieldName,
        /// Arguments to satisfy the column specified by 'column'
        arguments: BTreeMap<ArgumentName, Argument<T>>,
        /// Path to a nested field within an object column
        field_path: Option<Vec<FieldName>>,
        /// Single column aggregate function name.
        function: T::AggregateFunction,
        result_type: Type<T::ScalarType>,
    },
    StarCount,
}

impl<T: ConnectorTypes> Aggregate<T> {
    pub fn result_type(&self) -> Cow<Type<T::ScalarType>> {
        match self {
            Aggregate::ColumnCount { .. } => Cow::Owned(T::count_aggregate_type()),
            Aggregate::SingleColumn { result_type, .. } => Cow::Borrowed(result_type),
            Aggregate::StarCount => Cow::Owned(T::count_aggregate_type().into()),
        }
    }
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub struct NestedObject<T: ConnectorTypes> {
    pub fields: IndexMap<ndc::FieldName, Field<T>>,
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
        column: ndc::FieldName,

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
        relationship: ndc::RelationshipName,
        aggregates: Option<IndexMap<ndc::FieldName, Aggregate<T>>>,
        fields: Option<IndexMap<ndc::FieldName, Field<T>>>,
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
    /// A comparison against a nested array column.
    /// Only used if the 'query.nested_fields.filter_by.nested_arrays' capability is supported.
    ArrayComparison {
        column: ComparisonTarget<T>,
        comparison: ArrayComparison<T>,
    },
    Exists {
        in_collection: ExistsInCollection<T>,
        predicate: Option<Box<Expression<T>>>,
    },
}

impl<T: ConnectorTypes> Expression<T> {
    /// Get an iterator of columns referenced by the expression, not including columns of related
    /// collections. This is used to build a plan for joining the referenced collection - we need
    /// to include fields in the join that the expression needs to access.
    //
    // TODO: ENG-1457 When we implement query.aggregates.filter_by we'll need to collect aggregates
    // references. That's why this function returns [ComparisonTarget] instead of [Field].
    pub fn query_local_comparison_targets<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = Cow<'a, ComparisonTarget<T>>> + 'a> {
        match self {
            Expression::And { expressions } => Box::new(
                expressions
                    .iter()
                    .flat_map(|e| e.query_local_comparison_targets()),
            ),
            Expression::Or { expressions } => Box::new(
                expressions
                    .iter()
                    .flat_map(|e| e.query_local_comparison_targets()),
            ),
            Expression::Not { expression } => expression.query_local_comparison_targets(),
            Expression::UnaryComparisonOperator { column, .. } => {
                Box::new(std::iter::once(Cow::Borrowed(column)))
            }
            Expression::BinaryComparisonOperator { column, value, .. } => Box::new(
                std::iter::once(Cow::Borrowed(column))
                    .chain(Self::local_targets_from_comparison_value(value).map(Cow::Owned)),
            ),
            Expression::ArrayComparison { column, comparison } => {
                let value_targets = match comparison {
                    ArrayComparison::Contains { value } => Either::Left(
                        Self::local_targets_from_comparison_value(value).map(Cow::Owned),
                    ),
                    ArrayComparison::IsEmpty => Either::Right(std::iter::empty()),
                };
                Box::new(std::iter::once(Cow::Borrowed(column)).chain(value_targets))
            }
            Expression::Exists { .. } => Box::new(iter::empty()),
        }
    }

    fn local_targets_from_comparison_value(
        value: &ComparisonValue<T>,
    ) -> impl Iterator<Item = ComparisonTarget<T>> {
        match value {
            ComparisonValue::Column {
                path,
                name,
                arguments,
                field_path,
                field_type,
                ..
            } => {
                if path.is_empty() {
                    Either::Left(iter::once(ComparisonTarget::Column {
                        name: name.clone(),
                        arguments: arguments.clone(),
                        field_path: field_path.clone(),
                        field_type: field_type.clone(),
                    }))
                } else {
                    Either::Right(iter::empty())
                }
            }
            _ => Either::Right(std::iter::empty()),
        }
    }
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub enum ArrayComparison<T: ConnectorTypes> {
    /// Check if the array contains the specified value.
    /// Only used if the 'query.nested_fields.filter_by.nested_arrays.contains' capability is supported.
    Contains { value: ComparisonValue<T> },
    /// Check is the array is empty.
    /// Only used if the 'query.nested_fields.filter_by.nested_arrays.is_empty' capability is supported.
    IsEmpty,
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
        /// Any relationships to traverse to reach this column. These are translated from
        /// [ndc::OrderByElement] values in the [ndc::QueryRequest] to names of relation
        /// fields for the [QueryPlan].
        path: Vec<ndc::RelationshipName>,

        /// The name of the column
        name: ndc::FieldName,

        /// Arguments to satisfy the column specified by 'name'
        arguments: BTreeMap<ArgumentName, Argument<T>>,

        /// Path to a nested field within an object column
        field_path: Option<Vec<ndc::FieldName>>,
    },
    Aggregate {
        /// Non-empty collection of relationships to traverse
        path: Vec<ndc::RelationshipName>,
        /// The aggregation method to use
        aggregate: Aggregate<T>,
    },
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub enum ComparisonTarget<T: ConnectorTypes> {
    /// The comparison targets a column.
    Column {
        /// The name of the column
        name: ndc::FieldName,

        /// Arguments to satisfy the column specified by 'name'
        arguments: BTreeMap<ArgumentName, Argument<T>>,

        /// Path to a nested field within an object column
        field_path: Option<Vec<ndc::FieldName>>,

        /// Type of the field that you get *after* follwing `field_path` to a possibly-nested
        /// field.
        field_type: Type<T::ScalarType>,
    },
    // TODO: ENG-1457 Add this variant to support query.aggregates.filter_by
    // /// The comparison targets the result of aggregation.
    // /// Only used if the 'query.aggregates.filter_by' capability is supported.
    // Aggregate {
    //     /// Non-empty collection of relationships to traverse
    //     path: Vec<RelationshipName>,
    //     /// The aggregation method to use
    //     aggregate: Aggregate<T>,
    // },
}

impl<T: ConnectorTypes> ComparisonTarget<T> {
    pub fn target_type(&self) -> &Type<T::ScalarType> {
        match self {
            ComparisonTarget::Column { field_type, .. } => field_type,
            // TODO: ENG-1457
            // ComparisonTarget::Aggregate { aggregate, .. } => aggregate.result_type,
        }
    }
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub enum ComparisonValue<T: ConnectorTypes> {
    Column {
        /// Any relationships to traverse to reach this column.
        /// Only non-empty if the 'relationships.relation_comparisons' is supported.
        path: Vec<ndc::RelationshipName>,
        /// The name of the column
        name: ndc::FieldName,
        /// Arguments to satisfy the column specified by 'name'
        arguments: BTreeMap<ArgumentName, Argument<T>>,
        /// Path to a nested field within an object column.
        /// Only non-empty if the 'query.nested_fields.filter_by' capability is supported.
        field_path: Option<Vec<ndc::FieldName>>,
        /// Type of the field that you get *after* follwing `field_path` to a possibly-nested
        /// field.
        field_type: Type<T::ScalarType>,
        /// The scope in which this column exists, identified
        /// by an top-down index into the stack of scopes.
        /// The stack grows inside each `Expression::Exists`,
        /// so scope 0 (the default) refers to the current collection,
        /// and each subsequent index refers to the collection outside
        /// its predecessor's immediately enclosing `Expression::Exists`
        /// expression.
        /// Only used if the 'query.exists.named_scopes' capability is supported.
        scope: Option<usize>,
    },
    Scalar {
        value: serde_json::Value,
        value_type: Type<T::ScalarType>,
    },
    Variable {
        name: ndc::VariableName,
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
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Contains,
    ContainsInsensitive,
    StartsWith,
    StartsWithInsensitive,
    EndsWith,
    EndsWithInsensitive,
    Custom {
        /// The type of the argument to this operator
        argument_type: Type<T::ScalarType>,
    },
}

impl<T: ConnectorTypes> ComparisonOperatorDefinition<T> {
    pub fn argument_type(self, left_operand_type: &Type<T::ScalarType>) -> Type<T::ScalarType> {
        use ComparisonOperatorDefinition as C;
        match self {
            C::In => Type::ArrayOf(Box::new(left_operand_type.clone())),
            C::Equal
            | C::LessThan
            | C::LessThanOrEqual
            | C::GreaterThan
            | C::GreaterThanOrEqual => left_operand_type.clone(),
            C::Contains
            | C::ContainsInsensitive
            | C::StartsWith
            | C::StartsWithInsensitive
            | C::EndsWith
            | C::EndsWithInsensitive => T::string_type(),
            C::Custom { argument_type } => argument_type,
        }
    }
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub enum ExistsInCollection<T: ConnectorTypes> {
    /// The rows to evaluate the exists predicate against come from a related collection.
    /// Only used if the 'relationships' capability is supported.
    Related {
        /// Key of the relation in the [Query] joins map. Relationships are scoped to the sub-query
        /// that defines the relation source.
        relationship: ndc::RelationshipName,
    },
    /// The rows to evaluate the exists predicate against come from an unrelated collection
    /// Only used if the 'query.exists.unrelated' capability is supported.
    Unrelated {
        /// Key of the relation in the [QueryPlan] joins map. Unrelated collections are not scoped
        /// to a sub-query, instead they are given in the root [QueryPlan].
        unrelated_collection: String,
    },
    /// The rows to evaluate the exists predicate against come from a nested array field.
    /// Only used if the 'query.exists.nested_collections' capability is supported.
    NestedCollection {
        column_name: ndc::FieldName,
        arguments: BTreeMap<ndc::ArgumentName, Argument<T>>,
        /// Path to a nested collection via object columns
        field_path: Vec<ndc::FieldName>,
    },
    /// Specifies a column that contains a nested array of scalars. The
    /// array will be brought into scope of the nested expression where
    /// each element becomes an object with one '__value' column that
    /// contains the element value.
    /// Only used if the 'query.exists.nested_scalar_collections' capability is supported.
    NestedScalarCollection {
        column_name: FieldName,
        arguments: BTreeMap<ArgumentName, Argument<T>>,
        /// Path to a nested collection via object columns
        field_path: Vec<ndc::FieldName>,
    },
}
