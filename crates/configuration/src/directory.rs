use anyhow::{anyhow, Context as _};
use futures::stream::TryStreamExt as _;
use itertools::Itertools as _;
use ndc_models::{CollectionName, FunctionName};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use tokio::fs;
use tokio_stream::wrappers::ReadDirStream;

use crate::{
    configuration::ConfigurationOptions,
    schema::CollectionSchema,
    serialized::{NativeQuery, Schema},
    with_name::WithName,
    Configuration,
};

pub const SCHEMA_DIRNAME: &str = "schema";
pub const NATIVE_MUTATIONS_DIRNAME: &str = "native_mutations";
pub const NATIVE_QUERIES_DIRNAME: &str = "native_queries";
pub const CONFIGURATION_OPTIONS_BASENAME: &str = "configuration";

// Deprecated: Discussion came out that we standardize names and the decision
// was to use `native_mutations`. We should leave this in for a few releases
// with some CHANGELOG/Docs messaging around deprecation
pub const NATIVE_PROCEDURES_DIRNAME: &str = "native_procedures";

pub const CONFIGURATION_EXTENSIONS: [(&str, FileFormat); 3] =
    [("json", JSON), ("yaml", YAML), ("yml", YAML)];
pub const DEFAULT_EXTENSION: &str = "json";

#[derive(Clone, Copy, Debug)]
pub enum FileFormat {
    Json,
    Yaml,
}

const JSON: FileFormat = FileFormat::Json;
const YAML: FileFormat = FileFormat::Yaml;

/// Read configuration from a directory
pub async fn read_directory(
    configuration_dir: impl AsRef<Path> + Send,
) -> anyhow::Result<Configuration> {
    read_directory_with_ignored_configs(configuration_dir, &[]).await
}

/// Read configuration from a directory
pub async fn read_directory_with_ignored_configs(
    configuration_dir: impl AsRef<Path> + Send,
    ignored_configs: &[PathBuf],
) -> anyhow::Result<Configuration> {
    let dir = configuration_dir.as_ref();

    let schemas = read_subdir_configs::<String, Schema>(&dir.join(SCHEMA_DIRNAME), ignored_configs)
        .await?
        .unwrap_or_default();
    let schema = schemas.into_values().fold(Schema::default(), Schema::merge);

    // Deprecated see message above at NATIVE_PROCEDURES_DIRNAME
    let native_procedures =
        read_subdir_configs(&dir.join(NATIVE_PROCEDURES_DIRNAME), ignored_configs)
            .await?
            .unwrap_or_default();

    // TODO: Once we fully remove `native_procedures` after a deprecation period we can remove `mut`
    let mut native_mutations =
        read_subdir_configs(&dir.join(NATIVE_MUTATIONS_DIRNAME), ignored_configs)
            .await?
            .unwrap_or_default();

    let native_queries = read_native_query_directory(dir, ignored_configs)
        .await?
        .into_iter()
        .map(|(name, (config, _))| (name, config))
        .collect();

    let options = parse_configuration_options_file(dir).await?;

    native_mutations.extend(native_procedures.into_iter());

    Configuration::validate(schema, native_mutations, native_queries, options)
}

/// Read native queries only, and skip configuration processing
pub async fn read_native_query_directory(
    configuration_dir: impl AsRef<Path> + Send,
    ignored_configs: &[PathBuf],
) -> anyhow::Result<BTreeMap<FunctionName, (NativeQuery, PathBuf)>> {
    let dir = configuration_dir.as_ref();
    let native_queries =
        read_subdir_configs_with_paths(&dir.join(NATIVE_QUERIES_DIRNAME), ignored_configs)
            .await?
            .unwrap_or_default();
    Ok(native_queries)
}

/// Parse all files in a directory with one of the allowed configuration extensions according to
/// the given type argument. For example if `T` is `NativeMutation` this function assumes that all
/// json and yaml files in the given directory should be parsed as native mutation configurations.
///
/// Assumes that every configuration file has a `name` field.
async fn read_subdir_configs<N, T>(
    subdir: &Path,
    ignored_configs: &[PathBuf],
) -> anyhow::Result<Option<BTreeMap<N, T>>>
where
    for<'a> T: Deserialize<'a>,
    for<'a> N: Ord + ToString + Deserialize<'a>,
{
    let configs_with_paths = read_subdir_configs_with_paths(subdir, ignored_configs).await?;
    let configs_without_paths = configs_with_paths.map(|cs| {
        cs.into_iter()
            .map(|(name, (config, _))| (name, config))
            .collect()
    });
    Ok(configs_without_paths)
}

