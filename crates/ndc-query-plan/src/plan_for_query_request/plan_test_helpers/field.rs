#[macro_export]
macro_rules! field {
    ($name:literal: $typ:expr) => {
        (
            ::ndc_models::FieldName::from($name),
            $crate::Field::Column {
                column: $name.into(),
                column_type: $typ,
                fields: None,
            },
        )
    };
    ($name:literal => $column_name:literal: $typ:expr) => {
        (
            ::ndc_models::FieldName::from($name),
            $crate::Field::Column {
                column: $column_name.into(),
                column_type: $typ,
                fields: None,
            },
        )
    };
    ($name:literal => $column_name:literal: $typ:expr, $fields:expr) => {
        (
            ::ndc_models::FieldName::from($name),
            $crate::Field::Column {
                column: $column_name.into(),
                column_type: $typ,
                fields: Some($fields.into()),
            },
        )
    };
}

#[macro_export]
macro_rules! object {
    ($fields:expr) => {
        $crate::NestedField::Object($crate::NestedObject {
            fields: $fields
                .into_iter()
                .map(|(name, field)| (name.into(), field))
                .collect(),
        })
    };
}

#[macro_export]
macro_rules! array {
    ($fields:expr) => {
        $crate::NestedField::Array($crate::NestedArray {
            fields: Box::new($fields),
        })
    };
}

#[macro_export]
macro_rules! relation_field {
    ($name:literal => $relationship:literal) => {
        (
            $name,
            $crate::Field::Relationship {
                query: Box::new($crate::query().into()),
                relationship: $relationship.to_owned(),
                arguments: Default::default(),
            },
        )
    };
    ($name:literal => $relationship:literal, $query:expr) => {
        (
            $name,
            $crate::Field::Relationship {
                query: Box::new($query.into()),
                relationship: $relationship.to_owned(),
                arguments: Default::default(),
            },
        )
    };
}
