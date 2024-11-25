//! The interpretation of the commands that the CLI can handle.

mod exit_codes;
mod introspection;
mod logging;

#[cfg(feature = "native-query-subcommand")]
mod native_query;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

// Exported for use in tests
pub use introspection::type_from_bson;
use mongodb_agent_common::state::try_init_state_from_uri;
#[cfg(feature = "native-query-subcommand")]
pub use native_query::native_query_from_pipeline;

#[derive(Debug, Clone, Parser)]
pub struct UpdateArgs {
    #[arg(long = "sample-size", value_name = "N", required = false)]
    sample_size: Option<u32>,

    #[arg(long = "no-validator-schema", required = false)]
    no_validator_schema: Option<bool>,

    #[arg(long = "all-schema-nullable", required = false)]
    all_schema_nullable: Option<bool>,
}

/// The command invoked by the user.
#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Update the configuration by introspecting the database, using the configuration options.
    Update(UpdateArgs),

    #[cfg(feature = "native-query-subcommand")]
    #[command(subcommand)]
    NativeQuery(native_query::Command),
}

pub struct Context {
    pub path: PathBuf,
    pub connection_uri: Option<String>,
    pub display_color: bool,
}

/// Run a command in a given directory.
pub async fn run(command: Command, context: &Context) -> anyhow::Result<()> {
    match command {
        Command::Update(args) => update(context, &args).await?,

        #[cfg(feature = "native-query-subcommand")]
        Command::NativeQuery(command) => native_query::run(context, command).await?,
    };
    Ok(())
}

/// Update the configuration in the current directory by introspecting the database.
async fn update(context: &Context, args: &UpdateArgs) -> anyhow::Result<()> {
    let connector_state = try_init_state_from_uri(context.connection_uri.as_ref()).await?;

    let configuration_options =
        configuration::parse_configuration_options_file(&context.path).await?;
    // Prefer arguments passed to cli, and fallback to the configuration file
    let sample_size = match args.sample_size {
        Some(size) => size,
        None => configuration_options.introspection_options.sample_size,
    };
    let no_validator_schema = match args.no_validator_schema {
        Some(validator) => validator,
        None => {
            configuration_options
                .introspection_options
                .no_validator_schema
        }
    };
    let all_schema_nullable = match args.all_schema_nullable {
        Some(b) => b,
        None => {
            configuration_options
                .introspection_options
                .all_schema_nullable
        }
    };
    let config_file_changed = configuration::get_config_file_changed(&context.path).await?;

    if !no_validator_schema {
        let schemas_from_json_validation =
            introspection::get_metadata_from_validation_schema(&connector_state).await?;
        configuration::write_schema_directory(&context.path, schemas_from_json_validation).await?;
    }

    let existing_schemas = configuration::list_existing_schemas(&context.path).await?;
    let schemas_from_sampling = introspection::sample_schema_from_db(
        sample_size,
        all_schema_nullable,
        config_file_changed,
        &connector_state,
        &existing_schemas,
    )
    .await?;
    configuration::write_schema_directory(&context.path, schemas_from_sampling).await
}
