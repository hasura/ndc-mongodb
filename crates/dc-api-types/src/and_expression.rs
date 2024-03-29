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
pub struct AndExpression {
    #[serde(rename = "expressions")]
    pub expressions: Vec<crate::Expression>,
    #[serde(rename = "type")]
    pub r#type: RHashType,
}

impl AndExpression {
    pub fn new(expressions: Vec<crate::Expression>, r#type: RHashType) -> AndExpression {
        AndExpression {
            expressions,
            r#type,
        }
    }
}

///
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum RHashType {
    #[serde(rename = "and")]
    And,
}

impl Default for RHashType {
    fn default() -> RHashType {
        Self::And
    }
}
