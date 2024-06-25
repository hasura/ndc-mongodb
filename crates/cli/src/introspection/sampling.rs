use std::collections::{BTreeMap, HashSet};

use crate::log_warning;

use super::{inference::{make_object_type_for_collection}, type_unification::unify_object_types};
use configuration::{
    schema,
    Schema, WithName,
};
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb_agent_common::state::ConnectorState;



/// Sample from all collections in the database and return a Schema.
/// Return an error if there are any errors accessing the database
/// or if the types derived from the sample documents for a collection
/// are not unifiable.
pub async fn sample_schema_from_db(
    sample_size: u32,
    all_schema_nullable: bool,
    config_file_changed: bool,
    state: &ConnectorState,
    existing_schemas: &HashSet<std::string::String>,
) -> anyhow::Result<BTreeMap<std::string::String, Schema>> {
    let mut schemas = BTreeMap::new();
    let db = state.database();
    let mut collections_cursor = db.list_collections(None, None).await?;

    while let Some(collection_spec) = collections_cursor.try_next().await? {
        let collection_name = collection_spec.name;
        if !existing_schemas.contains(&collection_name) || config_file_changed {
            let collection_schema = sample_schema_from_collection(
                &collection_name,
                sample_size,
                all_schema_nullable,
                state,
            )
            .await?;
            if let Some(collection_schema) = collection_schema {
                schemas.insert(collection_name, collection_schema);
            } else {
                log_warning!("could not find any documents to sample from collection, {collection_name} - skipping");
            }
        }
    }
    Ok(schemas)
}

async fn sample_schema_from_collection(
    collection_name: &str,
    sample_size: u32,
    all_schema_nullable: bool,
    state: &ConnectorState,
) -> anyhow::Result<Option<Schema>> {
    let db = state.database();
    let options = None;
    let mut cursor = db
        .collection::<Document>(collection_name)
        .aggregate(vec![doc! {"$sample": { "size": sample_size }}], options)
        .await?;
    let mut collected_object_types = vec![];
    while let Some(document) = cursor.try_next().await? {
        let object_types = make_object_type_for_collection(
            collection_name,
            &document,
            all_schema_nullable,
        );
        collected_object_types = if collected_object_types.is_empty() {
            object_types
        } else {
            unify_object_types(collected_object_types, object_types)
        };
    }
    if collected_object_types.is_empty() {
        Ok(None)
    } else {
        let collection_info = WithName::named(
            collection_name.to_string(),
            schema::Collection {
                description: None,
                r#type: collection_name.to_string(),
            },
        );
        Ok(Some(Schema {
            collections: WithName::into_map([collection_info]),
            object_types: WithName::into_map(collected_object_types),
        }))
    }
}
