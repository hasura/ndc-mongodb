use configuration::MongoScalarType;
use mongodb_support::BsonScalarType;
use ndc_query_plan::Type;

pub fn is_nullable(t: &Type<MongoScalarType>) -> bool {
    matches!(
        t,
        Type::Nullable(_)
            | Type::Scalar(
                MongoScalarType::Bson(BsonScalarType::Null) | MongoScalarType::ExtendedJSON
            )
    )
}
