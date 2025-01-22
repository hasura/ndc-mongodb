use std::{borrow::Cow, collections::BTreeMap};

use derivative::Derivative;
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

// TODO: define Grouping
