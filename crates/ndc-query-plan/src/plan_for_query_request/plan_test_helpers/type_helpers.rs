use crate::{ObjectType, Type};

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

pub fn object_type(
    fields: impl IntoIterator<Item = (impl ToString, impl Into<Type<ScalarType>>)>,
) -> Type<ScalarType> {
    Type::Object(ObjectType {
        name: None,
        fields: fields
            .into_iter()
            .map(|(name, field)| (name.to_string(), field.into()))
            .collect(),
    })
}
