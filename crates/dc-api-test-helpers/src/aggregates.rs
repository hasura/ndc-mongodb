#[macro_export()]
macro_rules! column_aggregate {
    ($name:literal => $column:literal, $function:literal : $typ:literal) => {
        (
            $name.to_owned(),
            dc_api_types::Aggregate::SingleColumn {
                column: $column.to_owned(),
                function: $function.to_owned(),
                result_type: $typ.to_owned(),
            },
        )
    };
}

#[macro_export()]
macro_rules! star_count_aggregate {
    ($name:literal) => {
        (
            $name.to_owned(),
            dc_api_types::Aggregate::StarCount {},
        )
    };
}

#[macro_export()]
macro_rules! column_count_aggregate {
    ($name:literal => $column:literal, distinct:$distinct:literal) => {
        (
            $name.to_owned(),
            dc_api_types::Aggregate::ColumnCount {
                column: $column.to_owned(),
                distinct: $distinct.to_owned(),
            },
        )
    };
}
