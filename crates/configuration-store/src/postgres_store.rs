use anyhow::Context as _;
use configuration::{serialized::Schema, Configuration, ConfigurationOptions};
use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use postgres_native_tls::MakeTlsConnector;

const SOURCE: &str = "MONGODB";

/// Reads connector configuration from a PostgreSQL config store,
/// using the shared config_tables schema with a `raw_schema` column
/// that stores the connector's native schema JSON per collection.
#[derive(Clone)]
pub struct PostgresConfigurationStore {
    pool: Pool,
    connector_id: String,
    schema: String,
}

impl std::fmt::Debug for PostgresConfigurationStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresConfigurationStore")
            .field("connector_id", &self.connector_id)
            .field("schema", &self.schema)
            .finish()
    }
}

impl PostgresConfigurationStore {
    pub fn new(url: String, connector_id: String, schema: String) -> anyhow::Result<Self> {
        let tls_connector = native_tls::TlsConnector::builder()
            .build()
            .context("failed to build TLS connector")?;
        let tls = MakeTlsConnector::new(tls_connector);

        let pg_config: tokio_postgres::Config = url
            .parse()
            .context("failed to parse postgres connection URL")?;

        let manager_config = ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        };
        let manager = Manager::from_config(pg_config, tls, manager_config);
        let pool = Pool::builder(manager)
            .max_size(4)
            .build()
            .context("failed to build postgres connection pool")?;

        Ok(Self {
            pool,
            connector_id,
            schema,
        })
    }

    async fn get_client(&self) -> anyhow::Result<deadpool_postgres::Client> {
        self.pool
            .get()
            .await
            .map_err(|e| anyhow::anyhow!("failed to get postgres connection from pool: {e}"))
    }

    /// Read the schema for a single collection by name.
    /// Returns a Configuration containing only that collection and its associated object types.
    pub async fn read_collection_configuration(
        &self,
        collection_name: &str,
    ) -> anyhow::Result<Configuration> {
        let client = self.get_client().await?;

        let query = format!(
            r#"SELECT name, raw_schema
               FROM "{}".config_tables
               WHERE UPPER(source) = UPPER($1)
                 AND connector_id = $2
                 AND name = $3
                 AND deleted_at IS NULL
               ORDER BY updated_at DESC
               LIMIT 1"#,
            self.schema
        );

        let row = client
            .query_opt(&query, &[&SOURCE, &self.connector_id, &collection_name])
            .await
            .with_context(|| {
                format!("failed to query config_tables for collection {collection_name}")
            })?
            .ok_or_else(|| {
                anyhow::anyhow!("collection {collection_name} not found in config store")
            })?;

        let name: String = row.get(0);
        let raw_schema_json: serde_json::Value = row.get(1);
        let schema: Schema = serde_json::from_value(raw_schema_json)
            .with_context(|| format!("failed to parse raw_schema for collection {name}"))?;

        Configuration::validate(
            schema,
            Default::default(),
            Default::default(),
            ConfigurationOptions::default(),
        )
    }

    /// Read the connection URI from config_metadata.
    /// Returns the value if stored as {"value": "..."} or the env var name if {"variable": "..."}.
    /// Falls back to MONGODB_DATABASE_URI env var if not found.
    pub async fn read_connection_uri(&self) -> anyhow::Result<ConnectionUri> {
        let client = self.get_client().await?;

        let query = format!(
            r#"SELECT value
               FROM "{}".config_metadata
               WHERE UPPER(source) = UPPER($1)
                 AND connector_id = $2
                 AND key = $3
               ORDER BY updated_at DESC
               LIMIT 1"#,
            self.schema
        );

        let row = client
            .query_opt(&query, &[&SOURCE, &self.connector_id, &"connection_uri"])
            .await
            .context("failed to query config_metadata for connection_uri")?;

        match row {
            Some(row) => {
                let value: serde_json::Value = row.get(0);
                let uri: ConnectionUri =
                    serde_json::from_value(value).context("failed to parse connection_uri")?;
                Ok(uri)
            }
            None => Ok(ConnectionUri::Variable {
                variable: DEFAULT_DATABASE_URI_ENV_VAR.to_string(),
            }),
        }
    }
}

/// Connection URI as stored in config_metadata.
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub enum ConnectionUri {
    /// Direct value: {"value": "mongodb://..."}
    Value { value: String },
    /// Environment variable reference: {"variable": "MONGODB_DATABASE_URI"}
    Variable { variable: String },
}

impl ConnectionUri {
    /// Resolve to the actual URI string.
    pub fn resolve(&self) -> anyhow::Result<String> {
        match self {
            ConnectionUri::Value { value } => Ok(value.clone()),
            ConnectionUri::Variable { variable } => std::env::var(variable)
                .map_err(|_| anyhow::anyhow!("environment variable {variable} is not set")),
        }
    }
}

pub const DEFAULT_DATABASE_URI_ENV_VAR: &str = "MONGODB_DATABASE_URI";
