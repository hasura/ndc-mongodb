use std::{collections::BTreeMap, path::Path};

use async_tempfile::TempDir;
use configuration::{read_directory, Configuration};
use googletest::prelude::*;
use itertools::Itertools as _;
use mongodb::{
    bson::{self, doc, from_document, Bson},
    options::AggregateOptions,
};
use mongodb_agent_common::mongodb::{
    test_helpers::mock_stream, MockCollectionTrait, MockDatabaseTrait,
};
use ndc_models::{CollectionName, FieldName, ObjectField, ObjectType, Type};
use ndc_test_helpers::{array_of, named_type, nullable, object_type};
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

#[gtest]
#[tokio::test]
async fn adds_new_fields_on_re_introspection() -> anyhow::Result<()> {
    let config_dir = TempDir::new().await?;
    schema_from_sampling(
        &config_dir,
        vec![doc! { "title": "First post!", "author": "Alice" }],
    )
    .await?;

    // re-introspect after database changes
    let configuration = schema_from_sampling(
        &config_dir,
        vec![doc! { "title": "First post!", "author": "Alice", "body": "Hello, world!" }],
    )
    .await?;

    let updated_type = configuration
        .object_types
        .get("posts")
        .expect("got posts collection type");

    expect_that!(
        updated_type.fields,
        unordered_elements_are![
            (
                displays_as(eq("title")),
                field!(ObjectField.r#type, eq(&named_type("String")))
            ),
            (
                displays_as(eq("author")),
                field!(ObjectField.r#type, eq(&named_type("String")))
            ),
            (
                displays_as(eq("body")),
                field!(ObjectField.r#type, eq(&named_type("String")))
            ),
        ]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn changes_from_re_introspection_are_additive_only() -> anyhow::Result<()> {
    let config_dir = TempDir::new().await?;
    schema_from_sampling(
        &config_dir,
        vec![
            doc! {
                "created_at": "2025-07-03T02:31Z",
                "removed_field": true,
                "author": "Alice",
                "nested": {
                    "scalar_type_changed": 1,
                    "removed": 1,
                    "made_nullable": 1,

                },
                "nested_array": [{
                    "scalar_type_changed": 1,
                    "removed": 1,
                    "made_nullable": 1,

                }],
                "nested_nullable": {
                    "scalar_type_changed": 1,
                    "removed": 1,
                    "made_nullable": 1,

                }
            },
            doc! {
                "created_at": "2025-07-03T02:31Z",
                "removed_field": true,
                "author": "Alice",
                "nested": {
                    "scalar_type_changed": 1,
                    "removed": 1,
                    "made_nullable": 1,

                },
                "nested_array": [{
                    "scalar_type_changed": 1,
                    "removed": 1,
                    "made_nullable": 1,

                }],
                "nested_nullable": null,
            },
        ],
    )
    .await?;

    // re-introspect after database changes
    let configuration = schema_from_sampling(
        &config_dir,
        vec![
            doc! {
                "created_at": Bson::DateTime(bson::DateTime::from_millis(1741372252881)),
                "author": "Alice",
                "nested": {
                    "scalar_type_changed": true,
                    "made_nullable": 1,
                },
                "nested_array": [{
                    "scalar_type_changed": true,
                    "made_nullable": 1,

                }],
                "nested_nullable": {
                    "scalar_type_changed": true,
                    "made_nullable": 1,

                }
            },
            doc! {
                "created_at": Bson::DateTime(bson::DateTime::from_millis(1741372252881)),
                "author": null,
                "nested": {
                    "scalar_type_changed": true,
                    "made_nullable": null,
                },
                "nested_array": [{
                    "scalar_type_changed": true,
                    "made_nullable": null,
                }],
                "nested_nullable": null,
            },
        ],
    )
    .await?;

    let updated_type = configuration
        .object_types
        .get("posts")
        .expect("got posts collection type");

    expect_that!(
        updated_type.fields,
        unordered_elements_are![
            (
                displays_as(eq("created_at")),
                field!(ObjectField.r#type, eq(&named_type("String")))
            ),
            (
                displays_as(eq("removed_field")),
                field!(ObjectField.r#type, eq(&named_type("Bool")))
            ),
            (
                displays_as(eq("author")),
                field!(ObjectField.r#type, eq(&named_type("String")))
            ),
            (
                displays_as(eq("nested")),
                field!(ObjectField.r#type, eq(&named_type("posts_nested")))
            ),
            (
                displays_as(eq("nested_array")),
                field!(
                    ObjectField.r#type,
                    eq(&array_of(named_type("posts_nested_array")))
                )
            ),
            (
                displays_as(eq("nested_nullable")),
                field!(
                    ObjectField.r#type,
                    eq(&nullable(named_type("posts_nested_nullable")))
                )
            ),
        ]
    );
    expect_that!(
        configuration.object_types,
        contains_each![
            (
                displays_as(eq("posts_nested")),
                eq(&object_type([
                    ("scalar_type_changed", named_type("Int")),
                    ("removed", named_type("Int")),
                    ("made_nullable", named_type("Int")),
                ]))
            ),
            (
                displays_as(eq("posts_nested_array")),
                eq(&object_type([
                    ("scalar_type_changed", named_type("Int")),
                    ("removed", named_type("Int")),
                    ("made_nullable", named_type("Int")),
                ]))
            ),
            (
                displays_as(eq("posts_nested_nullable")),
                eq(&object_type([
                    ("scalar_type_changed", named_type("Int")),
                    ("removed", named_type("Int")),
                    ("made_nullable", named_type("Int")),
                ]))
            ),
        ]
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

    db.expect_collection().returning(|_collection_name| {
        let mut collection = MockCollectionTrait::new();
        collection
            .expect_aggregate()
            .returning(|_pipeline, _options: Option<AggregateOptions>| Ok(mock_stream(vec![])));
        collection
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

async fn schema_from_sampling(
    config_dir: &Path,
    sampled_documents: Vec<bson::Document>,
) -> anyhow::Result<Configuration> {
    let mut db = MockDatabaseTrait::new();

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
            "options": {},
            "info": { "readOnly": false },
        };
        Ok(mock_stream(vec![Ok(
            from_document(collection_spec).unwrap()
        )]))
    });

    db.expect_collection().returning(move |_collection_name| {
        let mut collection = MockCollectionTrait::new();
        let sample_results = sampled_documents
            .iter()
            .cloned()
            .map(Ok::<_, mongodb::error::Error>)
            .collect_vec();
        collection.expect_aggregate().returning(
            move |_pipeline, _options: Option<AggregateOptions>| {
                Ok(mock_stream(sample_results.clone()))
            },
        );
        collection
    });

    update(&context, &args, &db).await?;

    let configuration = read_directory(config_dir).await?;
    Ok(configuration)
}
