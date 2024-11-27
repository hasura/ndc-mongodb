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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::exit;

use clap::Subcommand;
use configuration::schema::ObjectField;
use configuration::{
    native_query::NativeQueryRepresentation::Collection, serialized::NativeQuery, Configuration,
};
use configuration::{read_directory_with_ignored_configs, read_native_query_directory, WithName};
use mongodb_support::aggregate::Pipeline;
use ndc_models::{CollectionName, FunctionName};
use pretty::termcolor::{ColorChoice, StandardStream};
use pretty_printing::pretty_print_native_query;
use tokio::fs;

use crate::exit_codes::ExitCode;
use crate::Context;

use self::error::Result;
use self::pipeline::infer_pipeline_types;
use self::pretty_printing::pretty_print_native_query_info;

/// Create or manage native queries - custom MongoDB queries that integrate into your data graph
#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    /// Create a native query from a JSON file containing an aggregation pipeline
    Create {
        /// Name that will identify the query in your data graph (defaults to base name of pipeline file)
        #[arg(long, short = 'n')]
        name: Option<String>,

        /// Name of the collection that acts as input for the pipeline - omit for a pipeline that does not require input
        #[arg(long, short = 'c')]
        collection: Option<CollectionName>,

        /// Overwrite any existing native query configuration with the same name
        #[arg(long, short = 'f')]
        force: bool,

        /// Path to a JSON file with an aggregation pipeline that specifies your custom query. This
        /// is a value that could be given to the MongoDB command db.<collectionName>.aggregate().
        pipeline_path: PathBuf,
    },

    /// Delete a native query identified by name. Use the list subcommand to see native query
    /// names.
    Delete { native_query_name: String },

    /// List all configured native queries
    List,

    /// Print details of a native query identified by name. Use the list subcommand to see native
    /// query names.
    Show { native_query_name: String },
}

pub async fn run(context: &Context, command: Command) -> anyhow::Result<()> {
    match command {
        Command::Create {
            name,
            collection,
            force,
            pipeline_path,
        } => create(context, name, collection, force, &pipeline_path).await,
        Command::Delete { native_query_name } => delete(context, &native_query_name).await,
        Command::List => list(context).await,
        Command::Show { native_query_name } => show(context, &native_query_name).await,
    }
}

async fn list(context: &Context) -> anyhow::Result<()> {
    let native_queries = read_native_queries(context).await?;
    for (name, _) in native_queries {
        println!("{}", name);
    }
    Ok(())
}

async fn delete(context: &Context, native_query_name: &str) -> anyhow::Result<()> {
    let (_, path) = find_native_query(context, native_query_name).await?;
    fs::remove_file(&path).await?;
    eprintln!(
        "Deleted native query configuration at {}",
        path.to_string_lossy()
    );
    Ok(())
}

async fn show(context: &Context, native_query_name: &str) -> anyhow::Result<()> {
    let (native_query, path) = find_native_query(context, native_query_name).await?;
    pretty_print_native_query(&mut stdout(context), &native_query, &path).await?;
    Ok(())
}

async fn create(
    context: &Context,
    name: Option<String>,
    collection: Option<CollectionName>,
    force: bool,
    pipeline_path: &Path,
) -> anyhow::Result<()> {
    let name = match name.or_else(|| {
        pipeline_path
            .file_stem()
            .map(|os_str| os_str.to_string_lossy().to_string())
    }) {
        Some(name) => name,
        None => {
            eprintln!("Could not determine name for native query.");
            exit(ExitCode::InvalidArguments.into())
        }
    };

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

    let pipeline = match read_pipeline(pipeline_path).await {
        Ok(p) => p,
        Err(err) => {
            write_stderr(&format!("Could not read aggregation pipeline.\n\n{err}"));
            exit(ExitCode::CouldNotReadAggregationPipeline.into())
        }
    };
    let native_query = match native_query_from_pipeline(&configuration, &name, collection, pipeline)
    {
        Ok(q) => WithName::named(name, q),
        Err(err) => {
            eprintln!();
            write_stderr(&err.to_string());
            eprintln!();
            write_stderr(&format!("If you are not able to resolve this error you can add the native query by writing the configuration file directly in {}. See https://hasura.io/docs/3.0/connectors/mongodb/native-operations/native-queries/#write-native-query-configurations-directly", native_query_path.to_string_lossy()));
            // eprintln!("See https://hasura.io/docs/3.0/connectors/mongodb/native-operations/native-queries/#write-native-query-configurations-directly");
            eprintln!();
            write_stderr("If you want to request support for a currently unsupported query feature, report a bug, or get support please file an issue at https://github.com/hasura/ndc-mongodb/issues/new?template=native-query.md");
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
        write_stderr(&format!("Error writing native query configuration: {err}"));
        exit(ExitCode::ErrorWriting.into())
    };
    eprintln!(
        "\nWrote native query configuration to {}",
        native_query_path.to_string_lossy()
    );
    eprintln!();
    pretty_print_native_query_info(&mut stdout(context), &native_query.value).await?;
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
            write_stderr(&format!("Could not read connector configuration - configuration must be initialized before creating native queries.\n\n{err:#}"));
            exit(ExitCode::CouldNotReadConfiguration.into())
        }
    };
    eprintln!(
        "Read configuration from {}",
        &context.path.to_string_lossy()
    );
    Ok(configuration)
}

/// Reads native queries skipping configuration processing, or exits with specific error code on error
async fn read_native_queries(
    context: &Context,
) -> anyhow::Result<BTreeMap<FunctionName, (NativeQuery, PathBuf)>> {
    let native_queries = match read_native_query_directory(&context.path, &[]).await {
        Ok(native_queries) => native_queries,
        Err(err) => {
            write_stderr(&format!("Could not read native queries.\n\n{err}"));
            exit(ExitCode::CouldNotReadConfiguration.into())
        }
    };
    Ok(native_queries)
}

async fn find_native_query(
    context: &Context,
    name: &str,
) -> anyhow::Result<(NativeQuery, PathBuf)> {
    let mut native_queries = read_native_queries(context).await?;
    let (_, definition_and_path) = match native_queries.remove_entry(name) {
        Some(native_query) => native_query,
        None => {
            eprintln!("No native query named {name} found.");
            exit(ExitCode::ResourceNotFound.into())
        }
    };
    Ok(definition_and_path)
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

fn stdout(context: &Context) -> StandardStream {
    if context.display_color {
        StandardStream::stdout(ColorChoice::Auto)
    } else {
        StandardStream::stdout(ColorChoice::Never)
    }
}

/// Write a message to sdterr with automatic line wrapping
fn write_stderr(message: &str) {
    let wrap_options = 120;
    eprintln!("{}", textwrap::fill(message, wrap_options))
}
