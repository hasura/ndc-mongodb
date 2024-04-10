use std::collections::BTreeMap;

use configuration::{native_query::NativeQuery, schema as config, Configuration};
use mongodb_support::EXTENDED_JSON_TYPE_NAME;
use ndc_sdk::models::{self as ndc, ArgumentInfo, FunctionInfo};

pub fn function_definitions(configuration: &Configuration) -> BTreeMap<String, FunctionInfo> {
    configuration
        .native_queries
        .iter()
        .map(|(name, native_query)| (name.clone(), native_query_to_function(name, native_query)))
        .collect()
}

fn native_query_to_function(name: &str, native_query: &NativeQuery) -> FunctionInfo {
    let arguments = native_query
        .arguments
        .iter()
        .map(|(name, argument)| {
            (
                name.clone(),
                ArgumentInfo {
                    argument_type: ndc_type(argument.r#type.clone()),
                    description: argument.description.clone(),
                },
            )
        })
        .collect();

    let result_type = ndc_type(native_query.result_type.clone());

    FunctionInfo {
        name: name.to_owned(),
        description: native_query.description.clone(),
        arguments,
        result_type,
    }
}

fn ndc_type(t: config::Type) -> ndc::Type {
    match t {
        config::Type::Scalar(scalar_type) => ndc::Type::Named {
            name: scalar_type.graphql_name(),
        },
        config::Type::Object(name) => ndc::Type::Named { name },
        config::Type::ArrayOf(t) => ndc::Type::Array {
            element_type: Box::new(ndc_type(*t)),
        },
        config::Type::Nullable(t) => ndc::Type::Nullable {
            underlying_type: Box::new(ndc_type(*t)),
        },
        config::Type::ExtendedJSON => ndc::Type::Named {
            name: EXTENDED_JSON_TYPE_NAME.to_owned(),
        },
    }
}
