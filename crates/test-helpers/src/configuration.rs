use configuration::Configuration;
use ndc_test_helpers::{array_of, collection, named_type, object_type};

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
                    ("genres", array_of(named_type("String"))),
                    ("imdb", named_type("Imdb")),
                    ("lastUpdated", named_type("String")),
                    ("num_mflix_comments", named_type("Int")),
                    ("rated", named_type("String")),
                    ("released", named_type("Date")),
                    ("runtime", named_type("Int")),
                    ("title", named_type("String")),
                    ("writers", array_of(named_type("String"))),
                    ("year", named_type("Int")),
                    ("tomatoes", named_type("Tomatoes")),
                ]),
            ),
            (
                "Imdb".into(),
                object_type([
                    ("rating", named_type("Double")),
                    ("votes", named_type("Int")),
                    ("id", named_type("Int")),
                ]),
            ),
            (
                "Tomatoes".into(),
                object_type([
                    ("critic", named_type("TomatoesCriticViewer")),
                    ("viewer", named_type("TomatoesCriticViewer")),
                    ("lastUpdated", named_type("Date")),
                ]),
            ),
            (
                "TomatoesCriticViewer".into(),
                object_type([
                    ("rating", named_type("Double")),
                    ("numReviews", named_type("Int")),
                    ("meter", named_type("Int")),
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
