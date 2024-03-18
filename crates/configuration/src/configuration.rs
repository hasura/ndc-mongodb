use std::path::Path;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{native_queries::NativeQuery, read_directory, Schema};

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Configuration {
    /// Descriptions of collections and types used in the database
    pub schema: Schema,

    /// Native queries allow arbitrary MongoDB aggregation pipelines where types of results are
    /// specified via user configuration.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub native_queries: Vec<NativeQuery>,
}

impl Configuration {
    pub fn from_schema(schema: Schema) -> Self {
        Self {
            schema,
            ..Default::default()
        }
    }

    pub async fn parse_configuration(
        configuration_dir: impl AsRef<Path> + Send,
    ) -> anyhow::Result<Self> {
        read_directory(configuration_dir).await
    }
}
