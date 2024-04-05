use std::path::Path;

use configuration::{schema, Schema, WithName};
use mongodb::bson::Document;
use tokio::fs;

use super::inference::make_object_type;

pub async fn schema_from_json_file(collection_name: &str, file_path: impl AsRef<Path>) -> anyhow::Result<Schema> {
  let bytes = fs::read(file_path.as_ref()).await?;
  let document: Document = serde_json::from_slice(&bytes)?;

  let object_types = make_object_type(collection_name, &document);
  let collection_info = WithName::named(
    collection_name.to_owned(),
    schema::Collection {
      description: None,
      r#type: collection_name.to_owned(),
    }
  );

  let schema = Schema {
    collections: WithName::into_map([collection_info]),
    object_types: WithName::into_map(object_types)
  };
  Ok(schema)
}