#[macro_export]
macro_rules! field {
    ($name:literal) => {
        (
            $name,
            $crate::ndc_models::Field::Column {
                column: $name.into(),
                arguments: Default::default(),
                fields: None,
            },
        )
    };
    ($name:literal => $column_name:literal) => {
        (
            $name,
            $crate::ndc_models::Field::Column {
                column: $column_name.into(),
                arguments: Default::default(),
                fields: None,
            },
        )
    };
    ($name:literal => $column_name:literal, $fields:expr) => {
        (
            $name,
            $crate::ndc_models::Field::Column {
                column: $column_name.into(),
                arguments: Default::default(),
                fields: Some($fields.into()),
            },
        )
    };
}

#[macro_export]
macro_rules! object {
    ($fields:expr) => {
        $crate::ndc_models::NestedField::Object($crate::ndc_models::NestedObject {
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
        $crate::ndc_models::NestedField::Array($crate::ndc_models::NestedArray {
            fields: Box::new($fields),
        })
    };
}

#[macro_export]
macro_rules! relation_field {
    ($name:literal => $relationship:literal) => {
        (
            $name,
            $crate::ndc_models::Field::Relationship {
                query: Box::new($crate::query().into()),
                relationship: $relationship.into(),
                arguments: Default::default(),
            },
        )
    };
    ($name:literal => $relationship:literal, $query:expr) => {
        (
            $name,
            $crate::ndc_models::Field::Relationship {
                query: Box::new($query.into()),
                relationship: $relationship.into(),
                arguments: Default::default(),
            },
        )
    };
}
