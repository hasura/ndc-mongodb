use async_trait::async_trait;
use futures_util::Stream;
use mongodb::results::CollectionSpecification;
use mongodb::{bson::Document, error::Error, options::AggregateOptions, Database};
use mongodb_support::aggregate::Pipeline;

#[cfg(any(test, feature = "test-helpers"))]
use mockall::automock;

use super::CollectionTrait;

#[cfg(any(test, feature = "test-helpers"))]
use super::MockCollectionTrait;

#[cfg(any(test, feature = "test-helpers"))]
use super::test_helpers::MockCursor;

/// Abstract MongoDB database methods. This lets us mock a database connection in tests. The
/// automock attribute generates a struct called MockDatabaseTrait that implements this trait. The
/// mock provides a variety of methods for mocking and spying on database behavior in tests. See
/// https://docs.rs/mockall/latest/mockall/
///
/// I haven't figured out how to make generic associated types work with automock, so  the type
/// argument for `Collection` values produced via `DatabaseTrait::collection` is fixed to to
/// `Document`. That's the way we're using collections in this app anyway.
#[cfg_attr(any(test, feature = "test-helpers"), automock(
    type Collection = MockCollectionTrait<Document>;
    type CollectionCursor = MockCursor<CollectionSpecification>;
    type DocumentCursor = MockCursor<Document>;
))]
#[async_trait]
pub trait DatabaseTrait {
    type Collection: CollectionTrait<Document>;
    type CollectionCursor: Stream<Item = Result<CollectionSpecification, Error>> + Unpin;
    type DocumentCursor: Stream<Item = Result<Document, Error>> + Unpin;

    async fn aggregate<Options>(
        &self,
        pipeline: Pipeline,
        options: Options,
    ) -> Result<Self::DocumentCursor, Error>
    where
        Options: Into<Option<AggregateOptions>> + Send + 'static;

    fn collection(&self, name: &str) -> Self::Collection;

    async fn list_collections(&self) -> Result<Self::CollectionCursor, Error>;
}

#[async_trait]
impl DatabaseTrait for Database {
    type Collection = mongodb::Collection<Document>;
    type CollectionCursor = mongodb::Cursor<CollectionSpecification>;
    type DocumentCursor = mongodb::Cursor<Document>;

    async fn aggregate<Options>(
        &self,
        pipeline: Pipeline,
        options: Options,
    ) -> Result<Self::DocumentCursor, Error>
    where
        Options: Into<Option<AggregateOptions>> + Send + 'static,
    {
        Database::aggregate(self, pipeline)
            .with_options(options)
            .await
    }

    fn collection(&self, name: &str) -> Self::Collection {
        Database::collection::<Document>(self, name)
    }

    async fn list_collections(&self) -> Result<Self::CollectionCursor, Error> {
        Database::list_collections(self).await
    }
}
