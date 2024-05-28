use std::collections::BTreeMap;

use configuration::{schema, Configuration};
use mongodb_support::BsonScalarType;
use ndc_models::CollectionInfo;
use ndc_test_helpers::{collection, make_primary_key_uniqueness_constraint, object_type};

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
                    foreign_keys: Default::default(),
                },
            ),
            collection("appearances"), // new helper gives more concise syntax
        ]),
        functions: Default::default(),
        object_types: BTreeMap::from([
            (
                "Author".to_owned(),
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
                "appearances".to_owned(),
                object_type([("authorId", schema::Type::Scalar(BsonScalarType::ObjectId))]),
            ),
        ]),
        procedures: Default::default(),
        native_mutations: Default::default(),
        native_queries: Default::default(),
        options: Default::default(),
    })
}
