use std::collections::BTreeMap;

use ndc_sdk::models::{self as v3, ComparisonOperatorDefinition, ScalarType};

use super::ConversionError;

pub fn lookup_relationship<'a>(
    relationships: &'a BTreeMap<String, v3::Relationship>,
    relationship: &str,
) -> Result<&'a v3::Relationship, ConversionError> {
    relationships
        .get(relationship)
        .ok_or_else(|| ConversionError::UnspecifiedRelation(relationship.to_owned()))
}

pub fn lookup_operator_definition(
    scalar_types: &BTreeMap<String, ScalarType>,
    type_name: &str,
    operator: &str,
) -> Result<ComparisonOperatorDefinition, ConversionError> {
    let scalar_type = scalar_types
        .get(type_name)
        .ok_or_else(|| ConversionError::UnknownScalarType(type_name.to_owned()))?;
    let operator = scalar_type
        .comparison_operators
        .get(operator)
        .ok_or_else(|| ConversionError::UnknownComparisonOperator(operator.to_owned()))?;
    Ok(operator.clone())
}
