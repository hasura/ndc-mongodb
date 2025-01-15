use futures_util::stream::{iter, Iter};
use mongodb::{
    bson::{to_bson, Bson},
    error::Error,
    options::AggregateOptions,
};
use pretty_assertions::assert_eq;

use super::{MockCollectionTrait, MockDatabaseTrait};

// In MockCollectionTrait and MockDatabaseTrait the cursor types are implemented using `Iter` which
// is a struct that wraps around and iterator, and implements `Stream` (and by extension implements
// `TryStreamExt`). I didn't know how to allow any Iterator type here, so I specified the type that
// is produced when calling `into_iter` on a `Vec`. - Jesse H.
//
// To produce a mock stream use the `mock_stream` function in this module.
pub type MockCursor<T> = futures::stream::Iter<<Vec<Result<T, Error>> as IntoIterator>::IntoIter>;

/// Create a stream that can be returned from mock implementations for
/// CollectionTrait::aggregate or CollectionTrait::find.
pub fn mock_stream<T>(
    items: Vec<Result<T, Error>>,
) -> Iter<<Vec<Result<T, Error>> as IntoIterator>::IntoIter> {
    iter(items)
}

/// Mocks the result of an aggregate call on a given collection.
pub fn mock_collection_aggregate_response(
    collection: impl ToString,
    result: Bson,
) -> MockDatabaseTrait {
    let collection_name = collection.to_string();

    let mut db = MockDatabaseTrait::new();
    db.expect_collection().returning(move |name| {
        assert_eq!(
            name, collection_name,
            "unexpected target for mock aggregate"
        );

        // Make some clones to work around ownership issues. These closures are `FnMut`, not
        // `FnOnce` so the type checker can't just move ownership into the closure.
        let per_colection_result = result.clone();

        let mut mock_collection = MockCollectionTrait::new();
        mock_collection.expect_aggregate().returning(
            move |_pipeline, _: Option<AggregateOptions>| {
                let result_docs = {
                    let items = match per_colection_result.clone() {
                        Bson::Array(xs) => xs,
                        _ => panic!("mock pipeline result should be an array of documents"),
                    };
                    items
                        .into_iter()
                        .map(|x| match x {
                            Bson::Document(doc) => Ok(doc),
                            _ => panic!("mock pipeline result should be an array of documents"),
                        })
                        .collect()
                };
                Ok(mock_stream(result_docs))
            },
        );
        mock_collection
    });
    db
}

/// Mocks the result of an aggregate call on a given collection. Asserts that the pipeline that the
/// aggregate call receives matches the given pipeline.
pub fn mock_collection_aggregate_response_for_pipeline(
    collection: impl ToString,
    expected_pipeline: Bson,
    result: Bson,
) -> MockDatabaseTrait {
    let collection_name = collection.to_string();

    let mut db = MockDatabaseTrait::new();
    db.expect_collection().returning(move |name| {
        assert_eq!(
            name, collection_name,
            "unexpected target for mock aggregate"
        );

        // Make some clones to work around ownership issues. These closures are `FnMut`, not
        // `FnOnce` so the type checker can't just move ownership into the closure.
        let per_collection_pipeline = expected_pipeline.clone();
        let per_colection_result = result.clone();

        let mut mock_collection = MockCollectionTrait::new();
        mock_collection.expect_aggregate().returning(
            move |pipeline, _: Option<AggregateOptions>| {
                assert_eq!(
                    to_bson(&pipeline).unwrap(),
                    per_collection_pipeline,
                    "actual pipeline (left) did not match expected (right)"
                );
                let result_docs = {
                    let items = match per_colection_result.clone() {
                        Bson::Array(xs) => xs,
                        _ => panic!("mock pipeline result should be an array of documents"),
                    };
                    items
                        .into_iter()
                        .map(|x| match x {
                            Bson::Document(doc) => Ok(doc),
                            _ => panic!("mock pipeline result should be an array of documents"),
                        })
                        .collect()
                };
                Ok(mock_stream(result_docs))
            },
        );
        mock_collection
    });
    db
}

/// Mocks the result of an aggregate call without a specified collection. Asserts that the pipeline
/// that the aggregate call receives matches the given pipeline.
pub fn mock_aggregate_response_for_pipeline(
    expected_pipeline: Bson,
    result: Bson,
) -> MockDatabaseTrait {
    let mut db = MockDatabaseTrait::new();
    db.expect_aggregate()
        .returning(move |pipeline, _: Option<AggregateOptions>| {
            assert_eq!(
                to_bson(&pipeline).unwrap(),
                expected_pipeline,
                "actual pipeline (left) did not match expected (right)"
            );
            let result_docs = {
                let items = match result.clone() {
                    Bson::Array(xs) => xs,
                    _ => panic!("mock pipeline result should be an array of documents"),
                };
                items
                    .into_iter()
                    .map(|x| match x {
                        Bson::Document(doc) => Ok(doc),
                        _ => panic!("mock pipeline result should be an array of documents"),
                    })
                    .collect()
            };
            Ok(mock_stream(result_docs))
        });
    db
}
