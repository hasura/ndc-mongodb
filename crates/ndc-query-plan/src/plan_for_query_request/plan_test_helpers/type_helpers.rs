use crate::Type;

use super::ScalarType;

pub fn date() -> Type<ScalarType> {
    Type::Scalar(ScalarType::Date)
}

pub fn double() -> Type<ScalarType> {
    Type::Scalar(ScalarType::Double)
}

pub fn int() -> Type<ScalarType> {
    Type::Scalar(ScalarType::Int)
}

pub fn string() -> Type<ScalarType> {
    Type::Scalar(ScalarType::String)
}
