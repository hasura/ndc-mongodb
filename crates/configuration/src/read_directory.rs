use futures::stream::TryStreamExt as _;
use itertools::Itertools as _;
use serde::Deserialize;
use std::{
    io,
    path::{Path, PathBuf},
};
use tokio::fs;
use tokio_stream::wrappers::ReadDirStream;

use crate::{native_queries::NativeQuery, Configuration};

pub const SCHEMA_FILENAME: &str = "schema";
pub const NATIVE_QUERIES_DIRNAME: &str = "native_queries";

pub const CONFIGURATION_EXTENSIONS: [(&str, FileFormat); 3] =
    [("json", JSON), ("yaml", YAML), ("yml", YAML)];

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
) -> io::Result<Configuration> {
    let dir = configuration_dir.as_ref();

    let schema = parse_json_or_yaml(dir, SCHEMA_FILENAME).await?;

    let native_queries: Vec<NativeQuery> =
        read_subdir_configs(&dir.join(NATIVE_QUERIES_DIRNAME)).await?;

    Ok(Configuration {
        schema,
        native_queries,
    })
}

/// Parse all files in a directory with one of the allowed configuration extensions according to
/// the given type argument. For example if `T` is `NativeQuery` this function assumes that all
/// json and yaml files in the given directory should be parsed as native query configurations.
async fn read_subdir_configs<T>(subdir: &Path) -> io::Result<Vec<T>>
where
    for<'a> T: Deserialize<'a>,
{
    let dir_stream = ReadDirStream::new(fs::read_dir(subdir).await?);
    dir_stream
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
        .and_then(|(path, format)| async move { parse_config_file::<T>(path, format).await })
        .try_collect::<Vec<T>>()
        .await
}

/// Given a base name, like "connection", looks for files of the form "connection.json",
/// "connection.yaml", etc; reads the file; and parses it according to its extension.
async fn parse_json_or_yaml<T>(configuration_dir: &Path, basename: &str) -> io::Result<T>
where
    for<'a> T: Deserialize<'a>,
{
    let (path, format) = find_file(configuration_dir, basename).await?;
    parse_config_file(path, format).await
}

/// Given a base name, like "connection", looks for files of the form "connection.json",
/// "connection.yaml", etc, and returns the found path with its file format.
async fn find_file(configuration_dir: &Path, basename: &str) -> io::Result<(PathBuf, FileFormat)> {
    for (extension, format) in CONFIGURATION_EXTENSIONS {
        let path = configuration_dir.join(format!("{basename}.{extension}"));
        if fs::try_exists(&path).await? {
            return Ok((path, format));
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "could not find file, {:?}",
            configuration_dir.join(format!(
                "{basename}.{{{}}}",
                CONFIGURATION_EXTENSIONS
                    .into_iter()
                    .map(|(ext, _)| ext)
                    .join(",")
            ))
        ),
    ))
}

async fn parse_config_file<T>(path: impl AsRef<Path>, format: FileFormat) -> io::Result<T>
where
    for<'a> T: Deserialize<'a>,
{
    let bytes = fs::read(path.as_ref()).await?;
    let value = match format {
        FileFormat::Json => serde_json::from_slice(&bytes)?,
        FileFormat::Yaml => serde_yaml::from_slice(&bytes)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?,
    };
    Ok(value)
}
