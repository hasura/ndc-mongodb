use configuration::{
    metadata::{Collection, ObjectField, ObjectType, Type},
    Metadata,
};
use futures_util::{StreamExt, TryStreamExt};
use indexmap::IndexMap;
use mongodb::bson::from_bson;
use mongodb::results::CollectionType;
use mongodb_support::{BsonScalarType, BsonType};
use serde::Deserialize;

use mongodb_agent_common::{
    interface_types::{MongoAgentError, MongoConfig},
    query::collection_name,
};

pub async fn get_metadata_from_validation_schema(
    config: &MongoConfig,
) -> Result<Metadata, MongoAgentError> {
    let db = config.client.database(&config.database);
    let collections_cursor = db.list_collections(None, None).await?;

    let (object_types, collections) = collections_cursor
        .into_stream()
        .map(
            |collection_spec| -> Result<(Vec<ObjectType>, Collection), MongoAgentError> {
                let collection_spec_value = collection_spec?;
                let name = &collection_spec_value.name;
                let collection_type = &collection_spec_value.collection_type;
                let schema_bson_option = collection_spec_value
                    .options
                    .validator
                    .as_ref()
                    .and_then(|x| x.get("$jsonSchema"));

                let table_info = match schema_bson_option {
                    Some(schema_bson) => {
                        from_bson::<ValidatorSchema>(schema_bson.clone()).map_err(|err| {
                            MongoAgentError::BadCollectionSchema(
                                name.to_owned(),
                                schema_bson.clone(),
                                err,
                            )
                        })
                    }
                    None => Ok(ValidatorSchema {
                        bson_type: BsonType::Object,
                        description: None,
                        required: Vec::new(),
                        properties: IndexMap::new(),
                    }),
                }
                .map(|validator_schema| make_collection(name, collection_type, &validator_schema));
                table_info
            },
        )
        .try_collect::<(Vec<Vec<ObjectType>>, Vec<Collection>)>()
        .await?;

    Ok(Metadata {
        collections,
        object_types: object_types.concat(),
    })
}

fn make_collection(
    collection_name: &str,
    collection_type: &CollectionType,
    validator_schema: &ValidatorSchema,
) -> (Vec<ObjectType>, Collection) {
    let properties = &validator_schema.properties;
    let required_labels = &validator_schema.required;

    let (mut object_type_defs, object_fields) = {
        let type_prefix = format!("{collection_name}_");
        let id_field = ObjectField {
            name: "_id".to_string(),
            description: Some("primary key _id".to_string()),
            r#type: Type::Scalar(BsonScalarType::ObjectId),
        };
        let (object_type_defs, mut object_fields): (Vec<Vec<ObjectType>>, Vec<ObjectField>) =
            properties
                .iter()
                .map(|prop| make_object_field(&type_prefix, required_labels, prop))
                .unzip();
        if !object_fields.iter().any(|info| info.name == "_id") {
            // There should always be an _id field, so add it unless it was already specified in
            // the validator.
            object_fields.push(id_field);
        }
        (object_type_defs.concat(), object_fields)
    };

    let collection_type = ObjectType {
        name: collection_name.to_string(),
        description: Some(format!("Object type for collection {collection_name}")),
        fields: object_fields,
    };

    object_type_defs.push(collection_type);

    let collection_info = Collection {
        name: collection_name.to_string(),
        description: validator_schema.description.clone(),
        r#type: collection_name.to_string(),
    };

    (object_type_defs, collection_info)
}

fn make_object_field(
    type_prefix: &str,
    required_labels: &[String],
    (prop_name, prop_schema): (&String, &Property),
) -> (Vec<ObjectType>, ObjectField) {
    let description = get_property_description(prop_schema);

    let object_type_name = format!("{type_prefix}{prop_name}");
    let (collected_otds, field_type) = make_field_type(&object_type_name, prop_schema);

    let object_field = ObjectField {
        name: prop_name.clone(),
        description: description,
        r#type: maybe_nullable(field_type, !required_labels.contains(prop_name)),
    };

    (collected_otds, object_field)
}

fn maybe_nullable(
    t: configuration::metadata::Type,
    is_nullable: bool,
) -> configuration::metadata::Type {
    if is_nullable {
        configuration::metadata::Type::Nullable(Box::new(t))
    } else {
        t
    }
}

