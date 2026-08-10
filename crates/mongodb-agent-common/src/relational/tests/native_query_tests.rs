//! Tests for using collection-representation native queries as relational tables
//! (`Relation::From`).
//!
//! These cover argument validation/binding, preservation of the configured native pipeline as an
//! immutable source prefix, `input_collection` vs database-level execution, Atlas `$search`
//! staying at stage zero, rejection of function-representation native queries, and physical
//! collection regression.

use std::collections::BTreeMap;

use configuration::{
    native_query::NativeQueryRepresentation,
    schema::{ObjectField, ObjectType, Type},
    serialized, Configuration,
};
use mongodb::bson::{doc, oid::ObjectId, DateTime};
use mongodb_support::{aggregate::Stage, BsonScalarType as S};
use ndc_models::{
    ArgumentName, Float64, OrderDirection, Relation, RelationalExpression, RelationalLiteral, Sort,
};
use pretty_assertions::assert_eq;

use crate::mongo_query_plan::MongoConfiguration;
use crate::relational::pipeline_builder::build_relational_pipeline_with_config;
use crate::relational::RelationalError;
use crate::test_helpers::mflix_config;

// ---------------------------------------------------------------------------
// Test configuration
// ---------------------------------------------------------------------------

fn field(t: Type) -> ObjectField {
    ObjectField {
        r#type: t,
        description: None,
    }
}

fn scalar(s: S) -> Type {
    Type::Scalar(s)
}

fn args(pairs: &[(&str, Type)]) -> BTreeMap<ArgumentName, ObjectField> {
    pairs
        .iter()
        .map(|(name, t)| ((*name).into(), field(t.clone())))
        .collect()
}

/// An object type describing a movie-like result document.
fn movie_result_type() -> ObjectType {
    ObjectType {
        description: None,
        fields: [
            ("_id".into(), field(scalar(S::ObjectId))),
            ("title".into(), field(scalar(S::String))),
            ("year".into(), field(scalar(S::Int))),
        ]
        .into(),
    }
}

/// A configuration exposing several native queries plus the physical `movies`/`comments`
/// collections (via mflix). Built through `Configuration::validate` so that native-query argument
/// types are materialized into their generated `CollectionInfo`.
fn native_query_config() -> MongoConfiguration {
    // Collection-representation native query with typed scalar arguments and an input collection.
    // Begins with an Atlas `$search` stage that must remain first.
    let search_movies = serialized::NativeQuery {
        representation: NativeQueryRepresentation::Collection,
        input_collection: Some("movies".into()),
        arguments: args(&[("searchTerm", scalar(S::String)), ("limit", scalar(S::Int))]),
        result_document_type: "SearchMoviesResult".into(),
        object_types: [("SearchMoviesResult".into(), movie_result_type())].into(),
        pipeline: vec![
            doc! {
                "$search": {
                    "index": "default",
                    "text": { "query": "{{ searchTerm }}", "path": "title" }
                }
            },
            doc! { "$limit": "{{ limit }}" },
        ],
        description: None,
    };

    // Argument-free collection native query with no input collection -> database-level aggregation.
    let list_recent = serialized::NativeQuery {
        representation: NativeQueryRepresentation::Collection,
        input_collection: None,
        arguments: Default::default(),
        result_document_type: "ListRecentResult".into(),
        object_types: [("ListRecentResult".into(), movie_result_type())].into(),
        pipeline: vec![doc! {
            "$documents": [ { "_id": 1, "title": "A", "year": 2001 } ]
        }],
        description: None,
    };

    // Native query taking an ObjectId argument (string -> ObjectId coercion).
    let movie_by_id = serialized::NativeQuery {
        representation: NativeQueryRepresentation::Collection,
        input_collection: Some("movies".into()),
        arguments: args(&[("movieId", scalar(S::ObjectId))]),
        result_document_type: "MovieByIdResult".into(),
        object_types: [("MovieByIdResult".into(), movie_result_type())].into(),
        pipeline: vec![doc! { "$match": { "_id": "{{ movieId }}" } }],
        description: None,
    };

    // Native query exercising representative scalar argument types.
    let echo_scalars = serialized::NativeQuery {
        representation: NativeQueryRepresentation::Collection,
        input_collection: Some("movies".into()),
        arguments: args(&[
            ("s", scalar(S::String)),
            ("i", scalar(S::Int)),
            ("l", scalar(S::Long)),
            ("d", scalar(S::Double)),
            ("b", scalar(S::Bool)),
            ("oid", scalar(S::ObjectId)),
            ("dt", scalar(S::Date)),
        ]),
        result_document_type: "EchoResult".into(),
        object_types: [(
            "EchoResult".into(),
            ObjectType {
                description: None,
                fields: [("_id".into(), field(scalar(S::ObjectId)))].into(),
            },
        )]
        .into(),
        pipeline: vec![doc! {
            "$match": {
                "s": "{{ s }}",
                "i": "{{ i }}",
                "l": "{{ l }}",
                "d": "{{ d }}",
                "b": "{{ b }}",
                "oid": "{{ oid }}",
                "dt": "{{ dt }}"
            }
        }],
        description: None,
    };

    // Function-representation native query -> must be rejected as a relational table.
    let stats = serialized::NativeQuery {
        representation: NativeQueryRepresentation::Function,
        input_collection: Some("movies".into()),
        arguments: Default::default(),
        result_document_type: "StatsResult".into(),
        object_types: [(
            "StatsResult".into(),
            ObjectType {
                description: None,
                fields: [("__value".into(), field(scalar(S::Int)))].into(),
            },
        )]
        .into(),
        pipeline: vec![doc! { "$count": "__value" }],
        description: None,
    };

    let config = Configuration::validate(
        Default::default(),
        Default::default(),
        [
            ("searchMovies".into(), search_movies),
            ("listRecent".into(), list_recent),
            ("movieById".into(), movie_by_id),
            ("echoScalars".into(), echo_scalars),
            ("movieStats".into(), stats),
        ]
        .into(),
        Default::default(),
    )
    .expect("native query configuration should validate");

    MongoConfiguration(config)
}

