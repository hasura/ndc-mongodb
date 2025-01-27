use std::fmt::Debug;
use std::hash::Hash;

use crate::Type;

pub trait ConnectorTypes {
    type ScalarType: Clone + Debug + Hash + PartialEq + Eq;
    type AggregateFunction: Clone + Debug + Hash + PartialEq + Eq;
    type ComparisonOperator: Clone + Debug + Hash + PartialEq + Eq;

    /// Result type for count aggregations
    fn count_aggregate_type() -> Type<Self::ScalarType>;

    fn string_type() -> Type<Self::ScalarType>;
}
