use std::collections::{BTreeMap, BTreeSet};

use mongodb_agent_common::{
    mongo_query_plan::MongoConfiguration, scalar_types_capabilities::SCALAR_TYPES,
};
use mongodb_support::{BsonScalarType, EXTENDED_JSON_TYPE_NAME};
use ndc_query_plan::QueryContext as _;
use ndc_sdk::{connector, models as ndc};

pub async fn get_schema(config: &MongoConfiguration) -> connector::Result<ndc::SchemaResponse> {
    let scalar_types = if config.relational_mode().enabled {
        scalar_types_for_relational_mode()
    } else {
        SCALAR_TYPES.clone()
    };
    let object_types = if config.relational_mode().enabled {
        object_types_for_relational_mode(config)
    } else {
        config
            .object_types()
            .iter()
            .map(|(name, object_type)| (name.clone(), object_type.clone()))
            .collect()
    };

    let schema = ndc::SchemaResponse {
        collections: config.collections().values().cloned().collect(),
        functions: config
            .functions()
            .values()
            .map(|(f, _)| f)
            .cloned()
            .collect(),
        procedures: config.procedures().values().cloned().collect(),
        object_types,
        scalar_types,
        capabilities: Some(ndc::CapabilitySchemaInfo {
            query: Some(ndc::QueryCapabilitiesSchemaInfo {
                aggregates: Some(ndc::AggregateCapabilitiesSchemaInfo {
                    count_scalar_type: BsonScalarType::Int.graphql_name().into(),
                }),
            }),
        }),
        request_arguments: None,
    };
    tracing::debug!(schema = %serde_json::to_string(&schema).unwrap(), "get_schema");
    Ok(schema)
}

/// Returns scalar types with JSON/nested types having String representation.
/// This is used when relational mode is enabled to ensure nested data is
/// serialized as JSON strings for SQL-style query compatibility.
fn scalar_types_for_relational_mode() -> BTreeMap<ndc::ScalarTypeName, ndc::ScalarType> {
    SCALAR_TYPES
        .iter()
        .map(|(name, scalar_type)| {
            let modified_type = match &scalar_type.representation {
                // Convert JSON representation to String for relational mode
                ndc::TypeRepresentation::JSON => ndc::ScalarType {
                    representation: ndc::TypeRepresentation::String,
                    ..scalar_type.clone()
                },
                // Keep other representations as-is
                _ => scalar_type.clone(),
            };
            (name.clone(), modified_type)
        })
        .collect()
}

/// Rewrites object/array/predicate references to ExtendedJSON in relational mode so
/// nested data is represented as strings in the relational schema.
fn object_types_for_relational_mode(
    config: &MongoConfiguration,
) -> BTreeMap<ndc::ObjectTypeName, ndc::ObjectType> {
    let object_type_names: BTreeSet<ndc::TypeName> = config
        .object_types()
        .keys()
        .map(|type_name| type_name.to_string().into())
        .collect();

    config
        .object_types()
        .iter()
        .map(|(type_name, object_type)| {
            let rewritten_fields = object_type
                .fields
                .iter()
                .map(|(field_name, field)| {
                    let rewritten_field = ndc::ObjectField {
                        description: field.description.clone(),
                        r#type: rewrite_nested_type_to_extended_json(
                            &field.r#type,
                            &object_type_names,
                        ),
                        arguments: field
                            .arguments
                            .iter()
                            .map(|(arg_name, arg_info)| {
                                (
                                    arg_name.clone(),
                                    ndc::ArgumentInfo {
                                        description: arg_info.description.clone(),
                                        argument_type: rewrite_nested_type_to_extended_json(
                                            &arg_info.argument_type,
                                            &object_type_names,
                                        ),
                                    },
                                )
                            })
                            .collect(),
                    };
                    (field_name.clone(), rewritten_field)
                })
                .collect();
            (
                type_name.clone(),
                ndc::ObjectType {
                    description: object_type.description.clone(),
                    fields: rewritten_fields,
                    foreign_keys: object_type.foreign_keys.clone(),
                },
            )
        })
        .collect()
}

