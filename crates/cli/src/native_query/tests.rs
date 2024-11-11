use std::collections::BTreeMap;

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

#[googletest::test]
fn supports_various_query_predicate_operators() -> googletest::Result<()> {
    let config = mflix_config();

    let pipeline = Pipeline::new(vec![Stage::Match(doc! {
        "title": { "$eq": "{{ title }}" },
        "rated": { "$ne": "{{ rating }}" },
        "year": "{{ year_1 }}",
        "imdb.votes": { "$gt": "{{ votes }}" },
        "num_mflix_comments": { "$in": "{{ num_comments_options }}" },
        "$not": { "runtime": { "$lt": "{{ runtime }}" } },
        "tomatoes.critic": { "$exists": "{{ critic_exists }}" },
        "lastUpdated": { "$type": "date" },
        "released": { "$type": ["date", "{{ other_type }}"] },
        "$or": [
            { "$and": [
                { "writers": { "$eq": "{{ writers }}" } },
                { "year": "{{ year_2 }}", }
            ] },
            {
                "year": { "$mod": ["{{ divisor }}", "{{ expected_remainder }}"] },
                "title": { "$regex": "{{ title_regex }}" },
            },
        ],
        "$and": [
            { "genres": { "$all": "{{ genres }}" } },
            { "genres": { "$all": ["{{ genre_1 }}"] } },
            { "genres": { "$elemMatch": {
                "$gt": "{{ genre_start }}",
                "$lt": "{{ genre_end }}",
            }} },
            { "genres": { "$size": "{{ genre_size }}" } },
        ],
    })]);

    let native_query =
        native_query_from_pipeline(&config, "operators_test", Some("movies".into()), pipeline)?;

    expect_eq!(
        native_query.arguments,
        object_fields([
            ("title", Type::Scalar(BsonScalarType::String)),
            ("rating", Type::Scalar(BsonScalarType::String)),
            ("year_1", Type::Scalar(BsonScalarType::Int)),
            ("year_2", Type::Scalar(BsonScalarType::Int)),
            ("votes", Type::Scalar(BsonScalarType::Int)),
            (
                "num_comments_options",
                Type::ArrayOf(Box::new(Type::Scalar(BsonScalarType::Int)))
            ),
            ("runtime", Type::Scalar(BsonScalarType::Int)),
            ("critic_exists", Type::Scalar(BsonScalarType::Bool)),
            ("other_type", Type::Scalar(BsonScalarType::String)),
            (
                "writers",
                Type::ArrayOf(Box::new(Type::Scalar(BsonScalarType::String)))
            ),
            ("divisor", Type::Scalar(BsonScalarType::Int)),
            ("expected_remainder", Type::Scalar(BsonScalarType::Int)),
            ("title_regex", Type::Scalar(BsonScalarType::Regex)),
            (
                "genres",
                Type::ArrayOf(Box::new(Type::Scalar(BsonScalarType::String)))
            ),
            ("genre_1", Type::Scalar(BsonScalarType::String)),
            ("genre_start", Type::Scalar(BsonScalarType::String)),
            ("genre_end", Type::Scalar(BsonScalarType::String)),
            ("genre_size", Type::Scalar(BsonScalarType::Int)),
        ])
    );

    Ok(())
}

#[googletest::test]
fn supports_various_aggregation_operators() -> googletest::Result<()> {
    let config = mflix_config();

    let pipeline = Pipeline::new(vec![
        Stage::Match(doc! {
            "$expr": {
                "$and": [
                    { "$eq": ["{{ title }}", "$title"] },
                    { "$or": [null, 1] },
                    { "$not": "{{ bool_param }}" },
                    { "$gt": ["$imdb.votes", "{{ votes }}"] },
                ]
            }
        }),
        Stage::ReplaceWith(Selection::new(doc! {
            "abs": { "$abs": "$year" },
            "add": { "$add": ["$tomatoes.viewer.rating", "{{ rating_inc }}"] },
            "divide": { "$divide": ["$tomatoes.viewer.rating", "{{ rating_div }}"] },
            "multiply": { "$multiply": ["$tomatoes.viewer.rating", "{{ rating_mult }}"] },
            "subtract": { "$subtract": ["$tomatoes.viewer.rating", "{{ rating_sub }}"] },
            "arrayElemAt": { "$arrayElemAt": ["$genres", "{{ idx }}"] },
            "title_words": { "$split": ["$title", " "] }
        })),
    ]);

    let native_query =
        native_query_from_pipeline(&config, "operators_test", Some("movies".into()), pipeline)?;

    expect_eq!(
        native_query.arguments,
        object_fields([
            ("title", Type::Scalar(BsonScalarType::String)),
            ("bool_param", Type::Scalar(BsonScalarType::Bool)),
            ("votes", Type::Scalar(BsonScalarType::Int)),
            ("rating_inc", Type::Scalar(BsonScalarType::Double)),
            ("rating_div", Type::Scalar(BsonScalarType::Double)),
            ("rating_mult", Type::Scalar(BsonScalarType::Double)),
            ("rating_sub", Type::Scalar(BsonScalarType::Double)),
            ("idx", Type::Scalar(BsonScalarType::Int)),
        ])
    );

    let result_type = native_query.result_document_type;
    expect_eq!(
        native_query.object_types[&result_type],
        ObjectType {
            fields: object_fields([
                ("abs", Type::Scalar(BsonScalarType::Int)),
                ("add", Type::Scalar(BsonScalarType::Double)),
                ("divide", Type::Scalar(BsonScalarType::Double)),
                ("multiply", Type::Scalar(BsonScalarType::Double)),
                ("subtract", Type::Scalar(BsonScalarType::Double)),
                (
                    "arrayElemAt",
                    Type::Nullable(Box::new(Type::Scalar(BsonScalarType::String)))
                ),
                (
                    "title_words",
                    Type::ArrayOf(Box::new(Type::Scalar(BsonScalarType::String)))
                ),
            ]),
            description: None,
        }
    );

    Ok(())
}

fn object_fields<S, K>(types: impl IntoIterator<Item = (S, Type)>) -> BTreeMap<K, ObjectField>
where
    S: Into<K>,
    K: Ord,
{
    types
        .into_iter()
        .map(|(name, r#type)| {
            (
                name.into(),
                ObjectField {
                    r#type,
                    description: None,
                },
            )
        })
        .collect()
}

async fn read_configuration() -> Result<Configuration> {
    read_directory("../../fixtures/hasura/sample_mflix/connector").await
}
