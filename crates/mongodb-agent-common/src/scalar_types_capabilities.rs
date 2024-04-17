use std::collections::HashMap;

use dc_api_types::ScalarTypeCapabilities;
use enum_iterator::all;
use itertools::Either;
use mongodb_support::BsonScalarType;

use crate::aggregation_function::{AggregationFunction, AggregationFunction as A};
use crate::comparison_function::{ComparisonFunction, ComparisonFunction as C};

use BsonScalarType as S;

pub fn scalar_types_capabilities() -> HashMap<String, ScalarTypeCapabilities> {
    let mut map = all::<BsonScalarType>()
        .map(|t| (t.graphql_name(), capabilities(t)))
        .collect::<HashMap<_, _>>();
    map.insert(
        mongodb_support::EXTENDED_JSON_TYPE_NAME.to_owned(),
        ScalarTypeCapabilities::new(),
    );
    map
}

pub fn aggregate_functions(
    scalar_type: BsonScalarType,
) -> impl Iterator<Item = (AggregationFunction, BsonScalarType)> {
    [(A::Count, S::Int)]
        .into_iter()
        .chain(iter_if(
            scalar_type.is_orderable(),
            [A::Min, A::Max]
                .into_iter()
                .map(move |op| (op, scalar_type)),
        ))
        .chain(iter_if(
            scalar_type.is_numeric(),
            [A::Avg, A::Sum]
                .into_iter()
                .map(move |op| (op, scalar_type)),
        ))
}

pub fn comparison_operators(
    scalar_type: BsonScalarType,
) -> impl Iterator<Item = (ComparisonFunction, BsonScalarType)> {
    iter_if(
        scalar_type.is_comparable(),
        [(C::Equal, scalar_type), (C::NotEqual, scalar_type)].into_iter(),
    )
    .chain(iter_if(
        scalar_type.is_orderable(),
        [
            C::LessThan,
            C::LessThanOrEqual,
            C::GreaterThan,
            C::GreaterThanOrEqual,
        ]
        .into_iter()
        .map(move |op| (op, scalar_type)),
    ))
    .chain(match scalar_type {
        S::String => Box::new([(C::Regex, S::String), (C::IRegex, S::String)].into_iter()),
        _ => Box::new(std::iter::empty()) as Box<dyn Iterator<Item = (C, S)>>,
    })
}

fn capabilities(scalar_type: BsonScalarType) -> ScalarTypeCapabilities {
    let aggregations: HashMap<String, String> = aggregate_functions(scalar_type)
        .map(|(a, t)| (a.graphql_name().to_owned(), t.graphql_name()))
        .collect();
    let comparisons: HashMap<String, String> = comparison_operators(scalar_type)
        .map(|(c, t)| (c.graphql_name().to_owned(), t.graphql_name()))
        .collect();
    ScalarTypeCapabilities {
        graphql_type: scalar_type.graphql_type(),
        aggregate_functions: Some(aggregations),
        comparison_operators: if comparisons.is_empty() {
            None
        } else {
            Some(comparisons)
        },
        update_column_operators: None,
    }
}

/// If `condition` is true returns an iterator with the same items as the given `iter` input.
/// Otherwise returns an empty iterator.
fn iter_if<Item>(condition: bool, iter: impl Iterator<Item = Item>) -> impl Iterator<Item = Item> {
    if condition {
        Either::Right(iter)
    } else {
        Either::Left(std::iter::empty())
    }
}
