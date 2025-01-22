use std::collections::BTreeMap;

use ndc_models::{Argument, ArgumentName, ExistsInCollection, FieldName};

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

#[derive(Debug)]
pub struct ExistsInNestedCollectionBuilder {
    column_name: FieldName,
    arguments: BTreeMap<ArgumentName, Argument>,
    field_path: Vec<FieldName>,
}

pub fn exists_in_nested(column_name: impl Into<FieldName>) -> ExistsInNestedCollectionBuilder {
    ExistsInNestedCollectionBuilder {
        column_name: column_name.into(),
        arguments: Default::default(),
        field_path: Default::default(),
    }
}

impl ExistsInNestedCollectionBuilder {
    pub fn arguments(
        mut self,
        arguments: impl IntoIterator<Item = (impl Into<ArgumentName>, impl Into<Argument>)>,
    ) -> Self {
        self.arguments = arguments
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        self
    }

    pub fn field_path(
        mut self,
        field_path: impl IntoIterator<Item = impl Into<FieldName>>,
    ) -> Self {
        self.field_path = field_path.into_iter().map(Into::into).collect();
        self
    }
}

impl From<ExistsInNestedCollectionBuilder> for ExistsInCollection {
    fn from(builder: ExistsInNestedCollectionBuilder) -> Self {
        ExistsInCollection::NestedCollection {
            column_name: builder.column_name,
            arguments: builder.arguments,
            field_path: builder.field_path,
        }
    }
}
