use std::collections::BTreeMap;

use configuration::{schema, Configuration};
use mongodb_support::BsonScalarType;
use ndc_models::CollectionInfo;
use ndc_test_helpers::{
    collection, make_primary_key_uniqueness_constraint, named_type, object_type,
};

use crate::mongo_query_plan::MongoConfiguration;

pub fn make_nested_schema() -> MongoConfiguration {
    MongoConfiguration(Configuration {
        collections: BTreeMap::from([
            (
                "authors".into(),
                CollectionInfo {
                    name: "authors".into(),
                    description: None,
                    collection_type: "Author".into(),
                    arguments: Default::default(),
                    uniqueness_constraints: make_primary_key_uniqueness_constraint("authors"),
                },
            ),
            collection("appearances"), // new helper gives more concise syntax
        ]),
        functions: Default::default(),
        object_types: BTreeMap::from([
            (
                "Author".into(),
                object_type([
                    ("name", schema::Type::Scalar(BsonScalarType::String)),
                    ("address", schema::Type::Object("Address".into())),
                    (
                        "articles",
                        schema::Type::ArrayOf(Box::new(schema::Type::Object("Article".into()))),
                    ),
                    (
                        "array_of_arrays",
                        schema::Type::ArrayOf(Box::new(schema::Type::ArrayOf(Box::new(
                            schema::Type::Object("Article".into()),
                        )))),
                    ),
                ]),
            ),
            (
                "Address".into(),
                object_type([
                    ("country", schema::Type::Scalar(BsonScalarType::String)),
                    ("street", schema::Type::Scalar(BsonScalarType::String)),
                    (
                        "apartment",
                        schema::Type::Nullable(Box::new(schema::Type::Scalar(
                            BsonScalarType::String,
                        ))),
                    ),
                    (
                        "geocode",
                        schema::Type::Nullable(Box::new(schema::Type::Object(
                            "Geocode".to_owned(),
                        ))),
                    ),
                ]),
            ),
            (
                "Article".into(),
                object_type([("title", schema::Type::Scalar(BsonScalarType::String))]),
            ),
            (
                "Geocode".into(),
                object_type([
                    ("latitude", schema::Type::Scalar(BsonScalarType::Double)),
                    ("longitude", schema::Type::Scalar(BsonScalarType::Double)),
                ]),
            ),
            (
                "appearances".into(),
                object_type([("authorId", schema::Type::Scalar(BsonScalarType::ObjectId))]),
            ),
        ]),
        procedures: Default::default(),
        native_mutations: Default::default(),
        native_queries: Default::default(),
        options: Default::default(),
    })
}

/// Configuration for a MongoDB database with Chinook test data
#[allow(dead_code)]
pub fn chinook_config() -> MongoConfiguration {
    MongoConfiguration(Configuration {
        collections: [
            collection("Album"),
            collection("Artist"),
            collection("Genre"),
            collection("Track"),
        ]
        .into(),
        object_types: [
            (
                "Album".into(),
                object_type([
                    ("AlbumId", named_type("Int")),
                    ("ArtistId", named_type("Int")),
                    ("Title", named_type("String")),
                ]),
            ),
            (
                "Artist".into(),
                object_type([
                    ("ArtistId", named_type("Int")),
                    ("Name", named_type("String")),
                ]),
            ),
            (
                "Genre".into(),
                object_type([
                    ("GenreId", named_type("Int")),
                    ("Name", named_type("String")),
                ]),
            ),
            (
                "Track".into(),
                object_type([
                    ("AlbumId", named_type("Int")),
                    ("GenreId", named_type("Int")),
                    ("TrackId", named_type("Int")),
                    ("Name", named_type("String")),
                    ("Milliseconds", named_type("Int")),
                ]),
            ),
        ]
        .into(),
        functions: Default::default(),
        procedures: Default::default(),
        native_mutations: Default::default(),
        native_queries: Default::default(),
        options: Default::default(),
    })
}

pub fn chinook_relationships() -> BTreeMap<String, ndc_models::Relationship> {
    [
        (
            "Albums",
            ndc_test_helpers::relationship("Album", [("ArtistId", &["ArtistId"])]),
        ),
        (
            "Tracks",
            ndc_test_helpers::relationship("Track", [("AlbumId", &["AlbumId"])]),
        ),
        (
            "Genre",
            ndc_test_helpers::relationship("Genre", [("GenreId", &["GenreId"])]).object_type(),
        ),
    ]
    .into_iter()
    .map(|(name, relationship_builder)| (name.to_string(), relationship_builder.into()))
    .collect()
}

/// Configuration for a MongoDB database that resembles MongoDB's sample_mflix test data set.
pub fn mflix_config() -> MongoConfiguration {
    MongoConfiguration(test_helpers::configuration::mflix_config())
}