fn make_field_type(object_type_name: &str, prop_schema: &Property) -> (Vec<ObjectType>, Type) {
    let mut collected_otds: Vec<ObjectType> = vec![];

    match prop_schema {
        Property::Object {
            bson_type: _,
            description: _,
            required,
            properties,
        } => {
            let type_prefix = format!("{object_type_name}_");
            let (otds, otd_fields): (Vec<Vec<ObjectType>>, Vec<ObjectField>) = properties
                .iter()
                .map(|prop| make_object_field(&type_prefix, required, prop))
                .unzip();

            let object_type_definition = ObjectType {
                name: object_type_name.to_string(),
                description: Some("generated from MongoDB validation schema".to_string()),
                fields: otd_fields,
            };

            collected_otds.append(&mut otds.concat());
            collected_otds.push(object_type_definition);

            (collected_otds, Type::Object(object_type_name.to_string()))
        }
        Property::Array {
            bson_type: _,
            description: _,
            items,
        } => {
            let item_schemas = *items.clone();

            let (mut otds, element_type) = make_field_type(object_type_name, &item_schemas);
            let column_type = Type::ArrayOf(Box::new(element_type));

            collected_otds.append(&mut otds);

            (collected_otds, column_type)
        }
        Property::Scalar {
            bson_type,
            description: _,
        } => (collected_otds, Type::Scalar(bson_type.to_owned())),
    }
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
struct ValidatorSchema {
    #[serde(rename = "bsonType", alias = "type", default = "default_bson_type")]
    #[allow(dead_code)]
    bson_type: BsonType,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default)]
    required: Vec<String>,
    #[serde(default)]
    properties: IndexMap<String, Property>,
}

#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(untagged)]
enum Property {
    Object {
        #[serde(rename = "bsonType", default = "default_bson_type")]
        #[allow(dead_code)]
        bson_type: BsonType,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        required: Vec<String>,
        properties: IndexMap<String, Property>,
    },
    Array {
        #[serde(rename = "bsonType", default = "default_bson_type")]
        #[allow(dead_code)]
        bson_type: BsonType,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        items: Box<Property>,
    },
    Scalar {
        #[serde(rename = "bsonType", default = "default_bson_scalar_type")]
        bson_type: BsonScalarType,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
}

fn get_property_description(p: &Property) -> Option<String> {
    match p {
        Property::Object {
            bson_type: _,
            description,
            required: _,
            properties: _,
        } => description.clone(),
        Property::Array {
            bson_type: _,
            description,
            items: _,
        } => description.clone(),
        Property::Scalar {
            bson_type: _,
            description,
        } => description.clone(),
    }
}

fn default_bson_scalar_type() -> BsonScalarType {
    BsonScalarType::Undefined
}

fn default_bson_type() -> BsonType {
    BsonType::Scalar(default_bson_scalar_type())
}

#[cfg(test)]
mod test {
    use indexmap::IndexMap;
    use mongodb::bson::{bson, from_bson};

    use mongodb_support::{BsonScalarType, BsonType};

    use super::{Property, ValidatorSchema};

    #[test]
    fn parses_scalar_property() -> Result<(), anyhow::Error> {
        let input = bson!({
          "bsonType": "string",
          "description": "'title' must be a string and is required"
        });

        assert_eq!(
            from_bson::<Property>(input)?,
            Property::Scalar {
                bson_type: BsonScalarType::String,
                description: Some("'title' must be a string and is required".to_owned())
            }
        );

        Ok(())
    }

    #[test]
    fn parses_object_property() -> Result<(), anyhow::Error> {
        let input = bson!({
          "bsonType": "object",
          "description": "Name of places",
          "required": [ "name", "description" ],
          "properties": {
            "name": {
              "bsonType": "string",
              "description": "'name' must be a string and is required"
            },
            "description": {
              "bsonType": "string",
              "description": "'description' must be a string and is required"
            }
          }
        });

        assert_eq!(
            from_bson::<Property>(input)?,
            Property::Object {
                bson_type: BsonType::Object,
                description: Some("Name of places".to_owned()),
                required: vec!["name".to_owned(), "description".to_owned()],
                properties: IndexMap::from([
                    (
                        "name".to_owned(),
                        Property::Scalar {
                            bson_type: BsonScalarType::String,
                            description: Some("'name' must be a string and is required".to_owned())
                        }
                    ),
                    (
                        "description".to_owned(),
                        Property::Scalar {
                            bson_type: BsonScalarType::String,
                            description: Some(
                                "'description' must be a string and is required".to_owned()
                            )
                        }
                    )
                ])
            }
        );

        Ok(())
    }

    #[test]
    fn parses_array_property() -> Result<(), anyhow::Error> {
        let input = bson!({
          "bsonType": "array",
          "description": "Location must be an array of objects",
          "uniqueItems": true,
          "items": {
            "bsonType": "object",
            "required": [ "name", "size" ],
            "properties": { "name": { "bsonType": "string" }, "size": { "bsonType": "number" } }
          }
        });

        assert_eq!(
            from_bson::<Property>(input)?,
            Property::Array {
                bson_type: BsonType::Array,
                description: Some("Location must be an array of objects".to_owned()),
                items: Box::new(Property::Object {
                    bson_type: BsonType::Object,
                    description: None,
                    required: vec!["name".to_owned(), "size".to_owned()],
                    properties: IndexMap::from([
                        (
                            "name".to_owned(),
                            Property::Scalar {
                                bson_type: BsonScalarType::String,
                                description: None
                            }
                        ),
                        (
                            "size".to_owned(),
                            Property::Scalar {
                                bson_type: BsonScalarType::Double,
                                description: None
                            }
                        )
                    ])
                }),
            }
        );

        Ok(())
    }

