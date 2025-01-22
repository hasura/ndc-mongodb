use std::fmt::Debug;

use crate::Type;

pub trait ConnectorTypes {
    type ScalarType: Clone + Debug + PartialEq + Eq;
    type AggregateFunction: Clone + Debug + PartialEq;
    type ComparisonOperator: Clone + Debug + PartialEq;

    /// Result type for count aggregations
    fn count_aggregate_type() -> Type<Self::ScalarType>;

    fn string_type() -> Type<Self::ScalarType>;
}