fn rewrite_nested_type_to_extended_json(
    input_type: &ndc::Type,
    object_type_names: &BTreeSet<ndc::TypeName>,
) -> ndc::Type {
    match input_type {
        ndc::Type::Named { name } => {
            if object_type_names.contains(name) {
                ndc::Type::Named {
                    name: EXTENDED_JSON_TYPE_NAME.to_owned().into(),
                }
            } else {
                input_type.clone()
            }
        }
        ndc::Type::Array { .. } | ndc::Type::Predicate { .. } => ndc::Type::Named {
            name: EXTENDED_JSON_TYPE_NAME.to_owned().into(),
        },
        ndc::Type::Nullable { underlying_type } => ndc::Type::Nullable {
            underlying_type: Box::new(rewrite_nested_type_to_extended_json(
                underlying_type,
                object_type_names,
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use configuration::{Configuration, ConfigurationOptions, RelationalModeConfig};
    use mongodb_agent_common::mongo_query_plan::MongoConfiguration;
    use ndc_sdk::models as ndc;

    use super::get_schema;

    fn named_type(name: &str) -> ndc::Type {
        ndc::Type::Named { name: name.into() }
    }

    fn nullable_type(t: ndc::Type) -> ndc::Type {
        ndc::Type::Nullable {
            underlying_type: Box::new(t),
        }
    }

    fn array_of_type(t: ndc::Type) -> ndc::Type {
        ndc::Type::Array {
            element_type: Box::new(t),
        }
    }

    fn object_type(fields: &[(&str, ndc::Type)]) -> ndc::ObjectType {
        ndc::ObjectType {
            description: None,
            fields: fields
                .iter()
                .map(|(field_name, field_type)| {
                    (
                        (*field_name).into(),
                        ndc::ObjectField {
                            description: None,
                            r#type: field_type.clone(),
                            arguments: BTreeMap::new(),
                        },
                    )
                })
                .collect(),
            foreign_keys: BTreeMap::new(),
        }
    }

    #[tokio::test]
    async fn relational_mode_rewrites_nested_types_to_string_backed_scalar() {
        let collections = BTreeMap::from([(
            "authors".into(),
            ndc::CollectionInfo {
                name: "authors".into(),
                description: None,
                arguments: BTreeMap::new(),
                collection_type: "authors".into(),
                uniqueness_constraints: BTreeMap::new(),
                relational_mutations: None,
            },
        )]);

        let config = Configuration {
            collections,
            object_types: BTreeMap::from([
                (
                    "authors".into(),
                    object_type(&[
                        ("name", named_type("String")),
                        ("address", named_type("Address")),
                        ("nullable_address", nullable_type(named_type("Address"))),
                        ("tags", array_of_type(named_type("String"))),
                        ("articles", array_of_type(named_type("Article"))),
                    ]),
                ),
                (
                    "Address".into(),
                    object_type(&[("street", named_type("String"))]),
                ),
                (
                    "Article".into(),
                    object_type(&[("title", named_type("String"))]),
                ),
            ]),
            options: ConfigurationOptions {
                relational_mode: RelationalModeConfig { enabled: true },
                ..Default::default()
            },
            ..Default::default()
        };
        let schema = get_schema(&MongoConfiguration(config))
            .await
            .expect("schema should be generated");

        let authors_type = schema
            .object_types
            .get(&"authors".into())
            .expect("authors object type should exist");

        assert_eq!(
            authors_type.fields["name"].r#type,
            ndc::Type::Named {
                name: "String".into()
            }
        );
        assert_eq!(
            authors_type.fields["address"].r#type,
            ndc::Type::Named {
                name: "ExtendedJSON".into()
            }
        );
        assert_eq!(
            authors_type.fields["nullable_address"].r#type,
            ndc::Type::Nullable {
                underlying_type: Box::new(ndc::Type::Named {
                    name: "ExtendedJSON".into()
                })
            }
        );
        assert_eq!(
            authors_type.fields["tags"].r#type,
            ndc::Type::Named {
                name: "ExtendedJSON".into()
            }
        );
        assert_eq!(
            authors_type.fields["articles"].r#type,
            ndc::Type::Named {
                name: "ExtendedJSON".into()
            }
        );

        let ext_json = schema
            .scalar_types
            .get(&"ExtendedJSON".into())
            .expect("ExtendedJSON scalar should exist");
        assert_eq!(ext_json.representation, ndc::TypeRepresentation::String);
    }
}
