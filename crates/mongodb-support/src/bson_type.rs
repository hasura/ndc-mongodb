use enum_iterator::{all, Sequence};
use mongodb::bson::Bson;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::Error;

// According to the MongoDB documentation this is the list of BSON types.
// https://www.mongodb.com/docs/manual/reference/operator/query/type/#std-label-document-type-available-types
//
// - "double"
// - "string"
// - "object"
// - "array"
// - "binData"
// - "undefined"
// - "objectId"
// - "bool"
// - "date"
// - "null"
// - "regex"
// - "dbPointer"
// - "javascript"
// - "symbol"
// - "javascriptWithScope"
// - "int"
// - "timestamp"
// - "long"
// - "decimal"
// - "minKey"
// - "maxKey"
//
// This list does not include "number" which is an alias for "double", "int", "long", or "decimal"

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BsonType {
    Object,
    Array,
    Scalar(BsonScalarType),
}

impl<'de> Deserialize<'de> for BsonType {
    /// bson_type may be a string, or an array containing a single string
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let type_name = if let Some(s) = value.as_str() {
            Ok(s)
        } else if let Some(a) = value.as_array() {
            if a.len() == 1 {
                if let Some(s) = a[0].as_str() {
                    Ok(s)
                } else {
                    Err(serde::de::Error::custom(
                        "expected bsonType array to contain a string",
                    ))
                }
            } else {
                Err(serde::de::Error::custom(
                    "expected bsonType array to contain exactly one string",
                ))
            }
        } else {
            Err(serde::de::Error::custom(format!(
        "found bsonType that is neither a string, nor an array containing a single string: {value}")))
        }?;

        match type_name {
            "object" => Ok(BsonType::Object),
            "array" => Ok(BsonType::Array),
            name => {
                let scalar_type = BsonScalarType::from_bson_name(name).map_err(|_| {
                    serde::de::Error::custom(format!("unknown BSON scalar type, {name}"))
                })?;
                Ok(BsonType::Scalar(scalar_type))
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Sequence, JsonSchema)]
pub enum BsonScalarType {
    // numeric
    Double,
    Decimal,
    Int,
    Long,

    // text
    String,

    // date/time
    Date,
    Timestamp,

    // binary subtypes - these are stored in BSON using the BinData type, but there are multiple
    // binary subtype codes, and it's useful to have first-class representations for those
    UUID, // subtype 4

    // other
    BinData,
    ObjectId,
    Bool,
    Null,
    Regex,
    Javascript,
    JavascriptWithScope,
    MinKey,
    MaxKey,

    // other other
    Undefined,
    DbPointer,
    Symbol,
}

use BsonScalarType as S;

impl BsonScalarType {
    pub fn bson_name(self) -> &'static str {
        match self {
            S::Double => "double",
            S::Decimal => "decimal",
            S::Int => "int",
            S::Long => "long",
            S::String => "string",
            S::Date => "date",
            S::Timestamp => "timestamp",
            S::BinData => "binData",
            S::ObjectId => "objectId",
            S::Bool => "bool",
            S::Null => "null",
            S::Regex => "regex",
            S::Javascript => "javascript",
            S::JavascriptWithScope => "javascriptWithScope",
            S::MinKey => "minKey",
            S::MaxKey => "maxKey",
            S::Undefined => "undefined",
            S::DbPointer => "dbPointer",
            S::Symbol => "symbol",
            S::UUID => "uuid",
        }
    }