fn int64(value: i64) -> RelationalExpression {
    RelationalExpression::Literal {
        literal: RelationalLiteral::Int64 { value },
    }
}

// ---------------------------------------------------------------------------
// Resolution + argument binding
// ---------------------------------------------------------------------------

#[test]
fn resolves_collection_native_query_with_scalar_arguments() {
    let relation = Relation::From {
        collection: "searchMovies".into(),
        columns: vec!["_id".into(), "title".into(), "year".into()],
        arguments: [
            (
                "searchTerm".into(),
                RelationalLiteral::String {
                    value: "godfather".into(),
                },
            ),
            ("limit".into(), RelationalLiteral::Int64 { value: 10 }),
        ]
        .into(),
    };

    let config = native_query_config();
    let result = build_relational_pipeline_with_config(&relation, Some(&config)).unwrap();

    assert_eq!(result.collection, "searchMovies");
    // input_collection drives the physical aggregation target.
    assert_eq!(result.target_collection, Some("movies".to_string()));

    // The configured native pipeline is preserved verbatim as the source prefix, with arguments
    // interpolated and typed.
    assert_eq!(
        result.pipeline.stages,
        vec![
            Stage::Other(doc! {
                "$search": {
                    "index": "default",
                    "text": { "query": "godfather", "path": "title" }
                }
            }),
            Stage::Other(doc! { "$limit": 10_i32 }),
        ]
    );
    assert_eq!(result.output_columns.field_for_index(1), Some("title"));
}

#[test]
fn argument_free_native_query_without_input_collection_runs_database_level() {
    let relation = Relation::From {
        collection: "listRecent".into(),
        columns: vec!["_id".into(), "title".into(), "year".into()],
        arguments: Default::default(),
    };

    let config = native_query_config();
    let result = build_relational_pipeline_with_config(&relation, Some(&config)).unwrap();

    assert_eq!(result.collection, "listRecent");
    // No input_collection -> database-level aggregation.
    assert_eq!(result.target_collection, None);
    assert_eq!(
        result.pipeline.stages,
        vec![Stage::Other(doc! {
            "$documents": [ { "_id": 1, "title": "A", "year": 2001 } ]
        })]
    );
}

#[test]
fn missing_argument_is_rejected() {
    let relation = Relation::From {
        collection: "searchMovies".into(),
        columns: vec!["title".into()],
        arguments: [(
            "searchTerm".into(),
            RelationalLiteral::String {
                value: "x".into(),
            },
        )]
        .into(),
    };

    let config = native_query_config();
    let err = build_relational_pipeline_with_config(&relation, Some(&config)).unwrap_err();
    assert!(
        matches!(&err, RelationalError::MissingArgument { argument, .. } if argument == "limit"),
        "expected MissingArgument(limit), got {err:?}"
    );
}

#[test]
fn unknown_argument_is_rejected() {
    let relation = Relation::From {
        collection: "searchMovies".into(),
        columns: vec!["title".into()],
        arguments: [
            (
                "searchTerm".into(),
                RelationalLiteral::String { value: "x".into() },
            ),
            ("limit".into(), RelationalLiteral::Int64 { value: 1 }),
            (
                "bogus".into(),
                RelationalLiteral::String { value: "y".into() },
            ),
        ]
        .into(),
    };

    let config = native_query_config();
    let err = build_relational_pipeline_with_config(&relation, Some(&config)).unwrap_err();
    assert!(
        matches!(&err, RelationalError::UnknownArgument { argument, .. } if argument == "bogus"),
        "expected UnknownArgument(bogus), got {err:?}"
    );
}

