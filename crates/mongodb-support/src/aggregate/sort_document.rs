use mongodb::bson;
use serde::{Deserialize, Serialize};

/// Wraps a BSON document that represents a set of sort criteria. A SortDocument value is intended
/// to be used as the argument to a $sort pipeline stage.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct SortDocument(pub bson::Document);

impl SortDocument {
    pub fn from_doc(doc: bson::Document) -> Self {
        SortDocument(doc)
    }
}
