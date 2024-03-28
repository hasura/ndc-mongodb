#[macro_export()]
macro_rules! column {
    ($name:literal : $typ:literal) => {
        (
            $name.to_owned(),
            dc_api_types::Field::Column {
                column: $name.to_owned(),
                column_type: $typ.to_owned(),
            },
        )
    };
    ($name:literal => $column:literal : $typ:literal) => {
        (
            $name.to_owned(),
            dc_api_types::Field::Column {
                column: $column.to_owned(),
                column_type: $typ.to_owned(),
            },
        )
    };
}

#[macro_export]
macro_rules! relation_field {
    ($relationship:literal => $name:literal, $query:expr) => {
        (
            $name.into(),
            dc_api_types::Field::Relationship {
                relationship: $relationship.to_owned(),
                query: Box::new($query.into()),
            },
        )
    };
}

#[macro_export()]
macro_rules! nested_object_field {
    ($column:literal, $query:expr) => {
        dc_api_types::Field::NestedObject {
            column: $column.to_owned(),
            query: Box::new($query.into()),
        }
    };
}

#[macro_export()]
macro_rules! nested_object {
    ($name:literal => $column:literal, $query:expr) => {
        (
            $name.to_owned(),
            dc_api_test_helpers::nested_object_field!($column, $query),
        )
    };
}

#[macro_export()]
macro_rules! nested_array_field {
    ($field:expr) => {
        dc_api_types::Field::NestedArray {
            field: Box::new($field),
            limit: None,
            offset: None,
            r#where: None,
        }
    };
}

#[macro_export()]
macro_rules! nested_array {
    ($name:literal, $field:expr) => {
        (
            $name.to_owned(),
            dc_api_test_helpers::nested_array_field!($field),
        )
    };
}
