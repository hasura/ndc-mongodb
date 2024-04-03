#[macro_export()]
macro_rules! column_aggregate {
    ($name:literal => $column:literal, $function:literal) => {
        (
            $name,
            ndc_sdk::models::Aggregate::SingleColumn {
                column: $column.to_owned(),
                function: $function.to_owned()
            },
        )
    };
}

#[macro_export()]
macro_rules! star_count_aggregate {
    ($name:literal) => {
        (
            $name,
            ndc_sdk::models::Aggregate::StarCount {},
        )
    };
}

#[macro_export()]
macro_rules! column_count_aggregate {
    ($name:literal => $column:literal, distinct:$distinct:literal) => {
        (
            $name,
            ndc_sdk::models::Aggregate::ColumnCount {
                column: $column.to_owned(),
                distinct: $distinct.to_owned(),
            },
        )
    };
}
