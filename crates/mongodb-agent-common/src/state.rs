use std::{env, error::Error};

use anyhow::anyhow;
use configuration::Configuration;

use crate::{interface_types::MongoConfig, mongodb_connection::get_mongodb_client};

pub const DATABASE_URI_ENV_VAR: &str = "MONGODB_DATABASE_URI";

/// Reads database connection URI from environment variable
pub async fn try_init_state(
    configuration: &Configuration,
) -> Result<MongoConfig, Box<dyn Error + Send + Sync>> {
    // Splitting this out of the `Connector` impl makes error translation easier
    let database_uri = env::var(DATABASE_URI_ENV_VAR)?;
    try_init_state_from_uri(&database_uri, configuration).await
}

pub async fn try_init_state_from_uri(
    database_uri: &str,
    configuration: &Configuration,
) -> Result<MongoConfig, Box<dyn Error + Send + Sync>> {
    let client = get_mongodb_client(database_uri).await?;
    let database_name = match client.default_database() {
        Some(database) => Ok(database.name().to_owned()),
        None => Err(anyhow!(
            "${DATABASE_URI_ENV_VAR} environment variable must include a database"
        )),
    }?;
    Ok(MongoConfig {
        client,
        database: database_name,
        native_queries: configuration.native_queries.clone(),
        object_types: configuration
            .object_types()
            .map(|(name, object_type)| (name.clone(), object_type.clone()))
            .collect(),
    })
}
