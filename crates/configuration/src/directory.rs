use anyhow::{anyhow, Context as _};
use futures::stream::TryStreamExt as _;
use itertools::Itertools as _;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs::Metadata,
    path::{Path, PathBuf},
};
use tokio::{fs, io::AsyncWriteExt};
use tokio_stream::wrappers::ReadDirStream;

use crate::{
    configuration::ConfigurationOptions, serialized::Schema, with_name::WithName, Configuration,
};

pub const SCHEMA_DIRNAME: &str = "schema";
pub const NATIVE_MUTATIONS_DIRNAME: &str = "native_mutations";
pub const NATIVE_QUERIES_DIRNAME: &str = "native_queries";
pub const CONFIGURATION_OPTIONS_BASENAME: &str = "configuration";
pub const CONFIGURATION_OPTIONS_METADATA: &str = ".configuration_metadata";

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
    let dir = configuration_dir.as_ref();

    let schemas = read_subdir_configs::<String, Schema>(&dir.join(SCHEMA_DIRNAME))
        .await?
        .unwrap_or_default();
    let schema = schemas.into_values().fold(Schema::default(), Schema::merge);

    // Deprecated see message above at NATIVE_PROCEDURES_DIRNAME
    let native_procedures = read_subdir_configs(&dir.join(NATIVE_PROCEDURES_DIRNAME))
        .await?
        .unwrap_or_default();

    // TODO: Once we fully remove `native_procedures` after a deprecation period we can remove `mut`
    let mut native_mutations = read_subdir_configs(&dir.join(NATIVE_MUTATIONS_DIRNAME))
        .await?
        .unwrap_or_default();

    let native_queries = read_subdir_configs(&dir.join(NATIVE_QUERIES_DIRNAME))
        .await?
        .unwrap_or_default();

    let options = parse_configuration_options_file(dir).await;

    native_mutations.extend(native_procedures.into_iter());

    Configuration::validate(schema, native_mutations, native_queries, options)
}

/// Parse all files in a directory with one of the allowed configuration extensions according to
/// the given type argument. For example if `T` is `NativeMutation` this function assumes that all
/// json and yaml files in the given directory should be parsed as native mutation configurations.
///
/// Assumes that every configuration file has a `name` field.
async fn read_subdir_configs<N, T>(subdir: &Path) -> anyhow::Result<Option<BTreeMap<N, T>>>
where
    for<'a> T: Deserialize<'a>,
    for<'a> N: Ord + ToString + Deserialize<'a>,
{
    if !(fs::try_exists(subdir).await?) {
        return Ok(None);
    }

    let dir_stream = ReadDirStream::new(fs::read_dir(subdir).await?);
    let configs: Vec<WithName<N, T>> = dir_stream
        .map_err(|err| err.into())
        .try_filter_map(|dir_entry| async move {
            // Permits regular files and symlinks, does not filter out symlinks to directories.
            let is_file = !(dir_entry.file_type().await?.is_dir());
            if !is_file {
                return Ok(None);
            }

            let path = dir_entry.path();
            let extension = path.extension().and_then(|ext| ext.to_str());

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
            parse_config_file::<WithName<N, T>>(path, format).await
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

pub async fn parse_configuration_options_file(dir: &Path) -> ConfigurationOptions {
    let json_filename = CONFIGURATION_OPTIONS_BASENAME.to_owned() + ".json";
    let json_config_file = parse_config_file(&dir.join(json_filename), JSON).await;
    if let Ok(config_options) = json_config_file {
        return config_options;
    }

    let yaml_filename = CONFIGURATION_OPTIONS_BASENAME.to_owned() + ".yaml";
    let yaml_config_file = parse_config_file(&dir.join(yaml_filename), YAML).await;
    if let Ok(config_options) = yaml_config_file {
        return config_options;
    }

    // If a configuration file does not exist use defaults and write the file
    let defaults: ConfigurationOptions = Default::default();
    let _ = write_file(dir, CONFIGURATION_OPTIONS_BASENAME, &defaults).await;
    let _ = write_config_metadata_file(dir).await;
    defaults
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

pub async fn list_existing_schemas(
    configuration_dir: impl AsRef<Path>,
) -> anyhow::Result<HashSet<String>> {
    let dir = configuration_dir.as_ref();

    // TODO: we don't really need to read and parse all the schema files here, just get their names.
    let schemas = read_subdir_configs::<_, Schema>(&dir.join(SCHEMA_DIRNAME))
        .await?
        .unwrap_or_default();

    Ok(schemas.into_keys().collect())
}

// Metadata file is just a dot filed used for the purposes of know if the user has updated their config to force refresh
// of the schema introspection.
async fn write_config_metadata_file(configuration_dir: impl AsRef<Path>) {
    let dir = configuration_dir.as_ref();
    let file_result = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(dir.join(CONFIGURATION_OPTIONS_METADATA))
        .await;

    if let Ok(mut file) = file_result {
        let _ = file.write_all(b"").await;
    };
}

pub async fn get_config_file_changed(dir: impl AsRef<Path>) -> anyhow::Result<bool> {
    let path = dir.as_ref();
    let dot_metadata: Result<Metadata, std::io::Error> =
        fs::metadata(&path.join(CONFIGURATION_OPTIONS_METADATA)).await;
    let json_metadata =
        fs::metadata(&path.join(CONFIGURATION_OPTIONS_BASENAME.to_owned() + ".json")).await;
    let yaml_metadata =
        fs::metadata(&path.join(CONFIGURATION_OPTIONS_BASENAME.to_owned() + ".yaml")).await;

    let compare = |dot_date, config_date| async move {
        if dot_date < config_date {
            let _ = write_config_metadata_file(path).await;
            Ok(true)
        } else {
            Ok(false)
        }
    };

    match (dot_metadata, json_metadata, yaml_metadata) {
        (Ok(dot), Ok(json), _) => compare(dot.modified()?, json.modified()?).await,
        (Ok(dot), _, Ok(yaml)) => compare(dot.modified()?, yaml.modified()?).await,
        _ => Ok(true),
    }
}
