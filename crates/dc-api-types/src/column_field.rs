/*
 *
 *
 * No description provided (generated by Openapi Generator https://github.com/openapitools/openapi-generator)
 *
 * The version of the OpenAPI document:
 *
 * Generated by: https://openapi-generator.tech
 */

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct ColumnField {
    #[serde(rename = "column")]
    pub column: String,
    #[serde(rename = "column_type")]
    pub column_type: String,
    #[serde(rename = "type")]
    pub r#type: RHashType,
}

impl ColumnField {
    pub fn new(column: String, column_type: String, r#type: RHashType) -> ColumnField {
        ColumnField {
            column,
            column_type,
            r#type,
        }
    }
}

///
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum RHashType {
    #[serde(rename = "column")]
    Column,
}

impl Default for RHashType {
    fn default() -> RHashType {
        Self::Column
    }
}
