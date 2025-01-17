use async_tempfile::TempDir;
use configuration::read_directory;
use mongodb::bson::{self, doc, from_document};
use mongodb_agent_common::mongodb::{test_helpers::mock_stream, MockDatabaseTrait};
use ndc_models::{CollectionName, FieldName, ObjectField, ObjectType, Type};
use pretty_assertions::assert_eq;

use crate::{update, Context, UpdateArgs};

#[tokio::test]
async fn required_field_from_validator_is_non_nullable() -> anyhow::Result<()> {
    let collection_object_type = collection_schema_from_validator(doc! {
        "bsonType": "object",
        "required": ["title"],
        "properties": {
            "title": { "bsonType": "string", "maxLength": 100 },
            "author": { "bsonType": "string", "maxLength": 100 },
        }
    })
    .await?;

    assert_eq!(
        collection_object_type
            .fields
            .get(&FieldName::new("title".into())),
        Some(&ObjectField {
            r#type: Type::Named {
                name: "String".into()
            },
            arguments: Default::default(),
            description: Default::default(),
        })
    );

    assert_eq!(
        collection_object_type
            .fields
            .get(&FieldName::new("author".into())),
        Some(&ObjectField {
            r#type: Type::Nullable {
                underlying_type: Box::new(Type::Named {
                    name: "String".into()
                })
            },
            arguments: Default::default(),
            description: Default::default(),
        })
    );

    Ok(())
}

#[tokio::test]
async fn validator_object_with_no_properties_becomes_extended_json_object() -> anyhow::Result<()> {
    let collection_object_type = collection_schema_from_validator(doc! {
        "bsonType": "object",
        "title": "posts validator",
        "additionalProperties": false,
        "properties": {
            "reactions": { "bsonType": "object" },
        }
    })
    .await?;

    assert_eq!(
        collection_object_type
            .fields
            .get(&FieldName::new("reactions".into())),
        Some(&ObjectField {
            r#type: Type::Nullable {
                underlying_type: Box::new(Type::Named {
                    name: "ExtendedJSON".into()
                })
            },
            arguments: Default::default(),
            description: Default::default(),
        })
    );

    Ok(())
}

async fn collection_schema_from_validator(validator: bson::Document) -> anyhow::Result<ObjectType> {
    let mut db = MockDatabaseTrait::new();
    let config_dir = TempDir::new().await?;

    let context = Context {
        path: config_dir.to_path_buf(),
        connection_uri: None,
        display_color: false,
    };

    let args = UpdateArgs {
        sample_size: Some(100),
        no_validator_schema: None,
        all_schema_nullable: Some(false),
    };

    db.expect_list_collections().returning(move || {
        let collection_spec = doc! {
            "name": "posts",
            "type": "collection",
            "options": {
                "validator": {
                    "$jsonSchema": &validator
                }
            },
            "info": { "readOnly": false },
        };
        Ok(mock_stream(vec![Ok(
            from_document(collection_spec).unwrap()
        )]))
    });

    update(&context, &args, &db).await?;

    let configuration = read_directory(config_dir).await?;

    let collection = configuration
        .collections
        .get(&CollectionName::new("posts".into()))
        .expect("posts collection");
    let collection_object_type = configuration
        .object_types
        .get(&collection.collection_type)
        .expect("posts object type");

    Ok(collection_object_type.clone())
}
