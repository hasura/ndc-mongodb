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
pub struct ObjectRelationInsertSchema {
    #[serde(rename = "insertion_order")]
    pub insertion_order: crate::ObjectRelationInsertionOrder,
    /// The name of the object relationship over which the related row must be inserted
    #[serde(rename = "relationship")]
    pub relationship: String,
    #[serde(rename = "type")]
    pub r#type: RHashType,
}

impl ObjectRelationInsertSchema {
    pub fn new(
        insertion_order: crate::ObjectRelationInsertionOrder,
        relationship: String,
        r#type: RHashType,
    ) -> ObjectRelationInsertSchema {
        ObjectRelationInsertSchema {
            insertion_order,
            relationship,
            r#type,
        }
    }
}

///
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum RHashType {
    #[serde(rename = "object_relation")]
    ObjectRelation,
}

impl Default for RHashType {
    fn default() -> RHashType {
        Self::ObjectRelation
    }
}
