use std::collections::BTreeMap;

use configuration::{native_query::NativeQuery, schema::ObjectType};

use crate::interface_types::MongoConfig;

/// Subset of MongoConfig that excludes database connection and connection string.
#[derive(Clone, Copy, Debug)]
pub struct QueryConfig<'a> {
    pub native_queries: &'a BTreeMap<String, NativeQuery>,
    pub object_types: &'a BTreeMap<String, ObjectType>,
}

impl Default for QueryConfig<'static> {
    fn default() -> Self {
        static NATIVE_QUERIES: BTreeMap<String, NativeQuery> = BTreeMap::new();
        static OBJECT_TYPES: BTreeMap<String, ObjectType> = BTreeMap::new();
        Self {
            native_queries: &NATIVE_QUERIES,
            object_types: &OBJECT_TYPES,
        }
    }
}

impl<'a> From<&'a MongoConfig> for QueryConfig<'a> {
    fn from(configuration: &'a MongoConfig) -> Self {
        QueryConfig {
            native_queries: &configuration.native_queries,
            object_types: &configuration.object_types,
        }
    }
}
