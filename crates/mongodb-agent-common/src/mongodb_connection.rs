use mongodb::{
    options::{ClientOptions, DriverInfo},
    Client,
};

use crate::interface_types::MongoAgentError;

const DRIVER_NAME: &str = "Hasura";

pub async fn get_mongodb_client(database_uri: &str) -> Result<Client, MongoAgentError> {
    // An extra line of code to work around a DNS issue on Windows:
    let mut options = ClientOptions::parse(database_uri).await?;

    // Helps MongoDB to collect statistics on Hasura use
    options.driver_info = Some(DriverInfo::builder().name(DRIVER_NAME).build());

    let client = Client::with_options(options)?;
    Ok(client)
}
