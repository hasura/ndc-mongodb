use std::env;

const CONFIGURATION_MODE_ENV: &str = "HASURA_CONFIGURATION_MODE";
const CONNECTOR_ID_ENV: &str = "HASURA_CONFIGURATION_CONNECTOR_ID";
const CONFIGURATION_SCHEMA_ENV: &str = "HASURA_CONFIGURATION_SCHEMA";
const DEFAULT_SCHEMA: &str = "connector_config";

#[derive(Clone)]
pub enum ConfigurationMode {
    /// Read configuration from the filesystem (default behavior)
    Json,
    /// Read configuration from a PostgreSQL database
    Postgres {
        url: String,
        connector_id: String,
        schema: String,
    },
}

impl std::fmt::Debug for ConfigurationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigurationMode::Json => write!(f, "Json"),
            ConfigurationMode::Postgres {
                connector_id,
                schema,
                ..
            } => f
                .debug_struct("Postgres")
                .field("url", &"<redacted>")
                .field("connector_id", connector_id)
                .field("schema", schema)
                .finish(),
        }
    }
}

/// Resolve configuration mode from environment variables.
///
/// - `HASURA_CONFIGURATION_MODE`: If unset or "json", uses file-based config.
///   If set to a postgres URL, uses postgres-based config.
/// - `HASURA_CONFIGURATION_CONNECTOR_ID`: Required when using postgres mode.
/// - `HASURA_CONFIGURATION_SCHEMA`: Postgres schema name (default: "connector_config").
pub fn resolve_configuration_mode() -> anyhow::Result<ConfigurationMode> {
    resolve_from_values(
        env::var(CONFIGURATION_MODE_ENV).ok().as_deref(),
        env::var(CONNECTOR_ID_ENV).ok().as_deref(),
        env::var(CONFIGURATION_SCHEMA_ENV).ok().as_deref(),
    )
}

fn resolve_from_values(
    mode: Option<&str>,
    connector_id: Option<&str>,
    schema: Option<&str>,
) -> anyhow::Result<ConfigurationMode> {
    let mode = mode.unwrap_or("");

    if mode.is_empty() || mode.eq_ignore_ascii_case("json") {
        return Ok(ConfigurationMode::Json);
    }

    // Treat the value as a postgres connection URL
    let url = mode.to_string();

    let connector_id = connector_id
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{CONNECTOR_ID_ENV} is required when {CONFIGURATION_MODE_ENV} is set to a postgres URL"
            )
        })?
        .to_string();

    let schema = schema
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_SCHEMA)
        .to_string();

    Ok(ConfigurationMode::Postgres {
        url,
        connector_id,
        schema,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_json_mode() {
        let mode = resolve_from_values(None, None, None).unwrap();
        assert!(matches!(mode, ConfigurationMode::Json));
    }

    #[test]
    fn empty_mode_is_json() {
        let mode = resolve_from_values(Some(""), None, None).unwrap();
        assert!(matches!(mode, ConfigurationMode::Json));
    }

    #[test]
    fn explicit_json_mode() {
        let mode = resolve_from_values(Some("json"), None, None).unwrap();
        assert!(matches!(mode, ConfigurationMode::Json));
    }

    #[test]
    fn json_mode_case_insensitive() {
        let mode = resolve_from_values(Some("JSON"), None, None).unwrap();
        assert!(matches!(mode, ConfigurationMode::Json));
    }

    #[test]
    fn postgres_mode_requires_connector_id() {
        let result = resolve_from_values(Some("postgres://localhost/config"), None, None);
        assert!(result.is_err());
    }

    #[test]
    fn postgres_mode_rejects_empty_connector_id() {
        let result = resolve_from_values(Some("postgres://localhost/config"), Some(""), None);
        assert!(result.is_err());
    }

    #[test]
    fn postgres_mode_with_connector_id() {
        let mode = resolve_from_values(
            Some("postgres://localhost/config"),
            Some("my-connector"),
            None,
        )
        .unwrap();
        match mode {
            ConfigurationMode::Postgres {
                url,
                connector_id,
                schema,
            } => {
                assert_eq!(url, "postgres://localhost/config");
                assert_eq!(connector_id, "my-connector");
                assert_eq!(schema, "connector_config");
            }
            _ => panic!("expected Postgres mode"),
        }
    }

    #[test]
    fn postgres_mode_with_custom_schema() {
        let mode = resolve_from_values(
            Some("postgres://localhost/config"),
            Some("my-connector"),
            Some("custom_schema"),
        )
        .unwrap();
        match mode {
            ConfigurationMode::Postgres { schema, .. } => {
                assert_eq!(schema, "custom_schema");
            }
            _ => panic!("expected Postgres mode"),
        }
    }
}
