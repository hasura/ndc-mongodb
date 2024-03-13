use std::{io, path::Path};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{read_directory, Metadata};

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Configuration {
    pub metadata: Metadata,
}

impl Configuration {
    pub async fn parse_configuration(
        configuration_dir: impl AsRef<Path> + Send,
    ) -> io::Result<Self> {
        read_directory(configuration_dir).await
    }
}
