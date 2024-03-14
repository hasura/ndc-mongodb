mod database;

use schemars::JsonSchema;
use serde::Deserialize;

pub use self::database::{Collection, ObjectField, ObjectType, Type};

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    #[serde(default)]
    pub collections: Vec<Collection>,
    #[serde(default)]
    pub object_types: Vec<ObjectType>,
}
