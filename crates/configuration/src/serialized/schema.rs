use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    schema::{Collection, ObjectType},
    WithName, WithNameRef,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    #[serde(default)]
    pub collections: BTreeMap<ndc_models::CollectionName, Collection>,
    #[serde(default)]
    pub object_types: BTreeMap<ndc_models::ObjectTypeName, ObjectType>,
}

impl Schema {
    pub fn into_named_collections(self) -> impl Iterator<Item = WithName<ndc_models::CollectionName, Collection>> {
        self.collections
            .into_iter()
            .map(|(name, field)| WithName::named(name, field))
    }

    pub fn into_named_object_types(self) -> impl Iterator<Item = WithName<ndc_models::ObjectTypeName, ObjectType>> {
        self.object_types
            .into_iter()
            .map(|(name, field)| WithName::named(name, field))
    }

    pub fn named_collections(&self) -> impl Iterator<Item = WithNameRef<'_, ndc_models::CollectionName, Collection>> {
        self.collections
            .iter()
            .map(|(name, field)| WithNameRef::named(name, field))
    }

    pub fn named_object_types(&self) -> impl Iterator<Item = WithNameRef<'_, ndc_models::ObjectTypeName, ObjectType>> {
        self.object_types
            .iter()
            .map(|(name, field)| WithNameRef::named(name, field))
    }

    /// Unify two schemas. Assumes that the schemas describe mutually exclusive sets of collections.
    pub fn merge(schema_a: Schema, schema_b: Schema) -> Schema {
        let collections = schema_a
            .collections
            .into_iter()
            .chain(schema_b.collections)
            .collect();
        let object_types = schema_a
            .object_types
            .into_iter()
            .chain(schema_b.object_types)
            .collect();
        Schema {
            collections,
            object_types,
        }
    }
}