    pub fn graphql_name(self) -> &'static str {
        match self {
            S::Double => "Double",
            S::Decimal => "Decimal",
            S::Int => "Int",
            S::Long => "Long",
            S::String => "String",
            S::Date => "Date",
            S::Timestamp => "Timestamp",
            S::BinData => "BinData",
            S::ObjectId => "ObjectId",
            S::Bool => "Bool",
            S::Null => "Null",
            S::Regex => "Regex",
            S::Javascript => "Javascript",
            S::JavascriptWithScope => "JavascriptWithScope",
            S::MinKey => "MinKey",
            S::MaxKey => "MaxKey",
            S::Undefined => "Undefined",
            S::DbPointer => "DbPointer",
            S::Symbol => "Symbol",
            S::UUID => "UUID",
        }
    }

    pub fn from_bson_name(name: &str) -> Result<Self, Error> {
        // "number" is an alias for "double", "int", or "long". Assume the most general-ish case of
        // "double"
        if name == "number" {
            return Ok(S::Double);
        }
        // case-insensitive comparison because we are inconsistent about initial-letter
        // capitalization between v2 and v3
        let scalar_type =
            all::<BsonScalarType>().find(|s| s.bson_name().eq_ignore_ascii_case(name));
        scalar_type.ok_or_else(|| Error::UnknownScalarType(name.to_owned()))
    }

    pub fn is_binary(self) -> bool {
        match self {
            S::BinData => true,
            S::UUID => true,
            S::Double => false,
            S::Decimal => false,
            S::Int => false,
            S::Long => false,
            S::String => false,
            S::Date => false,
            S::Timestamp => false,
            S::ObjectId => false,
            S::Bool => false,
            S::Null => false,
            S::Regex => false,
            S::Javascript => false,
            S::JavascriptWithScope => false,
            S::MinKey => false,
            S::MaxKey => false,
            S::Undefined => false,
            S::DbPointer => false,
            S::Symbol => false,
        }
    }

    pub fn is_orderable(self) -> bool {
        match self {
            S::Double => true,
            S::Decimal => true,
            S::Int => true,
            S::Long => true,
            S::String => true,
            S::Date => true,
            S::Timestamp => true,
            S::BinData => false,
            S::ObjectId => false,
            S::Bool => false,
            S::Null => false,
            S::Regex => false,
            S::Javascript => false,
            S::JavascriptWithScope => false,
            S::MinKey => false,
            S::MaxKey => false,
            S::Undefined => false,
            S::DbPointer => false,
            S::Symbol => false,
            S::UUID => false,
        }
    }

    pub fn is_numeric(self) -> bool {
        match self {
            S::Double => true,
            S::Decimal => true,
            S::Int => true,
            S::Long => true,
            S::String => false,
            S::Date => false,
            S::Timestamp => false,
            S::BinData => false,
            S::ObjectId => false,
            S::Bool => false,
            S::Null => false,
            S::Regex => false,
            S::Javascript => false,
            S::JavascriptWithScope => false,
            S::MinKey => false,
            S::MaxKey => false,
            S::Undefined => false,
            S::DbPointer => false,
            S::Symbol => false,
            S::UUID => false,
        }
    }

    pub fn is_fractional(self) -> bool {
        match self {
            S::Double => true,
            S::Decimal => true,
            S::Int => false,
            S::Long => false,
            S::String => false,
            S::Date => false,
            S::Timestamp => false,
            S::BinData => false,
            S::UUID => false,
            S::ObjectId => false,
            S::Bool => false,
            S::Null => false,
            S::Regex => false,
            S::Javascript => false,
            S::JavascriptWithScope => false,
            S::MinKey => false,
            S::MaxKey => false,
            S::Undefined => false,
            S::DbPointer => false,
            S::Symbol => false,
        }
    }

    pub fn is_comparable(self) -> bool {
        match self {
            S::Double => true,
            S::Decimal => true,
            S::Int => true,
            S::Long => true,
            S::String => true,
            S::Date => true,
            S::Timestamp => true,
            S::BinData => true,
            S::ObjectId => true,
            S::Bool => true,
            S::Null => true,
            S::Regex => false,
            S::Javascript => false,
            S::JavascriptWithScope => false,
            S::MinKey => true,
            S::MaxKey => true,
            S::Undefined => true,
            S::DbPointer => true,
            S::Symbol => true,
            S::UUID => true,
        }
    }

    /// True iff we consider a to be a supertype of b.
    ///
    /// Note that if you add more supertypes here then it is important to also update the custom
    /// equality check in our tests in mongodb_agent_common::query::serialization::tests. Equality
    /// needs to be transitive over supertypes, so for example if we have,
    ///
    /// (Double, Int), (Decimal, Double)
    ///
    /// then in addition to comparing ints to doubles, and doubles to decimals, we also need to compare
    /// decimals to ints.
    pub fn is_supertype(a: Self, b: Self) -> bool {
        Self::common_supertype(a, b).is_some_and(|c| c == a)
    }

    /// If there is a BSON scalar type that encompasses both a and b, return it. This does not
    /// require a and to overlap. The returned type may be equal to a or b if one is a supertype of
    /// the other.
    pub fn common_supertype(a: BsonScalarType, b: BsonScalarType) -> Option<BsonScalarType> {
        fn helper(a: BsonScalarType, b: BsonScalarType) -> Option<BsonScalarType> {
            if a == b {
                Some(a)
            } else if a.is_binary() && b.is_binary() {
                Some(S::BinData)
            } else {
                match (a, b) {
                    (S::Double, S::Int) => Some(S::Double),
                    _ => None,
                }
            }
        }
        helper(a, b).or_else(|| helper(b, a))
    }
}

