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
pub struct InsertMutationOperation {
    #[serde(rename = "post_insert_check", skip_serializing_if = "Option::is_none")]
    pub post_insert_check: Option<Box<crate::Expression>>,
    /// The fields to return for the rows affected by this insert operation
    #[serde(
        rename = "returning_fields",
        default,
        with = "::serde_with::rust::double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub returning_fields: Option<Option<::std::collections::HashMap<String, crate::Field>>>,
    /// The rows to insert into the table
    #[serde(rename = "rows")]
    pub rows: Vec<::std::collections::HashMap<String, crate::RowObjectValue>>,
    /// The fully qualified name of a table, where the last item in the array is the table name and any earlier items represent the namespacing of the table name
    #[serde(rename = "table")]
    pub table: Vec<String>,
    #[serde(rename = "type")]
    pub r#type: RHashType,
}

impl InsertMutationOperation {
    pub fn new(
        rows: Vec<::std::collections::HashMap<String, crate::RowObjectValue>>,
        table: Vec<String>,
        r#type: RHashType,
    ) -> InsertMutationOperation {
        InsertMutationOperation {
            post_insert_check: None,
            returning_fields: None,
            rows,
            table,
            r#type,
        }
    }
}

///
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum RHashType {
    #[serde(rename = "insert")]
    Insert,
}

impl Default for RHashType {
    fn default() -> RHashType {
        Self::Insert
    }
}
