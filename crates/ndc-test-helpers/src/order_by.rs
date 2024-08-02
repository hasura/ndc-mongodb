#[macro_export]
macro_rules! asc {
    ($name:literal) => {
        $crate::ndc_models::OrderByElement {
            order_direction: $crate::ndc_models::OrderDirection::Asc,
            target: $crate::ndc_models::OrderByTarget::Column {
                name: $crate::ndc_models::FieldName::new($crate::smol_str::SmolStr::new($name)),
                field_path: None,
                path: vec![],
            },
        }
    };
}

#[macro_export]
macro_rules! desc {
    ($name:literal) => {
        $crate::ndc_models::OrderByElement {
            order_direction: $crate::ndc_models::OrderDirection::Desc,
            target: $crate::ndc_models::OrderByTarget::Column {
                name: $crate::ndc_models::FieldName::new($crate::smol_str::SmolStr::new($name)),
                field_path: None,
                path: vec![],
            },
        }
    };
}
