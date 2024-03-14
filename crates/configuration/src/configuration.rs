use std::{io, path::Path};

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{read_directory, Schema, native_queries::NativeQuery};

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
    pub async fn parse_configuration(
        configuration_dir: impl AsRef<Path> + Send,
    ) -> io::Result<Self> {
        read_directory(configuration_dir).await
    }
}