    #[test]
    fn parses_validator_with_alias_field_name() -> Result<(), anyhow::Error> {
        let input = bson!({
            "bsonType": "object",
            "properties": {
                "count": {
                    "bsonType": "number",
                },
            },
            "required": ["count"],
        });

        assert_eq!(
            from_bson::<ValidatorSchema>(input)?,
            ValidatorSchema {
                bson_type: BsonType::Object,
                description: None,
                required: vec!["count".to_owned()],
                properties: IndexMap::from([(
                    "count".to_owned(),
                    Property::Scalar {
                        bson_type: BsonScalarType::Double,
                        description: None,
                    }
                )])
            }
        );
        Ok(())
    }

    #[test]
    fn parses_validator_property_as_object() -> Result<(), anyhow::Error> {
        let input = bson!({
            "bsonType": "object",
            "properties": {
                "counts": {
                    "bsonType": "object",
                    "properties": {
                        "xs": { "bsonType": "number" },
                        "os": { "bsonType": "number" },
                    },
                    "required": ["xs"],
                },
            },
            "required": ["counts"],
        });

        assert_eq!(
            from_bson::<ValidatorSchema>(input)?,
            ValidatorSchema {
                bson_type: BsonType::Object,
                description: None,
                required: vec!["counts".to_owned()],
                properties: IndexMap::from([(
                    "counts".to_owned(),
                    Property::Object {
                        bson_type: BsonType::Object,
                        description: None,
                        required: vec!["xs".to_owned()],
                        properties: IndexMap::from([
                            (
                                "xs".to_owned(),
                                Property::Scalar {
                                    bson_type: BsonScalarType::Double,
                                    description: None
                                }
                            ),
                            (
                                "os".to_owned(),
                                Property::Scalar {
                                    bson_type: BsonScalarType::Double,
                                    description: None
                                }
                            ),
                        ])
                    }
                )])
            }
        );
        Ok(())
    }

    /// This validator is from a test collection that the frontend team uses.
    /// https://github.com/hasura/graphql-engine-mono/blob/main/frontend/docker/DataSources/mongo/init.js
    #[test]
    fn parses_frontend_team_test_validator_students() -> Result<(), anyhow::Error> {
        let input = bson!({
            "bsonType": "object",
            "title": "Student Object Validation",
            "required": ["address", "gpa", "name", "year"],
            "properties": {
                "name": {
                    "bsonType": "string",
                    "description": "\"name\" must be a string and is required"
                },
                "year": {
                    "bsonType": "int",
                    "minimum": 2017,
                    "maximum": 3017,
                    "description": "\"year\" must be an integer in [ 2017, 3017 ] and is required"
                },
                "gpa": {
                    "bsonType": ["double"],
                    "description": "\"gpa\" must be a double if the field exists"
                },
                "address": {
                    "bsonType": ["object"],
                    "properties": {
                        "city": { "bsonType": "string" },
                        "street": { "bsonType": "string" }
                    },
                },
            },
          }
        );
        assert_eq!(
            from_bson::<ValidatorSchema>(input)?,
            ValidatorSchema {
                bson_type: BsonType::Object,
                description: None,
                required: ["address", "gpa", "name", "year"]
                    .into_iter()
                    .map(|s| s.to_owned())
                    .collect(),
                properties: IndexMap::from([
                    (
                        "name".to_owned(),
                        Property::Scalar {
                            bson_type: BsonScalarType::String,
                            description: Some(
                                "\"name\" must be a string and is required".to_owned()
                            ),
                        }
                    ),
                    (
                        "year".to_owned(),
                        Property::Scalar {
                            bson_type: BsonScalarType::Int,
                            description: Some(
                                "\"year\" must be an integer in [ 2017, 3017 ] and is required"
                                    .to_owned()
                            ),
                        }
                    ),
                    (
                        "gpa".to_owned(),
                        Property::Scalar {
                            bson_type: BsonScalarType::Double,
                            description: Some(
                                "\"gpa\" must be a double if the field exists".to_owned()
                            ),
                        }
                    ),
                    (
                        "address".to_owned(),
                        Property::Object {
                            bson_type: BsonType::Object,
                            description: None,
                            required: vec![],
                            properties: IndexMap::from([
                                (
                                    "city".to_owned(),
                                    Property::Scalar {
                                        bson_type: BsonScalarType::String,
                                        description: None,
                                    }
                                ),
                                (
                                    "street".to_owned(),
                                    Property::Scalar {
                                        bson_type: BsonScalarType::String,
                                        description: None,
                                    }
                                )
                            ])
                        }
                    )
                ]),
            }
        );
        Ok(())
    }
}
