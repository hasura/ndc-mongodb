//! The CLI application. This is used to configure a deployment of mongo-agent-v3.
//!
//! This is intended to be automatically downloaded and invoked via the Hasura CLI, as a plugin.
//! It is unlikely that end-users will use it directly.

use anyhow::anyhow;
use std::env;
use std::path::PathBuf;

use clap::Parser;
use mongodb_agent_v3::state::{try_init_state_from_uri, DATABASE_URI_ENV_VAR};
use mongodb_cli_plugin::{run, Command, Context};

/// The command-line arguments.
#[derive(Debug, Parser)]
pub struct Args {
    /// The path to the configuration. Defaults to the current directory.
    #[arg(
        long = "context",
        env = "HASURA_PLUGIN_CONNECTOR_CONTEXT_PATH",
        value_name = "DIRECTORY"
    )]
    pub context_path: Option<PathBuf>,

    #[arg(
        long = "connection-uri",
        env = DATABASE_URI_ENV_VAR,
        required = true,
        value_name = "URI"
    )]
    pub connection_uri: String,

    /// The command to invoke.
    #[command(subcommand)]
    pub subcommand: Command,
}

/// The application entrypoint. It pulls information from the environment and then calls the [run]
/// function. The library remains unaware of the environment, so that we can more easily test it.
#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    // Default the context path to the current directory.
    let path = match args.context_path {
        Some(path) => path,
        None => env::current_dir()?,
    };
    let mongo_config = try_init_state_from_uri(&args.connection_uri)
        .await
        .map_err(|e| anyhow!("Error initializing MongoDB state {}", e))?;
    let context = Context { path, mongo_config };
    run(args.subcommand, &context).await?;
    Ok(())
}