impl Serialize for BsonScalarType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.bson_name())
    }
}

impl<'de> Deserialize<'de> for BsonScalarType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        BsonScalarType::from_bson_name(&s).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for BsonScalarType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.bson_name())
    }
}

impl TryFrom<&Bson> for BsonScalarType {
    type Error = Error;

    fn try_from(value: &Bson) -> Result<Self, Self::Error> {
        match value {
            Bson::Double(_) => Ok(S::Double),
            Bson::String(_) => Ok(S::String),
            Bson::Array(_) => Err(Error::ExpectedScalarType(BsonType::Array)),
            Bson::Document(_) => Err(Error::ExpectedScalarType(BsonType::Object)),
            Bson::Boolean(_) => Ok(S::Bool),
            Bson::Null => Ok(S::Null),
            Bson::RegularExpression(_) => Ok(S::Regex),
            Bson::JavaScriptCode(_) => Ok(S::Javascript),
            Bson::JavaScriptCodeWithScope(_) => Ok(S::JavascriptWithScope),
            Bson::Int32(_) => Ok(S::Int),
            Bson::Int64(_) => Ok(S::Long),
            Bson::Timestamp(_) => Ok(S::Timestamp),
            Bson::Binary(_) => Ok(S::BinData),
            Bson::ObjectId(_) => Ok(S::ObjectId),
            Bson::DateTime(_) => Ok(S::Date),
            Bson::Symbol(_) => Ok(S::Symbol),
            Bson::Decimal128(_) => Ok(S::Decimal),
            Bson::Undefined => Ok(S::Undefined),
            Bson::MaxKey => Ok(S::MaxKey),
            Bson::MinKey => Ok(S::MinKey),
            Bson::DbPointer(_) => Ok(S::DbPointer),
        }
    }
}

impl TryFrom<BsonType> for BsonScalarType {
    type Error = Error;

    fn try_from(value: BsonType) -> Result<Self, Self::Error> {
        match value {
            BsonType::Scalar(scalar_type) => Ok(scalar_type),
            _ => Err(Error::ExpectedScalarType(value)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::BsonScalarType;

    use super::BsonType;

    #[test]
    fn parses_object_type_from_array() -> Result<(), anyhow::Error> {
        let t: BsonType = serde_json::from_str(r#"["object"]"#)?;
        assert_eq!(t, BsonType::Object);
        Ok(())
    }

    #[test]
    fn parses_scalar_type_from_array() -> Result<(), anyhow::Error> {
        let t: BsonType = serde_json::from_str(r#"["double"]"#)?;
        assert_eq!(t, BsonType::Scalar(BsonScalarType::Double));
        Ok(())
    }

    #[test]
    fn unifies_double_and_int() {
        use BsonScalarType as S;
        let t1 = S::common_supertype(S::Double, S::Int);
        let t2 = S::common_supertype(S::Int, S::Double);
        assert_eq!(t1, Some(S::Double));
        assert_eq!(t2, Some(S::Double));
    }

    #[test]
    fn unifies_bin_data_and_uuid() {
        use BsonScalarType as S;
        let t1 = S::common_supertype(S::BinData, S::UUID);
        let t2 = S::common_supertype(S::UUID, S::BinData);
        assert_eq!(t1, Some(S::BinData));
        assert_eq!(t2, Some(S::BinData));
    }
}
