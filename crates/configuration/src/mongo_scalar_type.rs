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
    pub fn lookup_scalar_type(name: &ndc_models::ScalarTypeName) -> Option<Self> {
        Self::try_from(name).ok()
    }
}

impl From<BsonScalarType> for MongoScalarType {
    fn from(value: BsonScalarType) -> Self {
        Self::Bson(value)
    }
}

impl TryFrom<&ndc_models::ScalarTypeName> for MongoScalarType {
    type Error = QueryPlanError;

    fn try_from(name: &ndc_models::ScalarTypeName) -> Result<Self, Self::Error> {
        let name_str = name.to_string();
        if name_str == EXTENDED_JSON_TYPE_NAME {
            Ok(MongoScalarType::ExtendedJSON)
        } else {
            let t = BsonScalarType::from_bson_name(&name_str)
                .map_err(|_| QueryPlanError::UnknownScalarType(name.to_owned()))?;
            Ok(MongoScalarType::Bson(t))
        }
    }
}
