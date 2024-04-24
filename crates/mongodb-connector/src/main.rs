mod api_type_conversions;
mod capabilities;
mod error_mapping;
mod mongo_connector;
mod mutation;
mod query_context;
mod query_response;
mod schema;

#[cfg(test)]
mod test_helpers;

use std::error::Error;

use mongo_connector::MongoConnector;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    ndc_sdk::default_main::default_main::<MongoConnector>().await
}
