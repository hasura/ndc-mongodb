use std::fmt::Display;

use ndc_models::{ObjectField, ObjectType, Type};

// pub fn object_type(
//     name: impl Display,
//     fields: impl IntoIterator<Item = (impl ToString, impl Into<Type>)>,
// ) -> (String, ObjectType) {
//     let t = ObjectType {
//         description: Default::default(),
//         fields: fields
//             .into_iter()
//             .map(|(name, field_type)| {
//                 (
//                     name.to_string(),
//                     ObjectField {
//                         description: Default::default(),
//                         r#type: field_type.into(),
//                     },
//                 )
//             })
//             .collect(),
//     };
//     (name.to_string(), t)
// }

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
                        r#type: field_type.into(),
                    },
                )
            })
            .collect(),
    }
}
