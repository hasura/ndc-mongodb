pub mod align;
mod bson_type;
pub mod error;

pub use self::bson_type::{BsonScalarType, BsonType};

pub const EXTENDED_JSON_TYPE_NAME: &str = "ExtendedJSON";
