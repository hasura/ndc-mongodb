#[macro_export()]
macro_rules! target {
    ($column:literal) => {
        $crate::ndc_models::ComparisonTarget::Column {
            name: $column.into(),
            field_path: None,
            path: vec![],
        }
    };
    ($column:literal, field_path:$field_path:expr $(,)?) => {
        $crate::ndc_models::ComparisonTarget::Column {
            name: $column.into(),
            field_path: $field_path.into_iter().map(|x| x.into()).collect(),
            path: vec![],
        }
    };
    ($column:literal, relations:$path:expr $(,)?) => {
        $crate::ndc_models::ComparisonTarget::Column {
            name: $column.into(),
            field_path: None,
            path: $path.into_iter().map(|x| x.into()).collect(),
        }
    };
    ($column:literal, field_path:$field_path:expr, relations:$path:expr $(,)?) => {
        $crate::ndc_models::ComparisonTarget::Column {
            name: $column.into(),
            // field_path: $field_path.into_iter().map(|x| x.into()).collect(),
            path: $path.into_iter().map(|x| x.into()).collect(),
        }
    };
    ($target:expr) => {
        $target
    };
}
