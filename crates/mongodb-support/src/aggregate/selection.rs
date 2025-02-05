use mongodb::bson::{self, Bson};
use serde::{Deserialize, Serialize};

/// Wraps a BSON document that represents a MongoDB "expression" that constructs a document based
/// on the output of a previous aggregation pipeline stage. A Selection value is intended to be
/// used as the argument to a $replaceWith pipeline stage.
///
/// When we compose pipelines, we can pair each Pipeline with a Selection that extracts the data we
/// want, in the format we want it to provide to HGE. We can collect Selection values and merge
/// them to form one stage after all of the composed pipelines.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Selection(bson::Document);

impl Selection {
    pub fn new(doc: bson::Document) -> Self {
        Self(doc)
    }

    /// Transform the contained BSON document in a callback. This may return an error on invariant
    /// violations in the future.
    pub fn try_map_document<F>(self, callback: F) -> Result<Self, anyhow::Error>
    where
        F: FnOnce(bson::Document) -> bson::Document,
    {
        let doc = self.into();
        let updated_doc = callback(doc);
        Ok(Self::new(updated_doc))
    }
}

/// The extend implementation provides a shallow merge.
impl Extend<(String, Bson)> for Selection {
    fn extend<T: IntoIterator<Item = (String, Bson)>>(&mut self, iter: T) {
        self.0.extend(iter);
    }
}

impl From<Selection> for Bson {
    fn from(value: Selection) -> Self {
        value.0.into()
    }
}

impl From<Selection> for bson::Document {
    fn from(value: Selection) -> Self {
        value.0
    }
}

impl<'a> From<&'a Selection> for &'a bson::Document {
    fn from(value: &'a Selection) -> Self {
        &value.0
    }
}

// This won't fail, but it might in the future if we add some sort of validation or parsing.
impl TryFrom<bson::Document> for Selection {
    type Error = anyhow::Error;
    fn try_from(value: bson::Document) -> Result<Self, Self::Error> {
        Ok(Selection(value))
    }
}
