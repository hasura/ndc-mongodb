#[macro_export()]
macro_rules! column_aggregate {
    ($name:literal => $column:literal, $function:literal) => {
        (
            $name,
            $crate::ndc_models::Aggregate::SingleColumn {
                column: $column.into(),
                function: $function.into(),
                field_path: None,
            },
        )
    };
}

#[macro_export()]
macro_rules! star_count_aggregate {
    ($name:literal) => {
        ($name, $crate::ndc_models::Aggregate::StarCount {})
    };
}

#[macro_export()]
macro_rules! column_count_aggregate {
    ($name:literal => $column:literal, distinct:$distinct:literal) => {
        (
            $name,
            $crate::ndc_models::Aggregate::ColumnCount {
                column: $column.into(),
                distinct: $distinct.to_owned(),
                field_path: None,
            },
        )
    };
}
