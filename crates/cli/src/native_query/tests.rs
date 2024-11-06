use anyhow::Result;
use configuration::{
    native_query::NativeQueryRepresentation::Collection,
    read_directory,
    schema::{ObjectField, ObjectType, Type},
    serialized::NativeQuery,
    Configuration,
};
use googletest::prelude::*;
use mongodb::bson::doc;
use mongodb_support::{
    aggregate::{Accumulator, Pipeline, Selection, Stage},
    BsonScalarType,
};
use ndc_models::ObjectTypeName;
use pretty_assertions::assert_eq;
use test_helpers::configuration::mflix_config;

use super::native_query_from_pipeline;

#[tokio::test]
async fn infers_native_query_from_pipeline() -> Result<()> {
    let config = read_configuration().await?;
    let pipeline = Pipeline::new(vec![Stage::Documents(vec![
        doc! { "foo": 1 },
        doc! { "bar": 2 },
    ])]);
    let native_query = native_query_from_pipeline(
        &config,
        "selected_title",
        Some("movies".into()),
        pipeline.clone(),
    )?;

    let expected_document_type_name: ObjectTypeName = "selected_title_documents".into();

    let expected_object_types = [(
        expected_document_type_name.clone(),
        ObjectType {
            fields: [
                (
                    "foo".into(),
                    ObjectField {
                        r#type: Type::Nullable(Box::new(Type::Scalar(BsonScalarType::Int))),
                        description: None,
                    },
                ),
                (
                    "bar".into(),
                    ObjectField {
                        r#type: Type::Nullable(Box::new(Type::Scalar(BsonScalarType::Int))),
                        description: None,
                    },
                ),
            ]
            .into(),
            description: None,
        },
    )]
    .into();

    let expected = NativeQuery {
        representation: Collection,
        input_collection: Some("movies".into()),
        arguments: Default::default(),
        result_document_type: expected_document_type_name,
        object_types: expected_object_types,
        pipeline: pipeline.into(),
        description: None,
    };

    assert_eq!(native_query, expected);
    Ok(())
}

#[tokio::test]
async fn infers_native_query_from_non_trivial_pipeline() -> Result<()> {
    let config = read_configuration().await?;
    let pipeline = Pipeline::new(vec![
        Stage::ReplaceWith(Selection::new(doc! {
            "title": "$title",
            "title_words": { "$split": ["$title", " "] }
        })),
        Stage::Unwind {
            path: "$title_words".to_string(),
            include_array_index: None,
            preserve_null_and_empty_arrays: None,
        },
        Stage::Group {
            key_expression: "$title_words".into(),
            accumulators: [("title_count".into(), Accumulator::Count)].into(),
        },
    ]);
    let native_query = native_query_from_pipeline(
        &config,
        "title_word_frequency",
        Some("movies".into()),
        pipeline.clone(),
    )?;

    assert_eq!(native_query.input_collection, Some("movies".into()));
    assert!(native_query
        .result_document_type
        .to_string()
        .starts_with("title_word_frequency"));
    assert_eq!(
        native_query
            .object_types
            .get(&native_query.result_document_type),
        Some(&ObjectType {
            fields: [
                (
                    "_id".into(),
                    ObjectField {
                        r#type: Type::Scalar(BsonScalarType::String),
                        description: None,
                    },
                ),
                (
                    "title_count".into(),
                    ObjectField {
                        r#type: Type::Scalar(BsonScalarType::Int),
                        description: None,
                    },
                ),
            ]
            .into(),
            description: None,
        })
    );
    Ok(())
}

#[googletest::test]
fn infers_native_query_from_pipeline_with_unannotated_parameter() -> googletest::Result<()> {
    let config = mflix_config();

    let pipeline = Pipeline::new(vec![Stage::Match(doc! {
        "title": { "$eq": "{{ title }}" },
    })]);

    let native_query =
        native_query_from_pipeline(&config, "movies_by_title", Some("movies".into()), pipeline)?;

    expect_that!(
        native_query.arguments,
        unordered_elements_are![(
            displays_as(eq("title")),
            field!(
                ObjectField.r#type,
                eq(&Type::Scalar(BsonScalarType::String))
            )
        )]
    );
    Ok(())
}

#[googletest::test]
fn infers_parameter_type_from_binary_comparison() -> googletest::Result<()> {
    let config = mflix_config();

    let pipeline = Pipeline::new(vec![Stage::Match(doc! {
        "$expr": { "$eq": ["{{ title }}", "$title"] }
    })]);

    let native_query =
        native_query_from_pipeline(&config, "movies_by_title", Some("movies".into()), pipeline)?;

    expect_that!(
        native_query.arguments,
        unordered_elements_are![(
            displays_as(eq("title")),
            field!(
                ObjectField.r#type,
                eq(&Type::Scalar(BsonScalarType::String))
            )
        )]
    );
    Ok(())
}

async fn read_configuration() -> Result<Configuration> {
    read_directory("../../fixtures/hasura/sample_mflix/connector").await
}