async fn read_subdir_configs_with_paths<N, T>(
    subdir: &Path,
    ignored_configs: &[PathBuf],
) -> anyhow::Result<Option<BTreeMap<N, (T, PathBuf)>>>
where
    for<'a> T: Deserialize<'a>,
    for<'a> N: Ord + ToString + Deserialize<'a>,
{
    if !(fs::try_exists(subdir).await?) {
        return Ok(None);
    }

    let dir_stream = ReadDirStream::new(fs::read_dir(subdir).await?);
    let configs: Vec<WithName<N, (T, PathBuf)>> = dir_stream
        .map_err(anyhow::Error::from)
        .try_filter_map(|dir_entry| async move {
            // Permits regular files and symlinks, does not filter out symlinks to directories.
            let is_file = !(dir_entry.file_type().await?.is_dir());
            if !is_file {
                return Ok(None);
            }

            let path = dir_entry.path();
            let extension = path.extension().and_then(|ext| ext.to_str());

            if ignored_configs
                .iter()
                .any(|ignored| path.ends_with(ignored))
            {
                return Ok(None);
            }

            let format_option = extension
                .and_then(|ext| {
                    CONFIGURATION_EXTENSIONS
                        .iter()
                        .find(|(expected_ext, _)| ext == *expected_ext)
                })
                .map(|(_, format)| *format);

            Ok(format_option.map(|format| (path, format)))
        })
        .and_then(|(path, format)| async move {
            let config = parse_config_file::<WithName<N, T>>(&path, format).await?;
            Ok(WithName {
                name: config.name,
                value: (config.value, path),
            })
        })
        .try_collect()
        .await?;

    let duplicate_names = configs
        .iter()
        .map(|c| c.name.to_string())
        .duplicates()
        .collect::<Vec<_>>();

    if duplicate_names.is_empty() {
        Ok(Some(WithName::into_map(configs)))
    } else {
        Err(anyhow!(
            "found duplicate names in configuration: {}",
            duplicate_names.join(", ")
        ))
    }
}

pub async fn parse_configuration_options_file(dir: &Path) -> anyhow::Result<ConfigurationOptions> {
    let json_filename = configuration_file_path(dir, JSON);
    if fs::try_exists(&json_filename).await? {
        return parse_config_file(json_filename, JSON).await;
    }

    let yaml_filename = configuration_file_path(dir, YAML);
    if fs::try_exists(&yaml_filename).await? {
        return parse_config_file(yaml_filename, YAML).await;
    }

    tracing::warn!(
        "{CONFIGURATION_OPTIONS_BASENAME}.json not found, using default connector settings"
    );

    // If a configuration file does not exist use defaults and write the file
    let defaults: ConfigurationOptions = Default::default();
    let _ = write_file(dir, CONFIGURATION_OPTIONS_BASENAME, &defaults).await;
    Ok(defaults)
}

fn configuration_file_path(dir: &Path, format: FileFormat) -> PathBuf {
    let mut file_path = dir.join(CONFIGURATION_OPTIONS_BASENAME);
    match format {
        FileFormat::Json => file_path.set_extension("json"),
        FileFormat::Yaml => file_path.set_extension("yaml"),
    };
    file_path
}

async fn parse_config_file<T>(path: impl AsRef<Path>, format: FileFormat) -> anyhow::Result<T>
where
    for<'a> T: Deserialize<'a>,
{
    let bytes = fs::read(path.as_ref()).await?;
    tracing::debug!(
        path = %path.as_ref().display(),
        ?format,
        content = %std::str::from_utf8(&bytes).unwrap_or("<invalid utf8 content>"),
        "parse_config_file"
    );
    let value = match format {
        FileFormat::Json => serde_json::from_slice(&bytes)
            .with_context(|| format!("error parsing {:?}", path.as_ref()))?,
        FileFormat::Yaml => serde_yaml::from_slice(&bytes)
            .with_context(|| format!("error parsing {:?}", path.as_ref()))?,
    };
    Ok(value)
}

async fn write_subdir_configs<T>(
    subdir: &Path,
    configs: impl IntoIterator<Item = (String, T)>,
) -> anyhow::Result<()>
where
    T: Serialize,
{
    if !(fs::try_exists(subdir).await?) {
        fs::create_dir(subdir).await?;
    }

    for (name, config) in configs {
        let with_name: WithName<String, T> = (name.clone(), config).into();
        write_file(subdir, &name, &with_name).await?;
    }

    Ok(())
}

pub async fn write_schema_directory(
    configuration_dir: impl AsRef<Path>,
    schemas: impl IntoIterator<Item = (String, Schema)>,
) -> anyhow::Result<()> {
    let subdir = configuration_dir.as_ref().join(SCHEMA_DIRNAME);
    write_subdir_configs(&subdir, schemas).await
}

fn default_file_path(configuration_dir: impl AsRef<Path>, basename: &str) -> PathBuf {
    let dir = configuration_dir.as_ref();
    dir.join(format!("{basename}.{DEFAULT_EXTENSION}"))
}

