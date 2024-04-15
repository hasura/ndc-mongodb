use std::{borrow::Cow, collections::BTreeMap};

use configuration::{native_query::NativeQuery, schema::ObjectType};

use crate::interface_types::MongoConfig;

/// Subset of MongoConfig that excludes database connection and connection string.
#[derive(Clone, Debug)]
pub struct QueryConfig<'a> {
    pub native_queries: Cow<'a, BTreeMap<String, NativeQuery>>,
    pub object_types: Cow<'a, BTreeMap<String, ObjectType>>,
}

impl Default for QueryConfig<'static> {
    fn default() -> Self {
        Self {
            native_queries: Cow::Owned(Default::default()),
            object_types: Cow::Owned(Default::default()),
        }
    }
}

impl<'a> From<&'a MongoConfig> for QueryConfig<'a> {
    fn from(configuration: &'a MongoConfig) -> Self {
        QueryConfig {
            native_queries: Cow::Borrowed(&configuration.native_queries),
            object_types: Cow::Borrowed(&configuration.object_types),
        }
    }
}
