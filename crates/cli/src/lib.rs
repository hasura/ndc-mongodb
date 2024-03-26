//! The interpretation of the commands that the CLI can handle.

mod introspection;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use mongodb_agent_common::interface_types::MongoConfig;

#[derive(Debug, Clone, Parser)]
pub struct UpdateArgs {
    #[arg(long = "sample-size", value_name = "N")]
    sample_size: Option<u32>,
}

/// The command invoked by the user.
#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Update the configuration by introspecting the database, using the configuration options.
    Update(UpdateArgs),
}

pub struct Context {
    pub path: PathBuf,
    pub mongo_config: MongoConfig,
}

/// Run a command in a given directory.
pub async fn run(command: Command, context: &Context) -> anyhow::Result<()> {
    match command {
        Command::Update(args) => update(context, &args).await?,
    };
    Ok(())
}

/// Update the configuration in the current directory by introspecting the database.
async fn update(context: &Context, args: &UpdateArgs) -> anyhow::Result<()> {
    let schemas = match args.sample_size {
        None => introspection::get_metadata_from_validation_schema(&context.mongo_config).await?,
        Some(sample_size) => {
            introspection::sample_schema_from_db(sample_size, &context.mongo_config).await?
        }
    };
    configuration::write_schema_directory(&context.path, schemas).await?;

    Ok(())
}
