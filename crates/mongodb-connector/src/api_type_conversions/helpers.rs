use std::collections::BTreeMap;

use ndc_sdk::models::{self as v3};

use super::ConversionError;

pub fn lookup_relationship<'a>(
    relationships: &'a BTreeMap<String, v3::Relationship>,
    relationship: &str,
) -> Result<&'a v3::Relationship, ConversionError> {
    relationships
        .get(relationship)
        .ok_or_else(|| ConversionError::UnspecifiedRelation(relationship.to_owned()))
}
