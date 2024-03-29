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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MutationOperation {
    #[serde(rename = "delete")]
    Delete {
        /// The fields to return for the rows affected by this delete operation
        #[serde(rename = "returning_fields", skip_serializing_if = "Option::is_none")]
        returning_fields: Option<::std::collections::HashMap<String, crate::Field>>,
        /// The fully qualified name of a table, where the last item in the array is the table name and any earlier items represent the namespacing of the table name
        #[serde(rename = "table")]
        table: Vec<String>,
        #[serde(rename = "where", skip_serializing_if = "Option::is_none")]
        r#where: Option<Box<crate::Expression>>,
    },
    #[serde(rename = "insert")]
    Insert {
        #[serde(rename = "post_insert_check", skip_serializing_if = "Option::is_none")]
        post_insert_check: Option<Box<crate::Expression>>,
        /// The fields to return for the rows affected by this insert operation
        #[serde(rename = "returning_fields", skip_serializing_if = "Option::is_none")]
        returning_fields: Option<::std::collections::HashMap<String, crate::Field>>,
        /// The rows to insert into the table
        #[serde(rename = "rows")]
        rows: Vec<::std::collections::HashMap<String, crate::RowObjectValue>>,
        /// The fully qualified name of a table, where the last item in the array is the table name and any earlier items represent the namespacing of the table name
        #[serde(rename = "table")]
        table: Vec<String>,
    },
    #[serde(rename = "update")]
    Update {
        #[serde(rename = "post_update_check", skip_serializing_if = "Option::is_none")]
        post_update_check: Option<Box<crate::Expression>>,
        /// The fields to return for the rows affected by this update operation
        #[serde(rename = "returning_fields", skip_serializing_if = "Option::is_none")]
        returning_fields: Option<::std::collections::HashMap<String, crate::Field>>,
        /// The fully qualified name of a table, where the last item in the array is the table name and any earlier items represent the namespacing of the table name
        #[serde(rename = "table")]
        table: Vec<String>,
        /// The updates to make to the matched rows in the table
        #[serde(rename = "updates")]
        updates: Vec<crate::RowUpdate>,
        #[serde(rename = "where", skip_serializing_if = "Option::is_none")]
        r#where: Option<Box<crate::Expression>>,
    },
}

///
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum RHashType {
    #[serde(rename = "update")]
    Update,
}

impl Default for RHashType {
    fn default() -> RHashType {
        Self::Update
    }
}
