#[macro_export]
macro_rules! field {
    ($name:literal) => {
        (
            $name,
            ndc_sdk::models::Field::Column {
                column: $name.to_owned(),
                fields: None,
            },
        )
    };
    ($name:literal => $column_name:literal) => {
        (
            $name,
            ndc_sdk::models::Field::Column {
                column: $column_name.to_owned(),
                fields: None,
            },
        )
    };
    ($name:literal => $column_name:literal, $fields:expr) => {
        (
            $name,
            ndc_sdk::models::Field::Column {
                column: $column_name.to_owned(),
                fields: Some($fields.into()),
            },
        )
    };
}

#[macro_export]
macro_rules! object {
    ($fields:expr) => {
        ndc_sdk::models::NestedField::Object(ndc_sdk::models::NestedObject {
            fields: $fields
                .into_iter()
                .map(|(name, field)| (name.to_owned(), field))
                .collect(),
        })
    };
}

#[macro_export]
macro_rules! array {
    ($fields:expr) => {
        ndc_sdk::models::NestedField::Array(ndc_sdk::models::NestedArray {
            fields: Box::new($fields),
        })
    };
}

#[macro_export]
macro_rules! relation_field {
    ($relationship:literal => $name:literal) => {
        (
            $name,
            ndc_sdk::models::Field::Relationship {
                query: Box::new($crate::query().into()),
                relationship: $relationship.to_owned(),
                arguments: Default::default(),
            },
        )
    };
    ($relationship:literal => $name:literal, $query:expr) => {
        (
            $name,
            ndc_sdk::models::Field::Relationship {
                query: Box::new($query.into()),
                relationship: $relationship.to_owned(),
                arguments: Default::default(),
            },
        )
    };
}
