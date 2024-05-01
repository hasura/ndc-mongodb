#[macro_export]
macro_rules! column_value {
    ($($column:tt)+) => {
        $crate::ndc_models::ComparisonValue::Column {
            column: $crate::target!($($column)+),
        }
    };
}

#[macro_export]
macro_rules! value {
    ($($value:tt)+) => {
        $crate::ndc_models::ComparisonValue::Scalar {
            value: serde_json::json!($($value)+),
        }
    };
}

#[macro_export]
macro_rules! variable {
    ($variable:ident) => {
        $crate::ndc_models::ComparisonValue::Variable {
            name: stringify!($variable).to_owned(),
        }
    };
    ($variable:expr) => {
        $crate::ndc_models::ComparisonValue::Variable { name: $expr }
    };
}
