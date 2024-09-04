mod capabilities;
mod mongo_connector;
mod mutation;
mod schema;

use mongo_connector::MongoConnector;

#[tokio::main]
async fn main() -> ndc_sdk::connector::Result<()> {
    ndc_sdk::default_main::default_main::<MongoConnector>().await
}
