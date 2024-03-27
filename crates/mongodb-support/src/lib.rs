mod bson_type;
pub mod error;
pub mod align;

pub use self::bson_type::{BsonScalarType, BsonType};

pub const ANY_TYPE_NAME: &str = "any";
