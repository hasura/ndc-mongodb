use anyhow::{anyhow, Context as _};
use futures::stream::TryStreamExt as _;
use itertools::Itertools as _;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use tokio::fs;
use tokio_stream::wrappers::ReadDirStream;

use crate::{with_name::WithName, Configuration, Schema};

pub const SCHEMA_DIRNAME: &str = "schema";
pub const NATIVE_QUERIES_DIRNAME: &str = "native_queries";

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

    let schemas = read_subdir_configs(&dir.join(SCHEMA_DIRNAME)).await?.unwrap_or_default();
    let schema = schemas.into_values().fold(Schema::default(), Schema::merge);

    let native_queries = read_subdir_configs(&dir.join(NATIVE_QUERIES_DIRNAME))
        .await?
        .unwrap_or_default();

    Configuration::validate(schema, native_queries)
}

/// Parse all files in a directory with one of the allowed configuration extensions according to
/// the given type argument. For example if `T` is `NativeQuery` this function assumes that all
/// json and yaml files in the given directory should be parsed as native query configurations.
///
/// Assumes that every configuration file has a `name` field.
async fn read_subdir_configs<T>(subdir: &Path) -> anyhow::Result<Option<BTreeMap<String, T>>>
where
    for<'a> T: Deserialize<'a>,
{
    if !(fs::try_exists(subdir).await?) {
        return Ok(None);
    }

    let dir_stream = ReadDirStream::new(fs::read_dir(subdir).await?);
    let configs: Vec<WithName<T>> = dir_stream
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
        .and_then(
            |(path, format)| async move { parse_config_file::<WithName<T>>(path, format).await },
        )
        .try_collect()
        .await?;

    let duplicate_names = configs
        .iter()
        .map(|c| c.name.as_ref())
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

/// Given a base name, like "connection", looks for files of the form "connection.json",
/// "connection.yaml", etc; reads the file; and parses it according to its extension.
async fn parse_json_or_yaml<T>(configuration_dir: &Path, basename: &str) -> anyhow::Result<T>
where
    for<'a> T: Deserialize<'a>,
{
    let (path, format) = find_file(configuration_dir, basename).await?;
    parse_config_file(path, format).await
}

/// Given a base name, like "connection", looks for files of the form "connection.json",
/// "connection.yaml", etc, and returns the found path with its file format.
async fn find_file(
    configuration_dir: &Path,
    basename: &str,
) -> anyhow::Result<(PathBuf, FileFormat)> {
    for (extension, format) in CONFIGURATION_EXTENSIONS {
        let path = configuration_dir.join(format!("{basename}.{extension}"));
        if fs::try_exists(&path).await? {
            return Ok((path, format));
        }
    }

    Err(anyhow!(
        "could not find file, {:?}",
        configuration_dir.join(format!(
            "{basename}.{{{}}}",
            CONFIGURATION_EXTENSIONS
                .into_iter()
                .map(|(ext, _)| ext)
                .join(",")
        ))
    ))
}

async fn parse_config_file<T>(path: impl AsRef<Path>, format: FileFormat) -> anyhow::Result<T>
where
    for<'a> T: Deserialize<'a>,
{
    let bytes = fs::read(path.as_ref()).await?;
    let value = match format {
        FileFormat::Json => serde_json::from_slice(&bytes)
            .with_context(|| format!("error parsing {:?}", path.as_ref()))?,
        FileFormat::Yaml => serde_yaml::from_slice(&bytes)
            .with_context(|| format!("error parsing {:?}", path.as_ref()))?,
    };
    Ok(value)
}

async fn write_subdir_configs<T>(subdir: &Path, configs: impl IntoIterator<Item = (String, T)>) -> anyhow::Result<()>
where
    T: Serialize,
{
    if !(fs::try_exists(subdir).await?) {
        fs::create_dir(subdir).await?;
    }

    for (name, config) in configs {
        let with_name: WithName<T> = (name.clone(), config).into();
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

// /// Currently only writes `schema.json`
// pub async fn write_directory(
//     configuration_dir: impl AsRef<Path>,
//     configuration: &Configuration,
// ) -> anyhow::Result<()> {
//     write_file(configuration_dir, SCHEMA_FILENAME, &configuration.schema).await
// }

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
    fs::write(path.clone(), bytes)
        .await
        .with_context(|| format!("error writing {:?}", path))
}
