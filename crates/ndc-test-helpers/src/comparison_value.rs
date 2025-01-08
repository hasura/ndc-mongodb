#[macro_export]
macro_rules! column_value {
    ($column:literal) => {
        $crate::ndc_models::ComparisonValue::Column {
            path: Default::default(),
            name: $column.into(),
            arguments: Default::default(),
            field_path: Default::default(),
            scope: Default::default(),
        }
    };
    ($column:literal, field_path:$field_path:expr $(,)?) => {
        $crate::ndc_models::ComparisonValue::Column {
            path: Default::default(),
            name: $column.into(),
            arguments: Default::default(),
            field_path: $field_path.into_iter().map(|x| x.into()).collect(),
            scope: Default::default(),
        }
    };
    ($column:literal, relations:$relations:expr $(,)?) => {
        $crate::ndc_models::ComparisonValue::Column {
            path: $relations.into_iter().map(|x| x.into()).collect(),
            name: $column.into(),
            arguments: Default::default(),
            field_path: Default::default(),
            scope: Default::default(),
        }
    };
    ($column:literal, field_path:$field_path:expr, relations:$relations:expr $(,)?) => {
        $crate::ndc_models::ComparisonValue::Column {
            path: $relations.into_iter().map(|x| x.into()).collect(),
            name: $column.into(),
            arguments: Default::default(),
            field_path: $field_path.into_iter().map(|x| x.into()).collect(),
            scope: Default::default(),
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
            name: stringify!($variable).into(),
        }
    };
    ($variable:expr) => {
        $crate::ndc_models::ComparisonValue::Variable { name: $expr }
    };
}