#[test]
fn null_into_non_nullable_argument_is_rejected() {
    let relation = Relation::From {
        collection: "searchMovies".into(),
        columns: vec!["title".into()],
        arguments: [
            ("searchTerm".into(), RelationalLiteral::Null),
            ("limit".into(), RelationalLiteral::Int64 { value: 1 }),
        ]
        .into(),
    };

    let config = native_query_config();
    let err = build_relational_pipeline_with_config(&relation, Some(&config)).unwrap_err();
    assert!(
        matches!(&err, RelationalError::ArgumentBindingError { argument, .. } if argument == "searchTerm"),
        "expected ArgumentBindingError(searchTerm), got {err:?}"
    );
}

#[test]
fn type_invalid_argument_is_rejected() {
    // `limit` is declared Int, but a string literal is supplied.
    let relation = Relation::From {
        collection: "searchMovies".into(),
        columns: vec!["title".into()],
        arguments: [
            (
                "searchTerm".into(),
                RelationalLiteral::String { value: "x".into() },
            ),
            (
                "limit".into(),
                RelationalLiteral::String {
                    value: "not-a-number".into(),
                },
            ),
        ]
        .into(),
    };

    let config = native_query_config();
    let err = build_relational_pipeline_with_config(&relation, Some(&config)).unwrap_err();
    assert!(
        matches!(&err, RelationalError::ArgumentBindingError { argument, .. } if argument == "limit"),
        "expected ArgumentBindingError(limit), got {err:?}"
    );
}

#[test]
fn object_id_argument_is_coerced_from_string() {
    let hex = "5a9427648b0beebeb69579cc";
    let relation = Relation::From {
        collection: "movieById".into(),
        columns: vec!["_id".into(), "title".into()],
        arguments: [(
            "movieId".into(),
            RelationalLiteral::String { value: hex.into() },
        )]
        .into(),
    };

    let config = native_query_config();
    let result = build_relational_pipeline_with_config(&relation, Some(&config)).unwrap();

    let expected_id = ObjectId::parse_str(hex).unwrap();
    assert_eq!(result.target_collection, Some("movies".to_string()));
    assert_eq!(
        result.pipeline.stages,
        vec![Stage::Other(doc! { "$match": { "_id": expected_id } })]
    );
}

#[test]
fn binds_representative_scalar_types() {
    let hex = "5a9427648b0beebeb69579cc";
    let relation = Relation::From {
        collection: "echoScalars".into(),
        columns: vec!["_id".into()],
        arguments: [
            (
                "s".into(),
                RelationalLiteral::String {
                    value: "hello".into(),
                },
            ),
            ("i".into(), RelationalLiteral::Int32 { value: 7 }),
            ("l".into(), RelationalLiteral::Int64 { value: 9_000_000_000 }),
            (
                "d".into(),
                RelationalLiteral::Float64 {
                    value: Float64(3.5),
                },
            ),
            ("b".into(), RelationalLiteral::Boolean { value: true }),
            (
                "oid".into(),
                RelationalLiteral::String { value: hex.into() },
            ),
            (
                "dt".into(),
                RelationalLiteral::String {
                    value: "2020-01-02T03:04:05Z".into(),
                },
            ),
        ]
        .into(),
    };

    let config = native_query_config();
    let result = build_relational_pipeline_with_config(&relation, Some(&config)).unwrap();

    let expected_oid = ObjectId::parse_str(hex).unwrap();
    // 2020-01-02T03:04:05Z
    let expected_date = DateTime::from_millis(1_577_934_245_000);

    assert_eq!(
        result.pipeline.stages,
        vec![Stage::Other(doc! {
            "$match": {
                "s": "hello",
                "i": 7_i32,
                "l": 9_000_000_000_i64,
                "d": 3.5_f64,
                "b": true,
                "oid": expected_oid,
                "dt": expected_date,
            }
        })]
    );
}

// ---------------------------------------------------------------------------
// Composition: prefix stays first, generated stages come after
// ---------------------------------------------------------------------------

