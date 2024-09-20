use std::{borrow::Borrow, ops::Deref};

use mongodb::bson;
use serde::{Deserialize, Serialize};

use super::stage::Stage;

/// Aggregation Pipeline
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Pipeline {
    pub stages: Vec<Stage>,
}

impl Pipeline {
    pub fn new(stages: Vec<Stage>) -> Pipeline {
        Pipeline { stages }
    }

    pub fn append(&mut self, mut other: Pipeline) {
        self.stages.append(&mut other.stages);
    }

    pub fn empty() -> Pipeline {
        Pipeline { stages: vec![] }
    }

    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }

    pub fn push(&mut self, stage: Stage) {
        self.stages.push(stage);
    }
}

impl AsRef<[Stage]> for Pipeline {
    fn as_ref(&self) -> &[Stage] {
        &self.stages
    }
}

impl Borrow<[Stage]> for Pipeline {
    fn borrow(&self) -> &[Stage] {
        &self.stages
    }
}

impl Deref for Pipeline {
    type Target = [Stage];

    fn deref(&self) -> &Self::Target {
        &self.stages
    }
}

/// This impl allows passing a [Pipeline] as the first argument to [mongodb::Collection::aggregate].
impl IntoIterator for Pipeline {
    type Item = bson::Document;

    // Using a dynamically-dispatched boxed type here because the concrete type that Iterator::map
    // produces includes a closure which cannot be named in code. The `dyn` keyword lets us use
    // a trait as a type with the caveat that its methods will be dispatched dynamically instead of
    // the usual static dispatch. Types that use `dyn` have to be boxed because they don't have
    // a statically-known size.
    type IntoIter = Box<dyn Iterator<Item = Self::Item>>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.stages.into_iter().map(|stage| {
            bson::ser::to_document(&stage).expect("An error occurred serializing a pipeline stage")
        }))
    }
}

impl FromIterator<Stage> for Pipeline {
    fn from_iter<T: IntoIterator<Item = Stage>>(iter: T) -> Self {
        Pipeline {
            stages: iter.into_iter().collect(),
        }
    }
}

impl From<Pipeline> for Vec<bson::Document> {
    fn from(value: Pipeline) -> Self {
        value.into_iter().collect()
    }
}
