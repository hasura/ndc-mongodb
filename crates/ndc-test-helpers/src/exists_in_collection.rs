#[macro_export]
macro_rules! related {
    ($rel:literal) => {
        ndc_sdk::models::ExistsInCollection::Related {
            relationship: $rel.to_owned(),
            arguments: Default::default(),
        }
    };
    ($rel:literal, $args:expr $(,)?) => {
        ndc_sdk::models::ExistsInCollection::Related {
            relationship: $rel.to_owned(),
            arguments: $args.into_iter().map(|x| x.into()).collect(),
        }
    };
}

#[macro_export]
macro_rules! unrelated {
    ($coll:literal) => {
        ndc_sdk::models::ExistsInCollection::Unrelated {
            collection: $coll.to_owned(),
            arguments: Default::default(),
        }
    };
    ($coll:literal, $args:expr $(,)?) => {
        ndc_sdk::models::ExistsInCollection::Related {
            collection: $coll.to_owned(),
            arguments: $args.into_iter().map(|x| x.into()).collect(),
        }
    };
}
