#[macro_export]
macro_rules! column_value {
    ($($column:tt)+) => {
        ndc_sdk::models::ComparisonValue::Column {
            column: $crate::target!($($column)+),
        }
    };
}

#[macro_export]
macro_rules! value {
    ($($value:tt)+) => {
        ndc_sdk::models::ComparisonValue::Scalar {
            value: serde_json::json!($($value)+),
        }
    };
}

#[macro_export]
macro_rules! variable {
    ($variable:ident) => {
        ndc_sdk::models::ComparisonValue::Variable {
            name: stringify!($variable).to_owned(),
        }
    };
    ($variable:expr) => {
        ndc_sdk::models::ComparisonValue::Variable { name: $expr }
    };
}
