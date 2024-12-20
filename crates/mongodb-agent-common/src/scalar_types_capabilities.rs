use std::collections::BTreeMap;

use itertools::Either;
use lazy_static::lazy_static;
use mongodb_support::BsonScalarType;
use ndc_models::{
    AggregateFunctionDefinition, AggregateFunctionName, ComparisonOperatorDefinition,
    ComparisonOperatorName, ScalarType, Type, TypeRepresentation,
};

use crate::aggregation_function::{AggregationFunction, AggregationFunction as A};
use crate::comparison_function::{ComparisonFunction, ComparisonFunction as C};

use BsonScalarType as S;

lazy_static! {
    pub static ref SCALAR_TYPES: BTreeMap<ndc_models::ScalarTypeName, ScalarType> = scalar_types();
}

pub fn scalar_types() -> BTreeMap<ndc_models::ScalarTypeName, ScalarType> {
    enum_iterator::all::<BsonScalarType>()
        .map(make_scalar_type)
        .chain([extended_json_scalar_type()])
        .collect::<BTreeMap<_, _>>()
}

fn extended_json_scalar_type() -> (ndc_models::ScalarTypeName, ScalarType) {
    // Extended JSON could be anything, so allow all aggregation functions
    let aggregation_functions = enum_iterator::all::<AggregationFunction>();

    // Extended JSON could be anything, so allow all comparison operators
    let comparison_operators = enum_iterator::all::<ComparisonFunction>();

    let ext_json_type = Type::Named {
        name: mongodb_support::EXTENDED_JSON_TYPE_NAME.into(),
    };

    (
        mongodb_support::EXTENDED_JSON_TYPE_NAME.into(),
        ScalarType {
            representation: Some(TypeRepresentation::JSON),
            aggregate_functions: aggregation_functions
                .into_iter()
                .map(|aggregation_function| {
                    let name = aggregation_function.graphql_name().into();
                    let result_type = match aggregation_function {
                        AggregationFunction::Avg => ext_json_type.clone(),
                        AggregationFunction::Count => bson_to_named_type(S::Int),
                        AggregationFunction::Min => ext_json_type.clone(),
                        AggregationFunction::Max => ext_json_type.clone(),
                        AggregationFunction::Sum => ext_json_type.clone(),
                    };
                    let definition = AggregateFunctionDefinition { result_type };
                    (name, definition)
                })
                .collect(),
            comparison_operators: comparison_operators
                .into_iter()
                .map(|comparison_fn| {
                    let name = comparison_fn.graphql_name().into();
                    let ndc_definition = comparison_fn.ndc_definition(|func| match func {
                        C::Equal => ext_json_type.clone(),
                        C::In => Type::Array {
                            element_type: ext_json_type.clone(),
                        },
                        C::LessThan => ext_json_type.clone(),
                        C::LessThanOrEqual => ext_json_type.clone(),
                        C::GreaterThan => ext_json_type.clone(),
                        C::GreaterThanOrEqual => ext_json_type.clone(),
                        C::Equal => ext_json_type.clone(),
                        C::NotEqual => ext_json_type.clone(),
                        C::NotIn => Type::Array {
                            element_type: ext_json_type.clone(),
                        },
                        C::Regex | C::IRegex => ComparisonOperatorDefinition::Custom {
                            argument_type: bson_to_named_type(S::Regex),
                        },
                    });
                    (name, ndc_definition)
                })
                .collect(),
        },
    )
}

fn make_scalar_type(bson_scalar_type: BsonScalarType) -> (ndc_models::ScalarTypeName, ScalarType) {
    let scalar_type_name = bson_scalar_type.graphql_name();
    let scalar_type = ScalarType {
        representation: bson_scalar_type_representation(bson_scalar_type),
        aggregate_functions: bson_aggregation_functions(bson_scalar_type),
        comparison_operators: bson_comparison_operators(bson_scalar_type),
    };
    (scalar_type_name.into(), scalar_type)
}

