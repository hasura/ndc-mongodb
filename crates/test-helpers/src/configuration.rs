use configuration::Configuration;
use ndc_test_helpers::{collection, named_type, object_type};

/// Configuration for a MongoDB database that resembles MongoDB's sample_mflix test data set.
pub fn mflix_config() -> Configuration {
    Configuration {
        collections: [collection("comments"), collection("movies")].into(),
        object_types: [
            (
                "comments".into(),
                object_type([
                    ("_id", named_type("ObjectId")),
                    ("movie_id", named_type("ObjectId")),
                    ("name", named_type("String")),
                ]),
            ),
            (
                "credits".into(),
                object_type([("director", named_type("String"))]),
            ),
            (
                "movies".into(),
                object_type([
                    ("_id", named_type("ObjectId")),
                    ("credits", named_type("credits")),
                    ("title", named_type("String")),
                    ("year", named_type("Int")),
                ]),
            ),
        ]
        .into(),
        functions: Default::default(),
        procedures: Default::default(),
        native_mutations: Default::default(),
        native_queries: Default::default(),
        options: Default::default(),
    }
}
