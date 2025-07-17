use std::{collections::BTreeMap, fmt::Display};

use ndc_models::{CollectionInfo, ObjectField, ObjectType, Type, UniquenessConstraint};

pub fn collection(name: impl Display + Clone) -> (ndc_models::CollectionName, CollectionInfo) {
    let coll = CollectionInfo {
        name: name.to_string().into(),
        description: None,
        arguments: Default::default(),
        collection_type: name.to_string().into(),
        uniqueness_constraints: make_primary_key_uniqueness_constraint(name.clone()),
        relational_mutations: None,
    };
    (name.to_string().into(), coll)
}

pub fn make_primary_key_uniqueness_constraint(
    collection_name: impl Display,
) -> BTreeMap<String, UniquenessConstraint> {
    [(
        format!("{collection_name}_id"),
        UniquenessConstraint {
            unique_columns: vec!["_id".to_owned().into()],
        },
    )]
    .into()
}