fn bson_scalar_type_representation(bson_scalar_type: BsonScalarType) -> Option<TypeRepresentation> {
    match bson_scalar_type {
        BsonScalarType::Double => Some(TypeRepresentation::Float64),
        BsonScalarType::Decimal => Some(TypeRepresentation::BigDecimal), // Not quite.... Mongo Decimal is 128-bit, BigDecimal is unlimited
        BsonScalarType::Int => Some(TypeRepresentation::Int32),
        BsonScalarType::Long => Some(TypeRepresentation::Int64),
        BsonScalarType::String => Some(TypeRepresentation::String),
        BsonScalarType::Date => Some(TypeRepresentation::Timestamp), // Mongo Date is milliseconds since unix epoch
        BsonScalarType::Timestamp => None, // Internal Mongo timestamp type
        BsonScalarType::BinData => None,
        BsonScalarType::ObjectId => Some(TypeRepresentation::String), // Mongo ObjectId is usually expressed as a 24 char hex string (12 byte number)
        BsonScalarType::Bool => Some(TypeRepresentation::Boolean),
        BsonScalarType::Null => None,
        BsonScalarType::Regex => None,
        BsonScalarType::Javascript => None,
        BsonScalarType::JavascriptWithScope => None,
        BsonScalarType::MinKey => None,
        BsonScalarType::MaxKey => None,
        BsonScalarType::Undefined => None,
        BsonScalarType::DbPointer => None,
        BsonScalarType::Symbol => None,
    }
}

fn bson_comparison_operators(
    bson_scalar_type: BsonScalarType,
) -> BTreeMap<ComparisonOperatorName, ComparisonOperatorDefinition> {
    comparison_operators(bson_scalar_type)
        .map(|(comparison_fn, argument_type)| {
            let fn_name = comparison_fn.graphql_name().into();
            (fn_name, comparison_fn.ndc_definition(|_| argument_type))
        })
        .collect()
}

fn bson_aggregation_functions(
    bson_scalar_type: BsonScalarType,
) -> BTreeMap<AggregateFunctionName, AggregateFunctionDefinition> {
    aggregate_functions(bson_scalar_type)
        .map(|(fn_name, result_type)| {
            let aggregation_definition = AggregateFunctionDefinition { result_type };
            (fn_name.graphql_name().into(), aggregation_definition)
        })
        .collect()
}

fn bson_to_named_type(bson_scalar_type: BsonScalarType) -> Type {
    Type::Named {
        name: bson_scalar_type.graphql_name().into(),
    }
}

pub fn aggregate_functions(
    scalar_type: BsonScalarType,
) -> impl Iterator<Item = (AggregationFunction, Type)> {
    let nullable_scalar_type = move || Type::Nullable {
        underlying_type: Box::new(bson_to_named_type(scalar_type)),
    };
    [(A::Count, bson_to_named_type(S::Int))]
        .into_iter()
        .chain(iter_if(
            scalar_type.is_orderable(),
            [A::Min, A::Max]
                .into_iter()
                .map(move |op| (op, nullable_scalar_type())),
        ))
        .chain(iter_if(
            scalar_type.is_numeric(),
            [A::Avg, A::Sum]
                .into_iter()
                .map(move |op| (op, nullable_scalar_type())),
        ))
}

pub fn comparison_operators(
    scalar_type: BsonScalarType,
) -> impl Iterator<Item = (ComparisonFunction, Type)> {
    iter_if(
        scalar_type.is_comparable(),
        [
            (C::Equal, bson_to_named_type(scalar_type)),
            (C::NotEqual, bson_to_named_type(scalar_type)),
            (
                C::In,
                Type::Array {
                    element_type: Box::new(bson_to_named_type(scalar_type)),
                },
            ),
            (
                C::NotIn,
                Type::Array {
                    element_type: Box::new(bson_to_named_type(scalar_type)),
                },
            ),
            (C::NotEqual, bson_to_named_type(scalar_type)),
        ]
        .into_iter(),
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
        .map(move |op| (op, bson_to_named_type(scalar_type))),
    ))
    .chain(match scalar_type {
        S::String => Box::new(
            [
                (C::Regex, bson_to_named_type(S::Regex)),
                (C::IRegex, bson_to_named_type(S::Regex)),
            ]
            .into_iter(),
        ),
        _ => Box::new(std::iter::empty()) as Box<dyn Iterator<Item = (C, Type)>>,
    })
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
