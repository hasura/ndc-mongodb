mod database;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use self::database::{Collection, ObjectField, ObjectType, Type};

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    #[serde(default)]
    pub collections: Vec<Collection>,
    #[serde(default)]
    pub object_types: Vec<ObjectType>,
}
