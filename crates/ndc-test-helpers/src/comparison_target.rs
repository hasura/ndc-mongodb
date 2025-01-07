#[macro_export()]
macro_rules! target {
    ($column:literal) => {
        $crate::ndc_models::ComparisonTarget::Column {
            name: $column.into(),
            arguments: Default::default(),
            field_path: None,
        }
    };
    ($column:literal, field_path:$field_path:expr $(,)?) => {
        $crate::ndc_models::ComparisonTarget::Column {
            name: $column.into(),
            field_path: $field_path.into_iter().map(|x| x.into()).collect(),
            path: vec![],
        }
    };
    ($target:expr) => {
        $target
    };
}
