use std::{borrow::Cow, collections::BTreeMap};

use crate::{
    api_type_conversions::{ConversionError, QueryContext},
    schema::SCALAR_TYPES,
};
use configuration::{
    native_query::{NativeQuery, NativeQueryRepresentation},
    schema as config, Configuration,
};
use mongodb_support::EXTENDED_JSON_TYPE_NAME;
use ndc_sdk::models::{self as ndc, ArgumentInfo, FunctionInfo};

/// Produce a query context from the connector configuration to direct query request processing
pub fn get_query_context(configuration: &Configuration) -> QueryContext<'_> {
    QueryContext {
        collections: Cow::Borrowed(&configuration.collections),
        functions: Cow::Borrowed(&configuration.functions),
        object_types: Cow::Borrowed(&configuration.object_types),
        scalar_types: Cow::Borrowed(&SCALAR_TYPES),
    }
}
