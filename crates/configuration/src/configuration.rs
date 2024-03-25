use std::{collections::BTreeMap, path::Path};

use anyhow::ensure;
use itertools::Itertools;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{native_queries::NativeQuery, read_directory, schema::ObjectType, Schema};

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Configuration {
    /// Descriptions of collections and types used in the database
    pub schema: Schema,

    /// Native queries allow arbitrary MongoDB aggregation pipelines where types of results are
    /// specified via user configuration.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub native_queries: BTreeMap<String, NativeQuery>,
}

impl Configuration {
    pub fn validate(
        schema: Schema,
        native_queries: BTreeMap<String, NativeQuery>,
    ) -> anyhow::Result<Self> {
        let config = Configuration {
            schema,
            native_queries,
        };

        {
            let duplicate_type_names: Vec<&str> = config
                .object_types()
                .map(|(name, _)| name.as_ref())
                .duplicates()
                .collect();
            ensure!(
                duplicate_type_names.is_empty(),
                "configuration contains multiple definitions for these object type names: {}",
                duplicate_type_names.join(", ")
            );
        }

        Ok(config)
    }

    pub fn from_schema(schema: Schema) -> anyhow::Result<Self> {
        Self::validate(schema, Default::default())
    }

    pub async fn parse_configuration(
        configuration_dir: impl AsRef<Path> + Send,
    ) -> anyhow::Result<Self> {
        read_directory(configuration_dir).await
    }

    /// Returns object types collected from schema and native queries
    pub fn object_types(&self) -> impl Iterator<Item = (&String, &ObjectType)> {
        let object_types_from_schema = self.schema.object_types.iter();
        let object_types_from_native_queries = self
            .native_queries
            .values()
            .flat_map(|native_query| &native_query.object_types);
        object_types_from_schema.chain(object_types_from_native_queries)
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::*;
    use crate::{schema::Type, Schema};

    #[test]
    fn fails_with_duplicate_object_types() {
        let schema = Schema {
            collections: Default::default(),
            object_types: [(
                "Album".to_owned(),
                ObjectType {
                    fields: Default::default(),
                    description: Default::default(),
                },
            )]
            .into_iter()
            .collect(),
        };
        let native_queries = [(
            "hello".to_owned(),
            NativeQuery {
                object_types: [(
                    "Album".to_owned(),
                    ObjectType {
                        fields: Default::default(),
                        description: Default::default(),
                    },
                )]
                .into_iter()
                .collect(),
                result_type: Type::Object("Album".to_owned()),
                command: doc! { "command": 1 },
                arguments: Default::default(),
                selection_criteria: Default::default(),
                description: Default::default(),
                mode: Default::default(),
            },
        )]
        .into_iter()
        .collect();
        let result = Configuration::validate(schema, native_queries);
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("multiple definitions"));
        assert!(error_msg.contains("Album"));
    }
}
