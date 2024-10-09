use std::{env, error::Error};

use anyhow::anyhow;
use mongodb::{Client, Database};

use crate::mongodb_connection::get_mongodb_client;

pub const DATABASE_URI_ENV_VAR: &str = "MONGODB_DATABASE_URI";

#[derive(Clone, Debug)]
pub struct ConnectorState {
    client: Client,

    /// Name of the database to connect to
    database: String,
}

impl ConnectorState {
    pub fn database(&self) -> Database {
        self.client.database(&self.database)
    }
}

/// Reads database connection URI from environment variable
pub async fn try_init_state() -> Result<ConnectorState, Box<dyn Error + Send + Sync>> {
    // Splitting this out of the `Connector` impl makes error translation easier
    let database_uri = env::var(DATABASE_URI_ENV_VAR)?;
    let state = try_init_state_from_uri(Some(&database_uri)).await?;
    Ok(state)
}

pub async fn try_init_state_from_uri(
    database_uri: Option<&impl AsRef<str>>,
) -> anyhow::Result<ConnectorState> {
    let database_uri = database_uri.ok_or(anyhow!(
        "Missing environment variable {}",
        DATABASE_URI_ENV_VAR
    ))?;
    let client = get_mongodb_client(database_uri.as_ref()).await?;
    let database_name = match client.default_database() {
        Some(database) => Ok(database.name().to_owned()),
        None => Err(anyhow!(
            "${DATABASE_URI_ENV_VAR} environment variable must include a database"
        )),
    }?;
    Ok(ConnectorState {
        client,
        database: database_name,
    })
}
