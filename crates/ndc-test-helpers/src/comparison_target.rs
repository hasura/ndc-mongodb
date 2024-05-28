#[macro_export()]
macro_rules! target {
    ($column:literal) => {
        $crate::ndc_models::ComparisonTarget::Column {
            name: $column.to_owned(),
            path: vec![],
        }
    };
    ($column:literal, field_path:$field_path:expr $(,)?) => {
        $crate::ndc_models::ComparisonTarget::Column {
            name: $column.to_owned(),
            field_path: $field_path.into_iter().map(|x| x.into()).collect(),
            path: vec![],
        }
    };
    ($column:literal, relations:$path:expr $(,)?) => {
        $crate::ndc_models::ComparisonTarget::Column {
            name: $column.to_owned(),
            path: $path.into_iter().map(|x| x.into()).collect(),
        }
    };
    ($column:literal, field_path:$field_path:expr, relations:$path:expr $(,)?) => {
        $crate::ndc_models::ComparisonTarget::Column {
            name: $column.to_owned(),
            // field_path: $field_path.into_iter().map(|x| x.into()).collect(),
            path: $path.into_iter().map(|x| x.into()).collect(),
        }
    };
    ($target:expr) => {
        $target
    };
}

pub fn root<S>(name: S) -> ndc_models::ComparisonTarget
where
    S: ToString,
{
    ndc_models::ComparisonTarget::RootCollectionColumn {
        name: name.to_string(),
    }
}
