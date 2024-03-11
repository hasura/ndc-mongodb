use thiserror::Error;

use crate::BsonType;

#[derive(Clone, Debug, Error)]
pub enum Error {
    #[error("unknown scalar type: {0}")]
    UnknownScalarType(String),
    #[error("expected scalar type, but found {0:?}")]
    ExpectedScalarType(BsonType),
}
