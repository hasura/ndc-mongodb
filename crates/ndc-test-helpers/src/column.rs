use std::collections::BTreeMap;

use itertools::Itertools as _;
use ndc_models::{Argument, ArgumentName, FieldName, PathElement};

pub struct Column {
    pub path: Vec<PathElement>,
    pub column: FieldName,
    pub arguments: BTreeMap<ArgumentName, Argument>,
    pub field_path: Option<Vec<FieldName>>,
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
    fn from(column: FieldName) -> Self {
        Column {
            path: Default::default(),
            column,
            arguments: Default::default(),
            field_path: Default::default(),
        }
    }
}