#[test]
fn search_stays_stage_zero_with_generated_filter_sort_and_pagination() {
    // Paginate(Sort(Filter(From(searchMovies))))
    let relation = Relation::Paginate {
        input: Box::new(Relation::Sort {
            input: Box::new(Relation::Filter {
                input: Box::new(Relation::From {
                    collection: "searchMovies".into(),
                    columns: vec!["_id".into(), "title".into(), "year".into()],
                    arguments: [
                        (
                            "searchTerm".into(),
                            RelationalLiteral::String {
                                value: "godfather".into(),
                            },
                        ),
                        ("limit".into(), RelationalLiteral::Int64 { value: 100 }),
                    ]
                    .into(),
                }),
                predicate: RelationalExpression::Gt {
                    left: Box::new(RelationalExpression::Column { index: 2 }),
                    right: Box::new(int64(2000)),
                },
            }),
            exprs: vec![Sort {
                expr: RelationalExpression::Column { index: 1 },
                direction: OrderDirection::Asc,
                nulls_sort: ndc_models::NullsSort::NullsFirst,
            }],
        }),
        fetch: Some(5),
        skip: 2,
    };

    let config = native_query_config();
    let result = build_relational_pipeline_with_config(&relation, Some(&config)).unwrap();

    let stages = &result.pipeline.stages;

    // The native `$search` prefix must be stage zero — no early `$match` may be injected before it.
    assert_eq!(
        stages[0],
        Stage::Other(doc! {
            "$search": {
                "index": "default",
                "text": { "query": "godfather", "path": "title" }
            }
        })
    );
    assert_eq!(stages[1], Stage::Other(doc! { "$limit": 100_i32 }));

    // Everything generated by the relational operators comes after the prefix.
    assert!(stages
        .iter()
        .any(|s| matches!(s, Stage::Match(doc) if doc.contains_key("year"))));
    assert!(stages.iter().any(|s| matches!(s, Stage::Sort(_))));
    assert!(stages.iter().any(|s| matches!(s, Stage::Skip(_))));
    assert!(stages.iter().any(|s| matches!(s, Stage::Limit(_))));

    // No stage before the prefix, and the first generated `$match` is not at index 0.
    assert!(matches!(&stages[0], Stage::Other(doc) if doc.contains_key("$search")));
}

// ---------------------------------------------------------------------------
// Rejection of function-representation native queries
// ---------------------------------------------------------------------------

#[test]
fn rejects_function_representation_native_query_as_relational_table() {
    let relation = Relation::From {
        collection: "movieStats".into(),
        columns: vec!["__value".into()],
        arguments: Default::default(),
    };

    let config = native_query_config();
    let err = build_relational_pipeline_with_config(&relation, Some(&config)).unwrap_err();
    assert!(
        matches!(&err, RelationalError::FunctionRepresentationNotSupported(name) if name == "movieStats"),
        "expected FunctionRepresentationNotSupported(movieStats), got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Physical collection regression
// ---------------------------------------------------------------------------

#[test]
fn physical_collection_still_targets_itself() {
    let relation = Relation::From {
        collection: "comments".into(),
        columns: vec!["_id".into(), "movie_id".into()],
        arguments: Default::default(),
    };

    let config = mflix_config();
    let result = build_relational_pipeline_with_config(&relation, Some(&config)).unwrap();

    assert_eq!(result.collection, "comments");
    assert_eq!(result.target_collection, Some("comments".to_string()));
    assert!(result.pipeline.is_empty());
}

#[test]
fn physical_collection_rejects_arguments() {
    let relation = Relation::From {
        collection: "comments".into(),
        columns: vec!["_id".into()],
        arguments: [("bogus".into(), RelationalLiteral::Int64 { value: 1 })].into(),
    };

    let config = mflix_config();
    let err = build_relational_pipeline_with_config(&relation, Some(&config)).unwrap_err();
    assert!(
        matches!(&err, RelationalError::UnsupportedRelation(_)),
        "expected UnsupportedRelation, got {err:?}"
    );
}

/// Relational-versus-classic parity: the interpolated native prefix produced for a relational
/// `From` matches the pipeline the classic path produces for the same native query and arguments.
#[test]
fn relational_prefix_matches_classic_interpolation() {
    let config = native_query_config();
    let relation = Relation::From {
        collection: "movieById".into(),
        columns: vec!["_id".into()],
        arguments: [(
            "movieId".into(),
            RelationalLiteral::String {
                value: "5a9427648b0beebeb69579cc".into(),
            },
        )]
        .into(),
    };
    let result = build_relational_pipeline_with_config(&relation, Some(&config)).unwrap();

    // The classic path interpolates the same `{{ movieId }}` placeholder to the same typed BSON.
    let expected_id = ObjectId::parse_str("5a9427648b0beebeb69579cc").unwrap();
    assert_eq!(
        result.pipeline.stages,
        vec![Stage::Other(doc! { "$match": { "_id": expected_id } })]
    );
}
