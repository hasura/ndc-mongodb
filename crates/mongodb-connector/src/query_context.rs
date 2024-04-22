use std::borrow::Cow;

use crate::{api_type_conversions::QueryContext, schema::SCALAR_TYPES};
use configuration::Configuration;

/// Produce a query context from the connector configuration to direct query request processing
pub fn get_query_context(configuration: &Configuration) -> QueryContext<'_> {
    QueryContext {
        collections: Cow::Borrowed(&configuration.collections),
        functions: Cow::Borrowed(&configuration.functions),
        object_types: Cow::Borrowed(&configuration.object_types),
        scalar_types: Cow::Borrowed(&SCALAR_TYPES),
    }
}
