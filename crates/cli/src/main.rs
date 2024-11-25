//! The CLI application. This is used to configure a deployment of mongo-agent-v3.
//!
//! This is intended to be automatically downloaded and invoked via the Hasura CLI, as a plugin.
//! It is unlikely that end-users will use it directly.

use std::env;
use std::path::PathBuf;

use clap::{Parser, ValueHint};
use mongodb_agent_common::state::DATABASE_URI_ENV_VAR;
use mongodb_cli_plugin::{run, Command, Context};

/// The command-line arguments.
#[derive(Debug, Parser)]
pub struct Args {
    /// The path to the configuration. Defaults to the current directory.
    #[arg(
        long = "context-path",
        short = 'p',
        env = "HASURA_PLUGIN_CONNECTOR_CONTEXT_PATH",
        value_name = "DIRECTORY",
        value_hint = ValueHint::DirPath
    )]
    pub context_path: Option<PathBuf>,

    #[arg(
        long = "connection-uri",
        env = DATABASE_URI_ENV_VAR,
        value_name = "URI",
        value_hint = ValueHint::Url
    )]
    pub connection_uri: Option<String>,

    /// Disable color in command output.
    #[arg(long = "no-color", short = 'C')]
    pub no_color: bool,

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
    let context = Context {
        path,
        connection_uri: args.connection_uri,
        display_color: !args.no_color,
    };
    run(args.subcommand, &context).await?;
    Ok(())
}
