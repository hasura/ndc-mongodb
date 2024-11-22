mod aggregation_expression;
pub mod error;
mod helpers;
mod pipeline;
mod pipeline_type_context;
mod pretty_printing;
mod prune_object_types;
mod reference_shorthand;
mod type_annotation;
mod type_constraint;
mod type_solver;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::process::exit;

use clap::Subcommand;
use configuration::schema::ObjectField;
use configuration::{
    native_query::NativeQueryRepresentation::Collection, serialized::NativeQuery, Configuration,
};
use configuration::{read_directory_with_ignored_configs, WithName};
use mongodb_support::aggregate::Pipeline;
use ndc_models::CollectionName;
use tokio::fs;

use crate::exit_codes::ExitCode;
use crate::Context;

use self::error::Result;
use self::pipeline::infer_pipeline_types;
use self::pretty_printing::pretty_print_native_query_info;

/// Create native queries - custom MongoDB queries that integrate into your data graph
#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    /// Create a native query from a JSON file containing an aggregation pipeline
    Create {
        /// Name that will identify the query in your data graph
        #[arg(long, short = 'n', required = true)]
        name: String,

        /// Name of the collection that acts as input for the pipeline - omit for a pipeline that does not require input
        #[arg(long, short = 'c')]
        collection: Option<CollectionName>,

        /// Overwrite any existing native query configuration with the same name
        #[arg(long, short = 'f')]
        force: bool,

        /// Path to a JSON file with an aggregation pipeline
        pipeline_path: PathBuf,
    },

    /// List all configured native queries
    List,
}

pub async fn run(context: &Context, command: Command) -> anyhow::Result<()> {
    match command {
        Command::Create {
            name,
            collection,
            force,
            pipeline_path,
        } => create(context, name, collection, force, &pipeline_path).await,
        Command::List => list(context).await,
    }
}

async fn list(context: &Context) -> std::result::Result<(), anyhow::Error> {
    let configuration = read_configuration(context, &[]).await?;
    for (name, _) in configuration.native_queries {
        println!("{}", name);
    }
    Ok(())
}

async fn create(
    context: &Context,
    name: String,
    collection: Option<CollectionName>,
    force: bool,
    pipeline_path: &Path,
) -> anyhow::Result<()> {
    let native_query_path = {
        let path = get_native_query_path(context, &name);
        if !force && fs::try_exists(&path).await? {
            eprintln!(
                "A native query named {name} already exists at {}.",
                path.to_string_lossy()
            );
            eprintln!("Re-run with --force to overwrite.");
            exit(ExitCode::RefusedToOverwrite.into())
        }
        path
    };

    let configuration = read_configuration(context, &[native_query_path.clone()]).await?;

    let pipeline = match read_pipeline(&pipeline_path).await {
        Ok(p) => p,
        Err(err) => {
            eprintln!("Could not read aggregation pipeline.\n\n{err}");
            exit(ExitCode::CouldNotReadAggregationPipeline.into())
        }
    };
    let native_query = match native_query_from_pipeline(&configuration, &name, collection, pipeline)
    {
        Ok(q) => WithName::named(name, q),
        Err(err) => {
            eprintln!("Error interpreting aggregation pipeline.\n\n{err}");
            exit(ExitCode::CouldNotReadAggregationPipeline.into())
        }
    };

    let native_query_dir = native_query_path
        .parent()
        .expect("parent directory of native query configuration path");
    if !(fs::try_exists(&native_query_dir).await?) {
        fs::create_dir(&native_query_dir).await?;
    }

    if let Err(err) = fs::write(
        &native_query_path,
        serde_json::to_string_pretty(&native_query)?,
    )
    .await
    {
        eprintln!("Error writing native query configuration: {err}");
        exit(ExitCode::ErrorWriting.into())
    };
    eprintln!(
        "\nWrote native query configuration to {}",
        native_query_path.to_string_lossy()
    );
    eprintln!();
    pretty_print_native_query_info(&mut std::io::stderr(), &native_query.value)?;
    Ok(())
}

/// Reads configuration, or exits with specific error code on error
async fn read_configuration(
    context: &Context,
    ignored_configs: &[PathBuf],
) -> anyhow::Result<Configuration> {
    let configuration = match read_directory_with_ignored_configs(&context.path, ignored_configs)
        .await
    {
        Ok(c) => c,
        Err(err) => {
            eprintln!("Could not read connector configuration - configuration must be initialized before creating native queries.\n\n{err:#}");
            exit(ExitCode::CouldNotReadConfiguration.into())
        }
    };
    eprintln!(
        "Read configuration from {}",
        &context.path.to_string_lossy()
    );
    Ok(configuration)
}

async fn read_pipeline(pipeline_path: &Path) -> anyhow::Result<Pipeline> {
    let input = fs::read(pipeline_path).await?;
    let pipeline = serde_json::from_slice(&input)?;
    Ok(pipeline)
}

fn get_native_query_path(context: &Context, name: &str) -> PathBuf {
    context
        .path
        .join(configuration::NATIVE_QUERIES_DIRNAME)
        .join(name)
        .with_extension("json")
}

pub fn native_query_from_pipeline(
    configuration: &Configuration,
    name: &str,
    input_collection: Option<CollectionName>,
    pipeline: Pipeline,
) -> Result<NativeQuery> {
    let pipeline_types =
        infer_pipeline_types(configuration, name, input_collection.as_ref(), &pipeline)?;

    let arguments = pipeline_types
        .parameter_types
        .into_iter()
        .map(|(name, parameter_type)| {
            (
                name,
                ObjectField {
                    r#type: parameter_type,
                    description: None,
                },
            )
        })
        .collect();

    // TODO: move warnings to `run` function
    for warning in pipeline_types.warnings {
        println!("warning: {warning}");
    }
    Ok(NativeQuery {
        representation: Collection,
        input_collection,
        arguments,
        result_document_type: pipeline_types.result_document_type,
        object_types: pipeline_types.object_types,
        pipeline: pipeline.into(),
        description: None,
    })
}
