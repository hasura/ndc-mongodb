use std::collections::{BTreeMap, HashSet};

use super::{inference::make_object_type, type_unification::unify_object_types};
use configuration::{
    schema,
    Schema, WithName,
};
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb_agent_common::interface_types::MongoConfig;


/// Sample from all collections in the database and return a Schema.
/// Return an error if there are any errors accessing the database
/// or if the types derived from the sample documents for a collection
/// are not unifiable.
pub async fn sample_schema_from_db(
    sample_size: u32,
    config: &MongoConfig,
    existing_schemas: &HashSet<std::string::String>,
) -> anyhow::Result<BTreeMap<std::string::String, Schema>> {
    let mut schemas = BTreeMap::new();
    let db = config.client.database(&config.database);
    let mut collections_cursor = db.list_collections(None, None).await?;

    while let Some(collection_spec) = collections_cursor.try_next().await? {
        let collection_name = collection_spec.name;
        if !existing_schemas.contains(&collection_name) {
            let collection_schema =
                sample_schema_from_collection(&collection_name, sample_size, config).await?;
            schemas.insert(collection_name, collection_schema);
        }
    }
    Ok(schemas)
}

async fn sample_schema_from_collection(
    collection_name: &str,
    sample_size: u32,
    config: &MongoConfig,
) -> anyhow::Result<Schema> {
    let db = config.client.database(&config.database);
    let options = None;
    let mut cursor = db
        .collection::<Document>(collection_name)
        .aggregate(vec![doc! {"$sample": { "size": sample_size }}], options)
        .await?;
    let mut collected_object_types = vec![];
    while let Some(document) = cursor.try_next().await? {
        let object_types = make_object_type(collection_name, &document);
        collected_object_types = if collected_object_types.is_empty() {
            object_types
        } else {
            unify_object_types(collected_object_types, object_types)
        };
    }
    let collection_info = WithName::named(
        collection_name.to_string(),
        schema::Collection {
            description: None,
            r#type: collection_name.to_string(),
        },
    );

    Ok(Schema {
        collections: WithName::into_map([collection_info]),
        object_types: WithName::into_map(collected_object_types),
    })
}
