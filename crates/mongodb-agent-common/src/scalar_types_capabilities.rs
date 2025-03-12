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
use crate::mongo_query_plan as plan;

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
            representation: TypeRepresentation::JSON,
            aggregate_functions: aggregation_functions
                .into_iter()
                .map(|aggregation_function| {
                    use AggregateFunctionDefinition as NDC;
                    use AggregationFunction as Plan;
                    let name = aggregation_function.graphql_name().into();
                    let definition = match aggregation_function {
                        // Using custom instead of standard aggregations because we want the result
                        // types to be ExtendedJSON instead of specific numeric types
                        Plan::Avg => NDC::Custom {
                            result_type: Type::Named {
                                name: mongodb_support::EXTENDED_JSON_TYPE_NAME.into(),
                            },
                        },
                        Plan::Min => NDC::Min,
                        Plan::Max => NDC::Max,
                        Plan::Sum => NDC::Custom {
                            result_type: Type::Named {
                                name: mongodb_support::EXTENDED_JSON_TYPE_NAME.into(),
                            },
                        },
                    };
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
                            element_type: Box::new(ext_json_type.clone()),
                        },
                        C::LessThan => ext_json_type.clone(),
                        C::LessThanOrEqual => ext_json_type.clone(),
                        C::GreaterThan => ext_json_type.clone(),
                        C::GreaterThanOrEqual => ext_json_type.clone(),
                        C::NotEqual => ext_json_type.clone(),
                        C::NotIn => Type::Array {
                            element_type: Box::new(ext_json_type.clone()),
                        },
                        C::Regex | C::IRegex => bson_to_named_type(S::Regex),
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

fn bson_scalar_type_representation(bson_scalar_type: BsonScalarType) -> TypeRepresentation {
    use TypeRepresentation as R;
    match bson_scalar_type {
        S::Double => R::Float64,
        S::Decimal => R::BigDecimal, // Not quite.... Mongo Decimal is 128-bit, BigDecimal is unlimited
        S::Int => R::Int32,
        S::Long => R::Int64,
        S::String => R::String,
        S::Date => R::TimestampTZ, // Mongo Date is milliseconds since unix epoch, but we serialize to JSON as an ISO string
        S::Timestamp => R::JSON,   // Internal Mongo timestamp type
        S::BinData => R::JSON,
        S::UUID => R::String,
        S::ObjectId => R::String, // Mongo ObjectId is usually expressed as a 24 char hex string (12 byte number) - not using R::Bytes because that expects base64
        S::Bool => R::Boolean,
        S::Null => R::JSON,
        S::Regex => R::JSON,
        S::Javascript => R::String,
        S::JavascriptWithScope => R::JSON,
        S::MinKey => R::JSON,
        S::MaxKey => R::JSON,
        S::Undefined => R::JSON,
        S::DbPointer => R::JSON,
        S::Symbol => R::String,
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
        .map(|(fn_name, aggregation_definition)| {
            (fn_name.graphql_name().into(), aggregation_definition)
        })
        .collect()
}

fn bson_to_named_type(bson_scalar_type: BsonScalarType) -> Type {
    Type::Named {
        name: bson_scalar_type.graphql_name().into(),
    }
}

fn bson_to_scalar_type_name(bson_scalar_type: BsonScalarType) -> ndc_models::ScalarTypeName {
    bson_scalar_type.graphql_name().into()
}

fn aggregate_functions(
    scalar_type: BsonScalarType,
) -> impl Iterator<Item = (AggregationFunction, AggregateFunctionDefinition)> {
    use AggregateFunctionDefinition as NDC;
    iter_if(
        scalar_type.is_orderable(),
        [(A::Min, NDC::Min), (A::Max, NDC::Max)].into_iter(),
    )
    .chain(iter_if(
        scalar_type.is_numeric(),
        [
            (
                A::Avg,
                NDC::Average {
                    result_type: bson_to_scalar_type_name(
                        A::expected_result_type(A::Avg, &plan::Type::scalar(scalar_type))
                            .expect("average result type is defined"),
                        // safety: this expect is checked in integration tests
                    ),
                },
            ),
            (
                A::Sum,
                NDC::Sum {
                    result_type: bson_to_scalar_type_name(
                        A::expected_result_type(A::Sum, &plan::Type::scalar(scalar_type))
                            .expect("sum result type is defined"),
                        // safety: this expect is checked in integration tests
                    ),
                },
            ),
        ]
        .into_iter(),
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
