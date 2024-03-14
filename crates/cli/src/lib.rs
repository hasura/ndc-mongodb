//! The interpretation of the commands that the CLI can handle.
//!
//! The CLI can do a few things. This provides a central point where those things are routed and
//! then done, making it easier to test this crate deterministically.


use std::path::PathBuf;

use clap::Subcommand;

use configuration::Configuration;
use mongodb_agent_common::{interface_types::MongoConfig, schema::get_schema};

/// The command invoked by the user.
#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Initialize a configuration in the current (empty) directory.
    // Initialize,
    /// Update the configuration by introspecting the database, using the configuration options.
    Update,
}

/// The set of errors that can go wrong _in addition to_ generic I/O or parsing errors.
#[derive(Debug, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("directory is not empty")]
    DirectoryIsNotEmpty,
}

pub struct Context {
    pub path: PathBuf,
    pub mongo_config: MongoConfig,
}

/// Run a command in a given directory.
pub async fn run(command: Command, context: &Context) -> anyhow::Result<()> {
    match command {
        // Command::Initialize => initialize(context_path)?,
        Command::Update => update(context).await?,
    };
    Ok(())
}

/// Initialize an empty directory with an empty connector configuration.
///
/// An empty configuration contains default settings and options, and is expected to be filled with
/// information such as the database connection string by the user, and later on metadata
/// information via introspection.
// fn initialize(context_path: &Path) -> anyhow::Result<()> {
//     let configuration_file = context_path.join(CONFIGURATION_FILENAME);
//     fs::create_dir_all(context_path)?;

//     // refuse to initialize the directory unless it is empty
//     let mut items_in_dir = fs::read_dir(context_path)?;
//     if items_in_dir.next().is_some() {
//         Err(Error::DirectoryIsNotEmpty)?;
//     }

//     let _writer = fs::File::create(configuration_file)?;
//     // serde_json::to_writer_pretty(writer, &MongoConnector::make_empty_configuration())?;
//     Ok(())
// }

/// Update the configuration in the current directory by introspecting the database.
///
/// This expects a configuration with a valid connection URI.
async fn update(context: &Context) -> anyhow::Result<()> {
    // TODO: Get metadata directly from DB introspection instead of going via v2 get_schema()
    let _schema = get_schema(&context.mongo_config).await?;

    let configuration = Configuration::default(); // v2_schema_response_to_configuration(schema);

    configuration::write_directory(&context.path, &configuration).await?;

    Ok(())
}
