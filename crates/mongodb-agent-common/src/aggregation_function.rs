use configuration::MongoScalarType;
use enum_iterator::{all, Sequence};

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, Sequence)]
pub enum AggregationFunction {
    Avg,
    Min,
    Max,
    Sum,
}

use mongodb_support::BsonScalarType;
use ndc_query_plan::QueryPlanError;
use AggregationFunction as A;

use crate::mongo_query_plan::Type;

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

    /// Returns the result type that is declared for this function in the schema.
    pub fn expected_result_type(self, argument_type: &Type) -> Option<BsonScalarType> {
        match self {
            A::Avg => Some(BsonScalarType::Double),
            A::Min => None,
            A::Max => None,
            A::Sum => Some(if is_fractional(argument_type) {
                BsonScalarType::Double
            } else {
                BsonScalarType::Long
            }),
        }
    }
}

fn is_fractional(t: &Type) -> bool {
    match t {
        Type::Scalar(MongoScalarType::Bson(s)) => s.is_fractional(),
        Type::Scalar(MongoScalarType::ExtendedJSON) => true,
        Type::Object(_) => false,
        Type::ArrayOf(_) => false,
        Type::Tuple(ts) => ts.iter().all(is_fractional),
        Type::Nullable(t) => is_fractional(t),
    }
}
