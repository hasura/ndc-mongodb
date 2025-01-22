use std::collections::BTreeMap;

use derivative::Derivative;
use ndc_models::{self as ndc, ArgumentName, OrderDirection};

use super::{Aggregate, Argument, ConnectorTypes};

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
