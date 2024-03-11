#[macro_export]
macro_rules! select {
    ($name:literal) => {
        dc_api_types::ColumnSelector::Column($name.to_owned())
    };
}

#[macro_export]
macro_rules! select_qualified {
    ([$($path_element:literal $(,)?)+]) => {
        dc_api_types::ColumnSelector::Path(
            nonempty::nonempty![
                $($path_element.to_owned(),)+
            ]
        )
    };
}
