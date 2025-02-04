use mongodb::bson::{self, Bson};
use serde::Deserialize;

pub const RESULT_FIELD: &str = "result";

/// Value must match the field name in [BsonRowSet]
pub const ROW_SET_AGGREGATES_KEY: &str = "aggregates";

/// Value must match the field name in [BsonRowSet]
pub const ROW_SET_GROUPS_KEY: &str = "groups";

/// Value must match the field name in [BsonRowSet]
pub const ROW_SET_ROWS_KEY: &str = "rows";

#[derive(Debug, Deserialize)]
pub struct BsonRowSet {
    #[serde(default)]
    pub aggregates: Bson, // name matches ROW_SET_AGGREGATES_KEY
    #[serde(default)]
    pub groups: Vec<bson::Document>, // name matches ROW_SET_GROUPS_KEY
    #[serde(default)]
    pub rows: Vec<bson::Document>, // name matches ROW_SET_ROWS_KEY
}
