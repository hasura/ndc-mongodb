use configuration::{
    metadata::{Collection, ObjectField, ObjectType, Type},
    Metadata,
};
use mongodb::bson::{Bson, Document};
use mongodb_agent_common::interface_types::{MongoAgentError, MongoConfig};
use mongodb_support::{BsonScalarType, BsonType};

pub fn schema_from_document(collection_name: &str, document: &Document) -> Metadata {
  let (object_types, collection) = make_collection(collection_name, document);
  Metadata { collections: vec!(collection), object_types}
}

fn make_collection(collection_name: &str, document: &Document) -> (Vec<ObjectType>, Collection) {
  todo!()
}