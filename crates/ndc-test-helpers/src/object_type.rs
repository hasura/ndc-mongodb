use std::collections::BTreeMap;

use ndc_models::{ObjectField, ObjectType, Type};

pub fn object_type(
    fields: impl IntoIterator<Item = (impl ToString, impl Into<Type>)>,
) -> ObjectType {
    ObjectType {
        description: Default::default(),
        fields: fields
            .into_iter()
            .map(|(name, field_type)| {
                (
                    name.to_string(),
                    ObjectField {
                        description: Default::default(),
                        arguments: BTreeMap::new(),
                        r#type: field_type.into(),
                    },
                )
            })
            .collect(),
    }
}
