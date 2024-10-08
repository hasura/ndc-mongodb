use async_trait::async_trait;
use futures_util::Stream;
use mongodb::{bson::Document, error::Error, options::AggregateOptions, Database};
use mongodb_support::aggregate::Pipeline;

#[cfg(test)]
use mockall::automock;

use super::CollectionTrait;

#[cfg(test)]
use super::MockCollectionTrait;

#[cfg(test)]
use super::test_helpers::MockCursor;

/// Abstract MongoDB database methods. This lets us mock a database connection in tests. The
/// automock attribute generates a struct called MockDatabaseTrait that implements this trait. The
/// mock provides a variety of methods for mocking and spying on database behavior in tests. See
/// https://docs.rs/mockall/latest/mockall/
///
/// I haven't figured out how to make generic associated types work with automock, so  the type
/// argument for `Collection` values produced via `DatabaseTrait::collection` is fixed to to
/// `Document`. That's the way we're using collections in this app anyway.
#[cfg_attr(test, automock(
    type Collection = MockCollectionTrait<Document>;
    type DocumentCursor = MockCursor<Document>;
))]
#[async_trait]
pub trait DatabaseTrait {
    type Collection: CollectionTrait<Document>;
    type DocumentCursor: Stream<Item = Result<Document, Error>>;

    async fn aggregate<Options>(
        &self,
        pipeline: Pipeline,
        options: Options,
    ) -> Result<Self::DocumentCursor, Error>
    where
        Options: Into<Option<AggregateOptions>> + Send + 'static;

    fn collection(&self, name: &str) -> Self::Collection;
}

#[async_trait]
impl DatabaseTrait for Database {
    type Collection = mongodb::Collection<Document>;
    type DocumentCursor = mongodb::Cursor<Document>;

    async fn aggregate<Options>(
        &self,
        pipeline: Pipeline,
        options: Options,
    ) -> Result<Self::DocumentCursor, Error>
    where
        Options: Into<Option<AggregateOptions>> + Send + 'static,
    {
        Database::aggregate(self, pipeline, options).await
    }

    fn collection(&self, name: &str) -> Self::Collection {
        Database::collection::<Document>(self, name)
    }
}
