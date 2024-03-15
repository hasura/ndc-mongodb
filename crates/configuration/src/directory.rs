use itertools::Itertools as _;
use serde::Deserialize;
use std::{
    io,
    path::{Path, PathBuf},
};
use tokio::fs;

use crate::Configuration;

pub const CONFIGURATION_FILENAME: &str = "schema";
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
) -> io::Result<Configuration> {
    parse_file(configuration_dir, CONFIGURATION_FILENAME).await
}

/// Given a base name, like "connection", looks for files of the form "connection.json",
/// "connection.yaml", etc; reads the file; and parses it according to its extension.
async fn parse_file<T>(configuration_dir: impl AsRef<Path>, basename: &str) -> io::Result<T>
where
    for<'a> T: Deserialize<'a>,
{
    let (path, format) = find_file(configuration_dir, basename).await?;
    read_file(path, format).await
}

/// Given a base name, like "connection", looks for files of the form "connection.json",
/// "connection.yaml", etc, and returns the found path with its file format.
async fn find_file(
    configuration_dir: impl AsRef<Path>,
    basename: &str,
) -> io::Result<(PathBuf, FileFormat)> {
    let dir = configuration_dir.as_ref();

    for (extension, format) in CONFIGURATION_EXTENSIONS {
        let path = dir.join(format!("{basename}.{extension}"));
        if fs::try_exists(&path).await? {
            return Ok((path, format));
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "could not find file, {:?}",
            dir.join(format!(
                "{basename}.{{{}}}",
                CONFIGURATION_EXTENSIONS
                    .into_iter()
                    .map(|(ext, _)| ext)
                    .join(",")
            ))
        ),
    ))
}

async fn read_file<T>(path: impl AsRef<Path>, format: FileFormat) -> io::Result<T>
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

pub async fn write_directory(
    configuration_dir: impl AsRef<Path>,
    configuration: &Configuration,
) -> io::Result<()> {
    write_file(configuration_dir, CONFIGURATION_FILENAME, configuration).await
}

fn default_file_path(configuration_dir: impl AsRef<Path>, basename: &str) -> PathBuf {
    let dir = configuration_dir.as_ref();
    dir.join(format!("{basename}.{DEFAULT_EXTENSION}"))
}

async fn write_file(
    configuration_dir: impl AsRef<Path>,
    basename: &str,
    configuration: &Configuration,
) -> io::Result<()> {
    let path = default_file_path(configuration_dir, basename);
    let bytes = serde_json::to_vec_pretty(configuration)?;
    fs::write(path, bytes).await
}
