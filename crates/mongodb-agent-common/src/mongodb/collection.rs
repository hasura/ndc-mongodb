use async_trait::async_trait;
use futures_util::Stream;
use mongodb::{
    bson::Document,
    error::Error,
    options::{AggregateOptions, FindOptions},
    Collection,
};
use serde::de::DeserializeOwned;

#[cfg(test)]
use mockall::automock;

use super::Pipeline;

#[cfg(test)]
use super::test_helpers::MockCursor;

/// Abstract MongoDB collection methods. This lets us mock a database connection in tests. The
/// automock attribute generates a struct called MockCollectionTrait that implements this trait.
/// The mock provides a variety of methods for mocking and spying on database behavior in tests.
/// See https://docs.rs/mockall/latest/mockall/
#[cfg_attr(test, automock(
    type DocumentCursor=MockCursor<Document>;
    type RowCursor=MockCursor<T>;
))]
#[async_trait]
pub trait CollectionTrait<T>
where
    T: DeserializeOwned + Unpin + Send + Sync + 'static,
{
    type DocumentCursor: Stream<Item = Result<Document, Error>> + 'static;
    type RowCursor: Stream<Item = Result<T, Error>> + 'static;

    async fn aggregate<Options>(
        &self,
        pipeline: Pipeline,
        options: Options,
    ) -> Result<Self::DocumentCursor, Error>
    where
        Options: Into<Option<AggregateOptions>> + Send + 'static;

    async fn find<Filter, Options>(
        &self,
        filter: Filter,
        options: Options,
    ) -> Result<Self::RowCursor, Error>
    where
        Filter: Into<Option<Document>> + Send + 'static,
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
        Collection::aggregate(self, pipeline, options).await
    }

    async fn find<Filter, Options>(
        &self,
        filter: Filter,
        options: Options,
    ) -> Result<Self::RowCursor, Error>
    where
        Filter: Into<Option<Document>> + Send + 'static,
        Options: Into<Option<FindOptions>> + Send + 'static,
    {
        Collection::find(self, filter, options).await
    }
}
