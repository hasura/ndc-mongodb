use futures_util::stream::{iter, Iter};
use mongodb::error::Error;

/// Create a stream that can be returned from mock implementations for
/// CollectionTrait::aggregate or CollectionTrait::find.
pub fn mock_stream<T>(
    items: Vec<Result<T, Error>>,
) -> Iter<<Vec<Result<T, Error>> as IntoIterator>::IntoIter> {
    iter(items)
}
