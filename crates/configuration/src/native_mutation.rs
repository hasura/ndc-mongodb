use std::collections::BTreeMap;

use mongodb::{bson, options::SelectionCriteria};

use crate::{
    schema::{ObjectField, Type},
    serialized::{self},
};

/// Internal representation of Native Mutations. For doc comments see
/// [crate::serialized::NativeMutation]
///
/// Note: this type excludes `name` and `object_types` from the serialized type. Object types are
/// intended to be merged into one big map so should not be accessed through values of this type.
/// Native query values are stored in maps so names should be taken from map keys.
#[derive(Clone, Debug)]
pub struct NativeMutation {
    pub result_type: Type,
    pub arguments: BTreeMap<String, ObjectField>,
    pub command: bson::Document,
    pub selection_criteria: Option<SelectionCriteria>,
    pub description: Option<String>,
}

impl From<serialized::NativeMutation> for NativeMutation {
    fn from(value: serialized::NativeMutation) -> Self {
        NativeMutation {
            result_type: value.result_type,
            arguments: value.arguments,
            command: value.command,
            selection_criteria: value.selection_criteria,
            description: value.description,
        }
    }
}
