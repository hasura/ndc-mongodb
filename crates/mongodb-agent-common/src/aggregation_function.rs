use enum_iterator::{all, Sequence};

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, Sequence)]
pub enum AggregationFunction {
    Avg,
    Min,
    Max,
    Sum,
}

use ndc_query_plan::QueryPlanError;
use AggregationFunction as A;

impl AggregationFunction {
    pub fn graphql_name(self) -> &'static str {
        match self {
            A::Avg => "avg",
            A::Min => "min",
            A::Max => "max",
            A::Sum => "sum",
        }
    }

    pub fn from_graphql_name(s: &str) -> Result<Self, QueryPlanError> {
        all::<AggregationFunction>()
            .find(|variant| variant.graphql_name() == s)
            .ok_or(QueryPlanError::UnknownAggregateFunction {
                aggregate_function: s.to_owned().into(),
            })
    }
}
