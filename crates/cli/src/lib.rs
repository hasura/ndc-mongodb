//! The interpretation of the commands that the CLI can handle.

mod introspection;

use std::path::PathBuf;

use clap::Subcommand;

use configuration::Configuration;
use mongodb_agent_common::interface_types::MongoConfig;

/// The command invoked by the user.
#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Update the configuration by introspecting the database, using the configuration options.
    Update,
}

pub struct Context {
    pub path: PathBuf,
    pub mongo_config: MongoConfig,
}

/// Run a command in a given directory.
pub async fn run(command: Command, context: &Context) -> anyhow::Result<()> {
    match command {
        Command::Update => update(context).await?,
    };
    Ok(())
}

/// Update the configuration in the current directory by introspecting the database.
async fn update(context: &Context) -> anyhow::Result<()> {
    let schema = introspection::get_metadata_from_validation_schema(&context.mongo_config).await?;
    let configuration = Configuration::from_schema(schema)?;

    configuration::write_directory(&context.path, &configuration).await?;

    Ok(())
}
