// Conditionally compile tests based on the "test" and "integration" features. Requiring
// "integration" causes these tests to be skipped when running a workspace-wide `cargo test` which
// is helpful because the integration tests only work with a set of running services.
//
// To run integration tests run, `cargo test --features integration`
#[cfg(all(test, feature = "integration"))]
mod tests;

mod connector;
mod graphql;

use std::env;

use anyhow::anyhow;
use url::Url;

pub use self::connector::{run_connector_query, ConnectorQueryRequest};
pub use self::graphql::{graphql_query, GraphQLRequest, GraphQLResponse};

const CONNECTOR_URL: &str = "CONNECTOR_URL";
const ENGINE_GRAPHQL_URL: &str = "ENGINE_GRAPHQL_URL";

fn get_connector_url() -> anyhow::Result<Url> {
    let input = env::var(CONNECTOR_URL).map_err(|_| anyhow!("please set {CONNECTOR_URL} to the the base URL of a running MongoDB connector instance"))?;
    let url = Url::parse(&input)?;
    Ok(url)
}

fn get_graphql_url() -> anyhow::Result<String> {
    env::var(ENGINE_GRAPHQL_URL).map_err(|_| anyhow!("please set {ENGINE_GRAPHQL_URL} to the GraphQL endpoint of a running GraphQL Engine server"))
}
