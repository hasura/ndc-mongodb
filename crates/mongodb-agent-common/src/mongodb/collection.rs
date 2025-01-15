use async_trait::async_trait;
use futures_util::Stream;
use mongodb::{
    bson::Document,
    error::Error,
    options::{AggregateOptions, FindOptions},
    Collection,
};
use mongodb_support::aggregate::Pipeline;
use serde::de::DeserializeOwned;

#[cfg(any(test, feature = "test-helpers"))]
use mockall::automock;

#[cfg(any(test, feature = "test-helpers"))]
use super::test_helpers::MockCursor;

/// Abstract MongoDB collection methods. This lets us mock a database connection in tests. The
/// automock attribute generates a struct called MockCollectionTrait that implements this trait.
/// The mock provides a variety of methods for mocking and spying on database behavior in tests.
/// See https://docs.rs/mockall/latest/mockall/
#[cfg_attr(any(test, feature = "test-helpers"), automock(
    type DocumentCursor=MockCursor<Document>;
    type RowCursor=MockCursor<T>;
))]
#[async_trait]
pub trait CollectionTrait<T>
where
    T: DeserializeOwned + Unpin + Send + Sync + 'static,
{
    type DocumentCursor: Stream<Item = Result<Document, Error>> + 'static + Unpin;
    type RowCursor: Stream<Item = Result<T, Error>> + 'static + Unpin;

    async fn aggregate<Options>(
        &self,
        pipeline: Pipeline,
        options: Options,
    ) -> Result<Self::DocumentCursor, Error>
    where
        Options: Into<Option<AggregateOptions>> + Send + 'static;

    async fn find<Options>(
        &self,
        filter: Document,
        options: Options,
    ) -> Result<Self::RowCursor, Error>
    where
        Options: Into<Option<FindOptions>> + Send + 'static;
}

#[async_trait]
impl<T> CollectionTrait<T> for Collection<T>
where
    T: DeserializeOwned + Unpin + Send + Sync + 'static,
{
    type DocumentCursor = mongodb::Cursor<Document>;
    type RowCursor = mongodb::Cursor<T>;

    async fn aggregate<Options>(
        &self,
        pipeline: Pipeline,
        options: Options,
    ) -> Result<Self::DocumentCursor, Error>
    where
        Options: Into<Option<AggregateOptions>> + Send + 'static,
    {
        Collection::aggregate(self, pipeline)
            .with_options(options)
            .await
    }

    async fn find<Options>(
        &self,
        filter: Document,
        options: Options,
    ) -> Result<Self::RowCursor, Error>
    where
        Options: Into<Option<FindOptions>> + Send + 'static,
    {
        Collection::find(self, filter).with_options(options).await
    }
}
