use std::path::Path;

use configuration::{schema, Schema, WithName};
use mongodb::bson::Document;
use tokio::fs;

use super::{inference::make_object_type, type_unification::unify_object_types};

type ObjectType = WithName<schema::ObjectType>;

pub async fn schema_from_directory(
    collection_name: &str,
    dir_path: impl AsRef<Path>,
) -> anyhow::Result<Schema> {
    let mut collected_object_types = vec![];
    let mut rd = fs::read_dir(dir_path.as_ref()).await?;
    while let Some(dir_entry) = rd.next_entry().await? {
        let object_types = object_types_from_json_file(collection_name, dir_entry.path()).await?;
        collected_object_types = if collected_object_types.is_empty() {
            object_types
        } else {
            unify_object_types(collected_object_types, object_types)
        }
    }

    let collection_info = WithName::named(
        collection_name.to_owned(),
        schema::Collection {
            description: None,
            r#type: collection_name.to_owned(),
        },
    );

    let schema = Schema {
        collections: WithName::into_map([collection_info]),
        object_types: WithName::into_map(collected_object_types),
    };
    Ok(schema)
}

async fn object_types_from_json_file(
    collection_name: &str,
    file_path: impl AsRef<Path>,
) -> anyhow::Result<Vec<ObjectType>> {
    let bytes = fs::read(file_path.as_ref()).await?;
    let document: Document = serde_json::from_slice(&bytes)?;

    Ok(make_object_type(collection_name, &document))
}
