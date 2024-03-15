use dc_api_types::GqlName;
use dc_api_types::{
    ColumnInfo, ColumnType, ObjectTypeDefinition, SchemaResponse, TableInfo, TableType,
};
use futures_util::{StreamExt, TryStreamExt};
use indexmap::IndexMap;
use mongodb::bson::from_bson;
use mongodb::results::CollectionType;
use mongodb_support::{BsonScalarType, BsonType};
use serde::Deserialize;

use crate::interface_types::{MongoAgentError, MongoConfig};

pub async fn get_schema(config: &MongoConfig) -> Result<SchemaResponse, MongoAgentError> {
    tracing::debug!(?config, "get_schema");

    let db = config.client.database(&config.database);
    let collections_cursor = db.list_collections(None, None).await?;

    let (object_types, tables) = collections_cursor
        .into_stream()
        .map(
            |collection_spec| -> Result<(Vec<ObjectTypeDefinition>, TableInfo), MongoAgentError> {
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
                .map(|validator_schema| {
                    make_table_info(name, collection_type,  &validator_schema)
                });
                tracing::debug!(
                    validator = %serde_json::to_string(&schema_bson_option).unwrap(),
                    table_info = %table_info.as_ref().map(|(_, info)| serde_json::to_string(&info).unwrap()).unwrap_or("null".to_owned()),
                );
                table_info
            },
        )
        .try_collect::<(Vec<Vec<ObjectTypeDefinition>>, Vec<TableInfo>)>()
        .await?;

    Ok(SchemaResponse {
        tables,
        object_types: object_types.concat(),
    })
}

fn make_table_info(
    collection_name: &str,
    collection_type: &CollectionType,
    validator_schema: &ValidatorSchema,
) -> (Vec<ObjectTypeDefinition>, TableInfo) {
    let properties = &validator_schema.properties;
    let required_labels = &validator_schema.required;

    let (object_type_defs, column_infos) = {
        let type_prefix = format!("{collection_name}_");
        let id_column = ColumnInfo {
            name: "_id".to_string(),
            r#type: ColumnType::Scalar(BsonScalarType::ObjectId.graphql_name()),
            nullable: false,
            description: Some(Some("primary key _id".to_string())),
            insertable: Some(false),
            updatable: Some(false),
            value_generated: None,
        };
        let (object_type_defs, mut columns_infos): (
            Vec<Vec<ObjectTypeDefinition>>,
            Vec<ColumnInfo>,
        ) = properties
            .iter()
            .map(|prop| make_column_info(&type_prefix, required_labels, prop))
            .unzip();
        if !columns_infos.iter().any(|info| info.name == "_id") {
            // There should always be an _id column, so add it unless it was already specified in
            // the validator.
            columns_infos.push(id_column);
        }
        (object_type_defs.concat(), columns_infos)
    };

    let table_info = TableInfo {
        name: vec![collection_name.to_string()],
        r#type: if collection_type == &CollectionType::View {
            Some(TableType::View)
        } else {
            Some(TableType::Table)
        },
        columns: column_infos,
        primary_key: Some(vec!["_id".to_string()]),
        foreign_keys: None,
        description: validator_schema.description.clone().map(Some),
        // Since we don't support mutations nothing is insertable, updatable, or deletable
        insertable: Some(false),
        updatable: Some(false),
        deletable: Some(false),
    };
    (object_type_defs, table_info)
}

fn make_column_info(
    type_prefix: &str,
    required_labels: &[String],
    (column_name, column_schema): (&String, &Property),
) -> (Vec<ObjectTypeDefinition>, ColumnInfo) {
    let description = get_property_description(column_schema);

    let object_type_name = format!("{type_prefix}{column_name}");
    let (collected_otds, column_type) = make_column_type(&object_type_name, column_schema);

    let column_info = ColumnInfo {
        name: column_name.clone(),
        r#type: column_type,
        nullable: !required_labels.contains(column_name),
        description: description.map(Some),
        // Since we don't support mutations nothing is insertable, updatable, or deletable
        insertable: Some(false),
        updatable: Some(false),
        value_generated: None,
    };

    (collected_otds, column_info)
}

fn make_column_type(
    object_type_name: &str,
    column_schema: &Property,
) -> (Vec<ObjectTypeDefinition>, ColumnType) {
    let mut collected_otds: Vec<ObjectTypeDefinition> = vec![];

    match column_schema {
        Property::Object {
            bson_type: _,
            description: _,
            required,
            properties,
        } => {
            let type_prefix = format!("{object_type_name}_");
            let (otds, otd_columns): (Vec<Vec<ObjectTypeDefinition>>, Vec<ColumnInfo>) = properties
                .iter()
                .map(|prop| make_column_info(&type_prefix, required, prop))
                .unzip();

            let object_type_definition = ObjectTypeDefinition {
                name: GqlName::from(object_type_name).into_owned(),
                description: Some("generated from MongoDB validation schema".to_string()),
                columns: otd_columns,
            };

            collected_otds.append(&mut otds.concat());
            collected_otds.push(object_type_definition);

            (
                collected_otds,
                ColumnType::Object(GqlName::from(object_type_name).into_owned()),
            )
        }
        Property::Array {
            bson_type: _,
            description: _,
            items,
        } => {
            let item_schemas = *items.clone();

            let (mut otds, element_type) = make_column_type(object_type_name, &item_schemas);
            let column_type = ColumnType::Array {
                element_type: Box::new(element_type),
                nullable: false,
            };

            collected_otds.append(&mut otds);

            (collected_otds, column_type)
        }
        Property::Scalar {
            bson_type,
            description: _,
        } => (collected_otds, ColumnType::Scalar(bson_type.graphql_name())),
    }
}

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct ValidatorSchema {
    #[serde(rename = "bsonType", alias = "type", default = "default_bson_type")]
    #[allow(dead_code)]
    pub bson_type: BsonType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub properties: IndexMap<String, Property>,
}

#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(untagged)]
pub enum Property {
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

pub fn get_property_description(p: &Property) -> Option<String> {
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
