use std::{borrow::Cow, collections::BTreeMap};

use derivative::Derivative;
use indexmap::IndexMap;
use ndc_models::{self as ndc, ArgumentName, FieldName};

use crate::Type;

use super::{Argument, ConnectorTypes};

pub type Arguments<T> = BTreeMap<ndc::ArgumentName, Argument<T>>;

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
            Aggregate::StarCount => Cow::Owned(T::count_aggregate_type()),
        }
    }
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub struct Grouping<T: ConnectorTypes> {
    /// Dimensions along which to partition the data
    pub dimensions: Vec<Dimension<T>>,
    /// Aggregates to compute in each group
    pub aggregates: IndexMap<ndc::FieldName, Aggregate<T>>,
    /// Optionally specify a predicate to apply after grouping rows.
    /// Only used if the 'query.aggregates.group_by.filter' capability is supported.
    pub predicate: Option<GroupExpression<T>>,
    /// Optionally specify how groups should be ordered
    /// Only used if the 'query.aggregates.group_by.order' capability is supported.
    pub order_by: Option<GroupOrderBy<T>>,
    /// Optionally limit to N groups
    /// Only used if the 'query.aggregates.group_by.paginate' capability is supported.
    pub limit: Option<u32>,
    /// Optionally offset from the Nth group
    /// Only used if the 'query.aggregates.group_by.paginate' capability is supported.
    pub offset: Option<u32>,
}

/// [GroupExpression] is like [Expression] but without [Expression::ArrayComparison] or
/// [Expression::Exists] variants.
#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub enum GroupExpression<T: ConnectorTypes> {
    And {
        expressions: Vec<GroupExpression<T>>,
    },
    Or {
        expressions: Vec<GroupExpression<T>>,
    },
    Not {
        expression: Box<GroupExpression<T>>,
    },
    UnaryComparisonOperator {
        target: GroupComparisonTarget<T>,
        operator: ndc::UnaryComparisonOperator,
    },
    BinaryComparisonOperator {
        target: GroupComparisonTarget<T>,
        operator: T::ComparisonOperator,
        value: GroupComparisonValue<T>,
    },
}

impl<T: ConnectorTypes> GroupExpression<T> {
    /// In some cases we receive the predicate expression `Some(Expression::And [])` which does not
    /// filter out anything, but fails equality checks with `None`. Simplifying that expression to
    /// `None` allows us to unify relationship references that we wouldn't otherwise be able to.
    pub fn simplify(self) -> Option<Self> {
        match self {
            GroupExpression::And { expressions } if expressions.is_empty() => None,
            e => Some(e),
        }
    }
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub enum GroupComparisonTarget<T: ConnectorTypes> {
    Aggregate { aggregate: Aggregate<T> },
}

impl<T: ConnectorTypes> GroupComparisonTarget<T> {
    pub fn result_type(&self) -> Cow<Type<T::ScalarType>> {
        match self {
            GroupComparisonTarget::Aggregate { aggregate } => aggregate.result_type(),
        }
    }
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub enum GroupComparisonValue<T: ConnectorTypes> {
    /// A scalar value to compare against
    Scalar {
        value: serde_json::Value,
        value_type: Type<T::ScalarType>,
    },
    /// A value to compare against that is to be drawn from the query's variables.
    /// Only used if the 'query.variables' capability is supported.
    Variable {
        name: ndc::VariableName,
        variable_type: Type<T::ScalarType>,
    },
}

#[derive(Derivative)]
#[derivative(
    Clone(bound = ""),
    Debug(bound = ""),
    Hash(bound = ""),
    PartialEq(bound = ""),
    Eq(bound = "")
)]
pub enum Dimension<T: ConnectorTypes> {
    Column {
        /// Any (object) relationships to traverse to reach this column.
        /// Only non-empty if the 'relationships' capability is supported.
        ///
        /// These are translated from [ndc::PathElement] values in the to names of relation fields
        /// for the [crate::QueryPlan].
        path: Vec<ndc::RelationshipName>,
        /// The name of the column
        column_name: FieldName,
        /// Arguments to satisfy the column specified by 'column_name'
        arguments: BTreeMap<ArgumentName, Argument<T>>,
        /// Path to a nested field within an object column
        field_path: Option<Vec<FieldName>>,
        /// Type of the field that you get *after* follwing `field_path` to a possibly-nested
        /// field.
        field_type: Type<T::ScalarType>,
    },
}

impl<T: ConnectorTypes> Dimension<T> {
    pub fn value_type(&self) -> &Type<T::ScalarType> {
        match self {
            Dimension::Column { field_type, .. } => field_type,
        }
    }
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub struct GroupOrderBy<T: ConnectorTypes> {
    /// The elements to order by, in priority order
    pub elements: Vec<GroupOrderByElement<T>>,
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub struct GroupOrderByElement<T: ConnectorTypes> {
    pub order_direction: ndc::OrderDirection,
    pub target: GroupOrderByTarget<T>,
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub enum GroupOrderByTarget<T: ConnectorTypes> {
    Dimension {
        /// The index of the dimension to order by, selected from the
        /// dimensions provided in the `Grouping` request.
        index: usize,
    },
    Aggregate {
        /// Aggregation method to apply
        aggregate: Aggregate<T>,
    },
}
