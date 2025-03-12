use std::collections::BTreeMap;

use ndc_models::{Argument, ArgumentName, ComparisonValue, FieldName, PathElement};

#[macro_export]
macro_rules! value {
    ($($value:tt)+) => {
        $crate::ndc_models::ComparisonValue::Scalar {
            value: serde_json::json!($($value)+),
        }
    };
}

#[macro_export]
macro_rules! variable {
    ($variable:ident) => {
        $crate::ndc_models::ComparisonValue::Variable {
            name: stringify!($variable).into(),
        }
    };
    ($variable:expr) => {
        $crate::ndc_models::ComparisonValue::Variable { name: $expr }
    };
}

#[derive(Debug)]
pub struct ColumnValueBuilder {
    path: Vec<PathElement>,
    name: FieldName,
    arguments: BTreeMap<ArgumentName, Argument>,
    field_path: Option<Vec<FieldName>>,
    scope: Option<usize>,
}

pub fn column_value(name: impl Into<FieldName>) -> ColumnValueBuilder {
    ColumnValueBuilder {
        path: Default::default(),
        name: name.into(),
        arguments: Default::default(),
        field_path: Default::default(),
        scope: Default::default(),
    }
}

impl ColumnValueBuilder {
    pub fn path(mut self, path: impl IntoIterator<Item = impl Into<PathElement>>) -> Self {
        self.path = path.into_iter().map(Into::into).collect();
        self
    }

    pub fn arguments(
        mut self,
        arguments: impl IntoIterator<Item = (impl Into<ArgumentName>, impl Into<Argument>)>,
    ) -> Self {
        self.arguments = arguments
            .into_iter()
            .map(|(name, arg)| (name.into(), arg.into()))
            .collect();
        self
    }

    pub fn field_path(
        mut self,
        field_path: impl IntoIterator<Item = impl Into<FieldName>>,
    ) -> Self {
        self.field_path = Some(field_path.into_iter().map(Into::into).collect());
        self
    }

    pub fn scope(mut self, scope: usize) -> Self {
        self.scope = Some(scope);
        self
    }
}

impl From<ColumnValueBuilder> for ComparisonValue {
    fn from(builder: ColumnValueBuilder) -> Self {
        ComparisonValue::Column {
            path: builder.path,
            name: builder.name,
            arguments: builder.arguments,
            field_path: builder.field_path,
            scope: builder.scope,
        }
    }
}
