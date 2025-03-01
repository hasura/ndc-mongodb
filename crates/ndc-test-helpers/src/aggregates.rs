use std::collections::BTreeMap;

use ndc_models::{Aggregate, AggregateFunctionName, Argument, ArgumentName, FieldName};

use crate::column::Column;

pub struct AggregateColumnBuilder {
    column: FieldName,
    arguments: BTreeMap<ArgumentName, Argument>,
    field_path: Option<Vec<FieldName>>,
    function: AggregateFunctionName,
}

pub fn column_aggregate(
    column: impl Into<Column>,
    function: impl Into<AggregateFunctionName>,
) -> AggregateColumnBuilder {
    let column = column.into();
    AggregateColumnBuilder {
        column: column.column,
        function: function.into(),
        arguments: column.arguments,
        field_path: column.field_path,
    }
}

impl AggregateColumnBuilder {
    pub fn field_path(
        mut self,
        field_path: impl IntoIterator<Item = impl Into<FieldName>>,
    ) -> Self {
        self.field_path = Some(field_path.into_iter().map(Into::into).collect());
        self
    }
}

impl From<AggregateColumnBuilder> for Aggregate {
    fn from(builder: AggregateColumnBuilder) -> Self {
        Aggregate::SingleColumn {
            column: builder.column,
            arguments: builder.arguments,
            function: builder.function,
            field_path: builder.field_path,
        }
    }
}

#[macro_export()]
macro_rules! star_count_aggregate {
    ($name:literal) => {
        ($name, $crate::ndc_models::Aggregate::StarCount {})
    };
}

#[macro_export()]
macro_rules! column_count_aggregate {
    ($name:literal => $column:literal, distinct:$distinct:literal) => {
        (
            $name,
            $crate::ndc_models::Aggregate::ColumnCount {
                column: $column.into(),
                arguments: Default::default(),
                distinct: $distinct.to_owned(),
                field_path: None,
            },
        )
    };
}
