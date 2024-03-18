use configuration::{
    schema::{Collection, ObjectField, ObjectType, Type},
    Schema,
};
use mongodb::bson::{Bson, Document};
use mongodb_agent_common::interface_types::{MongoAgentError, MongoConfig};
use mongodb_support::{BsonScalarType, BsonType};

pub fn schema_from_document(collection_name: &str, document: &Document) -> Schema {
  let (object_types, collection) = make_collection(collection_name, document);
  Schema { collections: vec!(collection), object_types}
}

fn make_collection(collection_name: &str, document: &Document) -> (Vec<ObjectType>, Collection) {
  todo!()
}