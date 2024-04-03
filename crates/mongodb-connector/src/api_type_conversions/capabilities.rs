use std::collections::{BTreeMap, HashMap};

use dc_api_types as v2;
use mongodb_agent_common::comparison_function::ComparisonFunction;
use ndc_sdk::models as v3;

pub fn v2_to_v3_scalar_type_capabilities(
    scalar_types: HashMap<String, v2::ScalarTypeCapabilities>,
) -> BTreeMap<String, v3::ScalarType> {
    scalar_types
        .into_iter()
        .map(|(name, capabilities)| (name, v2_to_v3_capabilities(capabilities)))
        .collect()
}

fn v2_to_v3_capabilities(capabilities: v2::ScalarTypeCapabilities) -> v3::ScalarType {
    v3::ScalarType {
        representation: capabilities.graphql_type.as_ref().map(graphql_type_to_representation),
        aggregate_functions: capabilities
            .aggregate_functions
            .unwrap_or_default()
            .into_iter()
            .map(|(name, result_type)| {
                (
                    name,
                    v3::AggregateFunctionDefinition {
                        result_type: v3::Type::Named { name: result_type },
                    },
                )
            })
            .collect(),
        comparison_operators: capabilities
            .comparison_operators
            .unwrap_or_default()
            .into_iter()
            .map(|(name, argument_type)| {
                let definition = match ComparisonFunction::from_graphql_name(&name).ok() {
                    Some(ComparisonFunction::Equal) => v3::ComparisonOperatorDefinition::Equal,
                    // TODO: Handle "In" NDC-393
                    _ => v3::ComparisonOperatorDefinition::Custom {
                        argument_type: v3::Type::Named {
                            name: argument_type,
                        },
                    },
                };
                (name, definition)
            })
            .collect(),
    }
}

fn graphql_type_to_representation(graphql_type: &v2::GraphQlType) -> v3::TypeRepresentation {
    match graphql_type {
        v2::GraphQlType::Int => v3::TypeRepresentation::Integer,
        v2::GraphQlType::Float => v3::TypeRepresentation::Number,
        v2::GraphQlType::String => v3::TypeRepresentation::String,
        v2::GraphQlType::Boolean => v3::TypeRepresentation::Boolean,
        v2::GraphQlType::Id => v3::TypeRepresentation::String,
    }
}
