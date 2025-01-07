#[macro_export]
macro_rules! related {
    ($rel:literal) => {
        $crate::ndc_models::ExistsInCollection::Related {
            field_path: Default::default(),
            relationship: $rel.into(),
            arguments: Default::default(),
        }
    };
    ($rel:literal, $args:expr $(,)?) => {
        $crate::ndc_models::ExistsInCollection::Related {
            field_path: Default::default(),
            relationship: $rel.into(),
            arguments: $args.into_iter().map(|x| x.into()).collect(),
        }
    };
}

#[macro_export]
macro_rules! unrelated {
    ($coll:literal) => {
        $crate::ndc_models::ExistsInCollection::Unrelated {
            collection: $coll.into(),
            arguments: Default::default(),
        }
    };
    ($coll:literal, $args:expr $(,)?) => {
        $crate::ndc_models::ExistsInCollection::Related {
            collection: $coll.into(),
            arguments: $args.into_iter().map(|x| x.into()).collect(),
        }
    };
}
