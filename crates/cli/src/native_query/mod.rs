mod aggregation_expression;
pub mod error;
mod helpers;
mod pipeline;
mod pipeline_type_context;
mod reference_shorthand;
mod type_constraint;

use std::path::{Path, PathBuf};
use std::process::exit;

use clap::Subcommand;
use configuration::{
    native_query::NativeQueryRepresentation::Collection, serialized::NativeQuery, Configuration,
};
use configuration::{read_directory, WithName};
use mongodb_support::aggregate::Pipeline;
use ndc_models::CollectionName;
use tokio::fs;

use crate::exit_codes::ExitCode;
use crate::Context;

use self::error::Result;
use self::pipeline::infer_pipeline_types;

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
}

pub async fn run(context: &Context, command: Command) -> anyhow::Result<()> {
    match command {
        Command::Create {
            name,
            collection,
            force,
            pipeline_path,
        } => {
            let configuration = match read_directory(&context.path).await {
                Ok(c) => c,
                Err(err) => {
                    eprintln!("Could not read connector configuration - configuration must be initialized before creating native queries.\n\n{err}");
                    exit(ExitCode::CouldNotReadConfiguration.into())
                }
            };
            eprintln!(
                "Read configuration from {}",
                &context.path.to_string_lossy()
            );

            let pipeline = match read_pipeline(&pipeline_path).await {
                Ok(p) => p,
                Err(err) => {
                    eprintln!("Could not read aggregation pipeline.\n\n{err}");
                    exit(ExitCode::CouldNotReadAggregationPipeline.into())
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
            let native_query =
                match native_query_from_pipeline(&configuration, &name, collection, pipeline) {
                    Ok(q) => WithName::named(name, q),
                    Err(_) => todo!(),
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
                "Wrote native query configuration to {}",
                native_query_path.to_string_lossy()
            );
            Ok(())
        }
    }
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
    // TODO: move warnings to `run` function
    for warning in pipeline_types.warnings {
        println!("warning: {warning}");
    }
    Ok(NativeQuery {
        representation: Collection,
        input_collection,
        arguments: Default::default(), // TODO: infer arguments
        result_document_type: pipeline_types.result_document_type,
        object_types: pipeline_types.object_types,
        pipeline: pipeline.into(),
        description: None,
    })
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use configuration::{
        native_query::NativeQueryRepresentation::Collection,
        read_directory,
        schema::{ObjectField, ObjectType, Type},
        serialized::NativeQuery,
        Configuration,
    };
    use mongodb::bson::doc;
    use mongodb_support::{
        aggregate::{Accumulator, Pipeline, Selection, Stage},
        BsonScalarType,
    };
    use ndc_models::ObjectTypeName;
    use pretty_assertions::assert_eq;

    use super::native_query_from_pipeline;

    #[tokio::test]
    async fn infers_native_query_from_pipeline() -> Result<()> {
        let config = read_configuration().await?;
        let pipeline = Pipeline::new(vec![Stage::Documents(vec![
            doc! { "foo": 1 },
            doc! { "bar": 2 },
        ])]);
        let native_query = native_query_from_pipeline(
            &config,
            "selected_title",
            Some("movies".into()),
            pipeline.clone(),
        )?;

        let expected_document_type_name: ObjectTypeName = "selected_title_documents".into();

        let expected_object_types = [(
            expected_document_type_name.clone(),
            ObjectType {
                fields: [
                    (
                        "foo".into(),
                        ObjectField {
                            r#type: Type::Nullable(Box::new(Type::Scalar(BsonScalarType::Int))),
                            description: None,
                        },
                    ),
                    (
                        "bar".into(),
                        ObjectField {
                            r#type: Type::Nullable(Box::new(Type::Scalar(BsonScalarType::Int))),
                            description: None,
                        },
                    ),
                ]
                .into(),
                description: None,
            },
        )]
        .into();

        let expected = NativeQuery {
            representation: Collection,
            input_collection: Some("movies".into()),
            arguments: Default::default(),
            result_document_type: expected_document_type_name,
            object_types: expected_object_types,
            pipeline: pipeline.into(),
            description: None,
        };

        assert_eq!(native_query, expected);
        Ok(())
    }

    #[tokio::test]
    async fn infers_native_query_from_non_trivial_pipeline() -> Result<()> {
        let config = read_configuration().await?;
        let pipeline = Pipeline::new(vec![
            Stage::ReplaceWith(Selection::new(doc! {
                "title": "$title",
                "title_words": { "$split": ["$title", " "] }
            })),
            Stage::Unwind {
                path: "$title_words".to_string(),
                include_array_index: None,
                preserve_null_and_empty_arrays: None,
            },
            Stage::Group {
                key_expression: "$title_words".into(),
                accumulators: [("title_count".into(), Accumulator::Count)].into(),
            },
        ]);
        let native_query = native_query_from_pipeline(
            &config,
            "title_word_frequency",
            Some("movies".into()),
            pipeline.clone(),
        )?;

        assert_eq!(native_query.input_collection, Some("movies".into()));
        assert!(native_query
            .result_document_type
            .to_string()
            .starts_with("title_word_frequency"));
        assert_eq!(
            native_query
                .object_types
                .get(&native_query.result_document_type),
            Some(&ObjectType {
                fields: [
                    (
                        "_id".into(),
                        ObjectField {
                            r#type: Type::Scalar(BsonScalarType::String),
                            description: None,
                        },
                    ),
                    (
                        "title_count".into(),
                        ObjectField {
                            r#type: Type::Scalar(BsonScalarType::Int),
                            description: None,
                        },
                    ),
                ]
                .into(),
                description: None,
            })
        );
        Ok(())
    }

    async fn read_configuration() -> Result<Configuration> {
        read_directory("../../fixtures/hasura/sample_mflix/connector").await
    }
}
