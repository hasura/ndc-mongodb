//! The interpretation of the commands that the CLI can handle.

mod exit_codes;
mod introspection;
mod logging;
#[cfg(test)]
mod tests;

#[cfg(feature = "native-query-subcommand")]
mod native_query;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use configuration::SCHEMA_DIRNAME;
use introspection::sampling::SampledSchema;
// Exported for use in tests
pub use introspection::type_from_bson;
use mongodb_agent_common::{mongodb::DatabaseTrait, state::try_init_state_from_uri};
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
        Command::Update(args) => {
            let connector_state = try_init_state_from_uri(context.connection_uri.as_ref()).await?;
            update(context, &args, &connector_state.database()).await?
        }

        #[cfg(feature = "native-query-subcommand")]
        Command::NativeQuery(command) => native_query::run(context, command).await?,
    };
    Ok(())
}

/// Update the configuration in the current directory by introspecting the database.
async fn update(
    context: &Context,
    args: &UpdateArgs,
    database: &impl DatabaseTrait,
) -> anyhow::Result<()> {
    let configuration_options =
        configuration::parse_configuration_options_file(&context.path).await?;
    // Prefer arguments passed to cli, and fall back to the configuration file
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

    if !no_validator_schema {
        let schemas_from_json_validation =
            introspection::get_metadata_from_validation_schema(database).await?;
        configuration::write_schema_directory(&context.path, schemas_from_json_validation).await?;
    }

    let existing_schemas = configuration::read_existing_schemas(&context.path).await?;
    let SampledSchema {
        schemas: schemas_from_sampling,
        ignored_changes,
    } = introspection::sample_schema_from_db(
        sample_size,
        all_schema_nullable,
        database,
        existing_schemas,
    )
    .await?;
    configuration::write_schema_directory(&context.path, schemas_from_sampling).await?;

    if !ignored_changes.is_empty() {
        eprintln!("Warning: introspection detected some changes to to database thate were **not** applied to existing
schema configurations. To avoid accidental breaking changes the introspection system is
conservative about what changes are applied automatically.");
        eprintln!();
        eprintln!("To apply changes delete the schema configuration files you want updated, and run introspection
again; or edit the files directly.");
        eprintln!();
        eprintln!("These database changes were **not** applied:");
    }
    for (collection_name, changes) in ignored_changes {
        let mut config_path = context.path.join(SCHEMA_DIRNAME).join(collection_name);
        config_path.set_extension("json");
        eprintln!();
        eprintln!("{}:", config_path.to_string_lossy());
        eprintln!("{}", changes)
    }
    Ok(())
}
