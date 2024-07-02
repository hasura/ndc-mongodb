use mongodb::bson::Bson;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExtendedJsonMode {
    #[default]
    Canonical,
    Relaxed,
}

impl ExtendedJsonMode {
    pub fn into_extjson(self, value: Bson) -> serde_json::Value {
        match self {
            ExtendedJsonMode::Canonical => value.into_canonical_extjson(),
            ExtendedJsonMode::Relaxed => value.into_relaxed_extjson(),
        }
    }
}
