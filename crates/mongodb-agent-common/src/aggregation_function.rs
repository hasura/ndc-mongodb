use enum_iterator::{all, Sequence};

// TODO: How can we unify this with the Accumulator type in the mongodb module?
#[derive(Copy, Clone, Debug, PartialEq, Eq, Sequence)]
pub enum AggregationFunction {
    Avg,
    Count,
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
            A::Count => "count",
            A::Min => "min",
            A::Max => "max",
            A::Sum => "sum",
        }
    }

    pub fn from_graphql_name(s: &str) -> Result<Self, QueryPlanError> {
        all::<AggregationFunction>()
            .find(|variant| variant.graphql_name() == s)
            .ok_or(QueryPlanError::UnknownAggregateFunction {
                aggregate_function: s.to_owned(),
            })
    }

    pub fn is_count(self) -> bool {
        match self {
            A::Avg => false,
            A::Count => true,
            A::Min => false,
            A::Max => false,
            A::Sum => false,
        }
    }
}
