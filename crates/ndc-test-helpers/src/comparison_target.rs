#[macro_export()]
macro_rules! target {
    ($column:literal) => {
        ndc_sdk::models::ComparisonTarget::Column {
            name: $column.to_owned(),
            path: vec![],
        }
    };
    ($column:literal, $path:expr $(,)?) => {
        ndc_sdk::models::ComparisonTarget::Column {
            name: $column.to_owned(),
            path: $path.into_iter().map(|x| x.into()).collect(),
        }
    };
    ($target:expr) => {
        $target
    };
}

pub fn root<S>(name: S) -> ndc_sdk::models::ComparisonTarget
where
    S: ToString,
{
    ndc_sdk::models::ComparisonTarget::RootCollectionColumn {
        name: name.to_string(),
    }
}
