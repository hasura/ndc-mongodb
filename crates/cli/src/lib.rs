//! The interpretation of the commands that the CLI can handle.

mod introspection;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use mongodb_agent_common::interface_types::MongoConfig;

#[derive(Debug, Clone, Parser)]
pub struct UpdateArgs {
    #[arg(long = "sample-size", value_name = "N", default_value_t = 10)]
    sample_size: u32,

    #[arg(long = "no-validator-schema", default_value_t = false)]
    no_validator_schema: bool,
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
    if !args.no_validator_schema {
        let schemas_from_json_validation =
            introspection::get_metadata_from_validation_schema(&context.mongo_config).await?;
        configuration::write_schema_directory(&context.path, schemas_from_json_validation).await?;
    }

    let existing_schemas = configuration::list_existing_schemas(&context.path).await?;
    let schemas_from_sampling = introspection::sample_schema_from_db(
        args.sample_size,
        &context.mongo_config,
        &existing_schemas,
    )
    .await?;
    configuration::write_schema_directory(&context.path, schemas_from_sampling).await
}
