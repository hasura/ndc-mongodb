use dc_api_types::BinaryComparisonOperator;
use enum_iterator::{all, Sequence};
use mongodb::bson::{doc, Bson, Document};

/// Supported binary comparison operators. This type provides GraphQL names, MongoDB operator
/// names, and aggregation pipeline code for each operator. Argument types are defined in
/// mongodb-agent-common/src/scalar_types_capabilities.rs.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Sequence)]
pub enum ComparisonFunction {
    // Equality and inequality operators (except for `NotEqual`) are built into the v2 spec, but
    // the only built-in operator in v3 is `Equal`. So we need at minimum definitions for
    // inequality operators here.
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Equal,
    NotEqual,

    Regex,
    /// case-insensitive regex
    IRegex,
}

use BinaryComparisonOperator as B;
use ComparisonFunction as C;

use crate::interface_types::MongoAgentError;

impl ComparisonFunction {
    pub fn graphql_name(self) -> &'static str {
        match self {
            C::LessThan => "_lt",
            C::LessThanOrEqual => "_lte",
            C::GreaterThan => "_gt",
            C::GreaterThanOrEqual => "_gte",
            C::Equal => "_eq",
            C::NotEqual => "_neq",
            C::Regex => "_regex",
            C::IRegex => "_iregex",
        }
    }

    pub fn mongodb_name(self) -> &'static str {
        match self {
            C::LessThan => "$lt",
            C::LessThanOrEqual => "$lte",
            C::GreaterThan => "$gt",
            C::GreaterThanOrEqual => "$gte",
            C::Equal => "$eq",
            C::NotEqual => "$ne",
            C::Regex => "$regex",
            C::IRegex => "$regex",
        }
    }

    pub fn from_graphql_name(s: &str) -> Result<Self, MongoAgentError> {
        all::<ComparisonFunction>()
            .find(|variant| variant.graphql_name() == s)
            .ok_or(MongoAgentError::UnknownAggregationFunction(s.to_owned()))
    }

    /// Produce a MongoDB expression that applies this function to the given operands.
    pub fn mongodb_expression(self, column_ref: String, comparison_value: Bson) -> Document {
        match self {
            C::IRegex => {
                doc! { column_ref: { self.mongodb_name(): comparison_value, "$options": "i" } }
            }
            _ => doc! { column_ref: { self.mongodb_name(): comparison_value } },
        }
    }
}

impl TryFrom<&BinaryComparisonOperator> for ComparisonFunction {
    type Error = MongoAgentError;

    fn try_from(operator: &BinaryComparisonOperator) -> Result<Self, Self::Error> {
        match operator {
            B::LessThan => Ok(C::LessThan),
            B::LessThanOrEqual => Ok(C::LessThanOrEqual),
            B::GreaterThan => Ok(C::GreaterThan),
            B::GreaterThanOrEqual => Ok(C::GreaterThanOrEqual),
            B::Equal => Ok(C::Equal),
            B::CustomBinaryComparisonOperator(op) => ComparisonFunction::from_graphql_name(op),
        }
    }
}
