#[macro_export]
macro_rules! compare {
    ($name:literal: $typ:literal) => {
        dc_api_types::ComparisonColumn {
            column_type: $typ.to_owned(),
            name: dc_api_types::ColumnSelector::Column($name.to_owned()),
            path: None,
        }
    };
    ($path:expr, $name:literal: $typ:literal) => {
        dc_api_types::ComparisonColumn {
            column_type: $typ.to_owned(),
            name: dc_api_types::ColumnSelector::Column($name.to_owned()),
            path: Some($path.into_iter().map(|v| v.to_string()).collect()),
        }
    };
}

#[macro_export]
macro_rules! compare_with_path {
    ($path:expr, $name:literal: $typ:literal) => {
        dc_api_types::ComparisonColumn {
            column_type: $typ.to_owned(),
            name: dc_api_types::ColumnSelector::Column($name.to_owned()),
            path: Some($path.into_iter().map(|v| v.to_string()).collect()),
        }
    };
}
