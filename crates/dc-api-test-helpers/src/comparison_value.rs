#[macro_export]
macro_rules! column_value {
    ($($col:tt)+) => {
        dc_api_types::ComparisonValue::AnotherColumnComparison {
            column: $crate::compare!($($col)+),
        }
    };
}

#[macro_export]
macro_rules! value {
    ($value:expr, $typ:literal) => {
        dc_api_types::ComparisonValue::ScalarValueComparison {
            value: $value,
            value_type: $typ.to_owned(),
        }
    };
}
