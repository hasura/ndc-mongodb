use std::borrow::Cow;

use anyhow::anyhow;
use dc_api_types::comparison_column::ColumnSelector;
use mongodb::bson::{doc, Document};
use once_cell::sync::Lazy;
use regex::Regex;

use crate::interface_types::MongoAgentError;

/// Produces a MongoDB expression that references a field by name in a way that is safe from code
/// injection.
pub fn get_field(name: &str) -> Document {
    doc! { "$getField": { "$literal": name } }
}

/// Returns its input prefixed with "v_" if it is a valid MongoDB variable name. Valid names may
/// include the ASCII characters [_a-zA-Z0-9] or any non-ASCII characters. The exclusion of special
/// characters like `$` and `.` avoids potential code injection.
///
/// We add the "v_" prefix because variable names may not begin with an underscore, but in some
/// cases, like when using relation-mapped column names as variable names, we want to be able to
/// use names like "_id".
///
/// TODO: Instead of producing an error we could use an escaping scheme to unambiguously map
/// invalid characters to safe ones.
pub fn variable(name: &str) -> Result<String, MongoAgentError> {
    static VALID_EXPRESSION: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^[_a-zA-Z0-9\P{ascii}]+$").unwrap());
    if VALID_EXPRESSION.is_match(name) {
        Ok(format!("v_{name}"))
    } else {
        Err(MongoAgentError::InvalidVariableName(name.to_owned()))
    }
}

/// Given a collection or field name, returns Ok if the name is safe, or Err if it contains
/// characters that MongoDB will interpret specially.
///
/// TODO: Can we handle names with dots or dollar signs safely instead of throwing an error?
pub fn safe_name(name: &str) -> Result<Cow<str>, MongoAgentError> {
    if name.starts_with('$') || name.contains('.') {
        Err(MongoAgentError::BadQuery(anyhow!("cannot execute query that includes the name, \"{name}\", because it includes characters that MongoDB interperets specially")))
    } else {
        Ok(Cow::Borrowed(name))
    }
}

pub fn safe_column_selector(column_selector: &ColumnSelector) -> Result<Cow<str>, MongoAgentError> {
    match column_selector {
        ColumnSelector::Path(p) => p
            .iter()
            .map(|s| safe_name(s))
            .collect::<Result<Vec<Cow<str>>, MongoAgentError>>()
            .map(|v| Cow::Owned(v.join("."))),
        ColumnSelector::Column(c) => safe_name(c),
    }
}
