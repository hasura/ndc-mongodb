use std::collections::BTreeMap;

use configuration::{native_queries::NativeQuery, schema::ObjectType};
use mongodb::Client;

#[derive(Clone, Debug)]
pub struct MongoConfig {
    pub client: Client,

    /// Name of the database to connect to
    pub database: String,

    pub native_queries: BTreeMap<String, NativeQuery>,
    pub object_types: BTreeMap<String, ObjectType>,
}
