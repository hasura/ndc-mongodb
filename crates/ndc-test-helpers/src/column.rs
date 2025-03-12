use std::collections::BTreeMap;

use itertools::Itertools as _;
use ndc_models::{Argument, ArgumentName, FieldName, PathElement, RelationshipName};

use crate::path_element;

/// An intermediate struct that can be used to populate ComparisonTarget::Column,
/// Dimension::Column, etc.
pub struct Column {
    pub path: Vec<PathElement>,
    pub column: FieldName,
    pub arguments: BTreeMap<ArgumentName, Argument>,
    pub field_path: Option<Vec<FieldName>>,
}

impl Column {
    pub fn path(mut self, elements: impl IntoIterator<Item = impl Into<PathElement>>) -> Self {
        self.path = elements.into_iter().map(Into::into).collect();
        self
    }

    pub fn from_relationship(mut self, name: impl Into<RelationshipName>) -> Self {
        self.path = vec![path_element(name).into()];
        self
    }
}

pub fn column(name: impl Into<FieldName>) -> Column {
    Column {
        path: Default::default(),
        column: name.into(),
        arguments: Default::default(),
        field_path: Default::default(),
    }
}

impl From<&str> for Column {
    fn from(input: &str) -> Self {
        let mut parts = input.split(".");
        let column = parts
            .next()
            .expect("a column reference must not be an empty string")
            .into();
        let field_path = parts.map(Into::into).collect_vec();
        Column {
            path: Default::default(),
            column,
            arguments: Default::default(),
            field_path: if field_path.is_empty() {
                None
            } else {
                Some(field_path)
            },
        }
    }
}

impl From<FieldName> for Column {
    fn from(name: FieldName) -> Self {
        column(name)
    }
}
