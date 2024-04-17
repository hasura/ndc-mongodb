use std::collections::BTreeMap;

use mongodb_agent_common::{
    comparison_function::ComparisonFunction,
    scalar_types_capabilities::{aggregate_functions, comparison_operators},
};
use mongodb_support::BsonScalarType;
use ndc_sdk::models::{
    AggregateFunctionDefinition, Capabilities, CapabilitiesResponse, ComparisonOperatorDefinition,
    LeafCapability, QueryCapabilities, RelationshipCapabilities, ScalarType, Type,
    TypeRepresentation,
};

pub fn mongo_capabilities_response() -> CapabilitiesResponse {
    ndc_sdk::models::CapabilitiesResponse {
        version: "0.1.2".to_owned(),
        capabilities: Capabilities {
            query: QueryCapabilities {
                aggregates: Some(LeafCapability {}),
                variables: Some(LeafCapability {}),
                explain: Some(LeafCapability {}),
            },
            mutation: ndc_sdk::models::MutationCapabilities {
                transactional: None,
                explain: None,
            },
            relationships: Some(RelationshipCapabilities {
                relation_comparisons: None,
                order_by_aggregate: None,
            }),
        },
    }
}

pub fn scalar_types() -> BTreeMap<String, ScalarType> {
    enum_iterator::all::<BsonScalarType>()
        .map(make_scalar_type)
        .chain([extended_json_scalar_type()])
        .collect::<BTreeMap<_, _>>()
}

fn extended_json_scalar_type() -> (String, ScalarType) {
    (
        mongodb_support::EXTENDED_JSON_TYPE_NAME.to_owned(),
        ScalarType {
            representation: Some(TypeRepresentation::JSON),
            aggregate_functions: BTreeMap::new(),
            comparison_operators: BTreeMap::new(),
        },
    )
}

fn make_scalar_type(bson_scalar_type: BsonScalarType) -> (String, ScalarType) {
    let scalar_type_name = bson_scalar_type.graphql_name();
    let scalar_type = ScalarType {
        representation: bson_scalar_type_representation(bson_scalar_type),
        aggregate_functions: bson_aggregation_functions(bson_scalar_type),
        comparison_operators: bson_comparison_operators(bson_scalar_type),
    };
    (scalar_type_name, scalar_type)
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

fn bson_aggregation_functions(
    bson_scalar_type: BsonScalarType,
) -> BTreeMap<String, AggregateFunctionDefinition> {
    aggregate_functions(bson_scalar_type)
        .map(|(fn_name, result_type)| {
            let aggregation_definition = AggregateFunctionDefinition {
                result_type: bson_to_named_type(result_type),
            };
            (fn_name.graphql_name().to_owned(), aggregation_definition)
        })
        .collect()
}

fn bson_comparison_operators(
    bson_scalar_type: BsonScalarType,
) -> BTreeMap<String, ComparisonOperatorDefinition> {
    comparison_operators(bson_scalar_type)
        .map(|(comparison_fn, arg_type)| {
            let fn_name = comparison_fn.graphql_name().to_owned();
            match comparison_fn {
                ComparisonFunction::Equal => (fn_name, ComparisonOperatorDefinition::Equal),
                _ => (
                    fn_name,
                    ComparisonOperatorDefinition::Custom {
                        argument_type: bson_to_named_type(arg_type),
                    },
                ),
            }
        })
        .collect()
}

fn bson_to_named_type(bson_scalar_type: BsonScalarType) -> Type {
    Type::Named {
        name: bson_scalar_type.graphql_name(),
    }
}