async fn write_file<T>(
    configuration_dir: impl AsRef<Path>,
    basename: &str,
    value: &T,
) -> anyhow::Result<()>
where
    T: Serialize,
{
    let path = default_file_path(configuration_dir, basename);
    let bytes = serde_json::to_vec_pretty(value)?;

    // Don't write the file if it hasn't changed.
    if let Ok(existing_bytes) = fs::read(&path).await {
        if bytes == existing_bytes {
            return Ok(());
        }
    }
    fs::write(&path, bytes)
        .await
        .with_context(|| format!("error writing {:?}", path))
}

// Read schemas with a separate map entry for each configuration file.
pub async fn read_existing_schemas(
    configuration_dir: impl AsRef<Path>,
) -> anyhow::Result<BTreeMap<CollectionName, CollectionSchema>> {
    let dir = configuration_dir.as_ref();

    let schemas = read_subdir_configs::<String, Schema>(&dir.join(SCHEMA_DIRNAME), &[])
        .await?
        .unwrap_or_default();

    // Get a single collection schema out of each file
    let schemas = schemas
        .into_iter()
        .flat_map(|(name, schema)| {
            let mut collections = schema.collections.into_iter().collect_vec();
            let (collection_name, collection) = collections.pop()?;
            if !collections.is_empty() {
                return Some(Err(anyhow!("found schemas for multiple collections in {SCHEMA_DIRNAME}/{name}.json - please limit schema configurations to one collection per file")));
            }
            Some(Ok((collection_name, CollectionSchema {
                collection,
                object_types: schema.object_types,
            })))
        })
        .collect::<anyhow::Result<BTreeMap<CollectionName, CollectionSchema>>>()?;

    Ok(schemas)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use async_tempfile::TempDir;
    use googletest::prelude::*;
    use mongodb_support::BsonScalarType;
    use ndc_models::FunctionName;
    use serde_json::json;
    use tokio::fs;

    use crate::{
        native_query::NativeQuery,
        read_directory_with_ignored_configs,
        schema::{ObjectField, ObjectType, Type},
        serialized, WithName, NATIVE_QUERIES_DIRNAME,
    };

    use super::{read_directory, CONFIGURATION_OPTIONS_BASENAME};

    #[googletest::test]
    #[tokio::test]
    async fn errors_on_typo_in_extended_json_mode_string() -> Result<()> {
        let input = json!({
            "introspectionOptions": {
                "sampleSize": 1_000,
                "noValidatorSchema": true,
                "allSchemaNullable": false,
            },
            "serializationOptions": {
                "extendedJsonMode": "no-such-mode",
            },
        });

        let config_dir = TempDir::new().await?;
        let mut config_file = config_dir.join(CONFIGURATION_OPTIONS_BASENAME);
        config_file.set_extension("json");
        fs::write(config_file, serde_json::to_vec(&input)?).await?;

        let actual = read_directory(config_dir).await;

        expect_that!(
            actual,
            err(predicate(|e: &anyhow::Error| e
                .root_cause()
                .to_string()
                .contains("unknown variant `no-such-mode`")))
        );

        Ok(())
    }

    #[googletest::test]
    #[tokio::test]
    async fn ignores_specified_config_files() -> anyhow::Result<()> {
        let native_query = WithName {
            name: "hello".to_string(),
            value: serialized::NativeQuery {
                representation: crate::native_query::NativeQueryRepresentation::Function,
                input_collection: None,
                arguments: Default::default(),
                result_document_type: "Hello".into(),
                object_types: [(
                    "Hello".into(),
                    ObjectType {
                        fields: [(
                            "__value".into(),
                            ObjectField {
                                r#type: Type::Scalar(BsonScalarType::String),
                                description: None,
                            },
                        )]
                        .into(),
                        description: None,
                    },
                )]
                .into(),
                pipeline: [].into(),
                description: None,
            },
        };

        let config_dir = TempDir::new().await?;
        tokio::fs::create_dir(config_dir.join(NATIVE_QUERIES_DIRNAME)).await?;
        let native_query_path = PathBuf::from(NATIVE_QUERIES_DIRNAME).join("hello.json");
        fs::write(
            config_dir.join(&native_query_path),
            serde_json::to_vec(&native_query)?,
        )
        .await?;

        let parsed_config = read_directory(&config_dir).await?;
        let parsed_config_ignoring_native_query =
            read_directory_with_ignored_configs(config_dir, &[native_query_path]).await?;

        expect_that!(
            parsed_config.native_queries,
            unordered_elements_are!(eq((
                &FunctionName::from("hello"),
                &NativeQuery::from_serialized(&Default::default(), native_query.value)?
            ))),
        );

        expect_that!(parsed_config_ignoring_native_query.native_queries, empty());

        Ok(())
    }
}
