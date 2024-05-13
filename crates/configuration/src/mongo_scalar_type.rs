use mongodb_support::{BsonScalarType, EXTENDED_JSON_TYPE_NAME};
use ndc_query_plan::QueryPlanError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MongoScalarType {
    /// One of the predefined BSON scalar types
    Bson(BsonScalarType),

    /// Any BSON value, represented as Extended JSON.
    /// To be used when we don't have any more information
    /// about the types of values that a column, field or argument can take.
    /// Also used when we unifying two incompatible types in schemas derived
    /// from sample documents.
    ExtendedJSON,
}

impl MongoScalarType {
    pub fn lookup_scalar_type(name: &str) -> Option<Self> {
        Self::try_from(name).ok()
    }
}

impl TryFrom<&str> for MongoScalarType {
    type Error = QueryPlanError;

    fn try_from(name: &str) -> Result<Self, Self::Error> {
        if name == EXTENDED_JSON_TYPE_NAME {
            Ok(MongoScalarType::ExtendedJSON)
        } else {
            let t = BsonScalarType::from_bson_name(name)
                .map_err(|_| QueryPlanError::UnknownScalarType(name.to_owned()))?;
            Ok(MongoScalarType::Bson(t))
        }
    }
}
