use std::collections::BTreeMap;

use indexmap::IndexMap;
use ndc_models::{
    Aggregate, Argument, ArgumentName, Dimension, FieldName, GroupExpression, GroupOrderBy,
    GroupOrderByElement, Grouping, OrderBy, OrderDirection, PathElement,
};

use crate::column::Column;

#[derive(Clone, Debug, Default)]
pub struct GroupingBuilder {
    dimensions: Vec<Dimension>,
    aggregates: IndexMap<FieldName, Aggregate>,
    predicate: Option<GroupExpression>,
    order_by: Option<GroupOrderBy>,
    limit: Option<u32>,
    offset: Option<u32>,
}

pub fn grouping() -> GroupingBuilder {
    Default::default()
}

impl GroupingBuilder {
    pub fn dimensions(
        mut self,
        dimensions: impl IntoIterator<Item = impl Into<Dimension>>,
    ) -> Self {
        self.dimensions = dimensions.into_iter().map(Into::into).collect();
        self
    }

    pub fn aggregates(
        mut self,
        aggregates: impl IntoIterator<Item = (impl Into<FieldName>, impl Into<Aggregate>)>,
    ) -> Self {
        self.aggregates = aggregates
            .into_iter()
            .map(|(name, aggregate)| (name.into(), aggregate.into()))
            .collect();
        self
    }

    pub fn predicate(mut self, predicate: impl Into<GroupExpression>) -> Self {
        self.predicate = Some(predicate.into());
        self
    }

    pub fn order_by(mut self, order_by: impl Into<GroupOrderBy>) -> Self {
        self.order_by = Some(order_by.into());
        self
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }
}

impl From<GroupingBuilder> for Grouping {
    fn from(value: GroupingBuilder) -> Self {
        Grouping {
            dimensions: value.dimensions,
            aggregates: value.aggregates,
            predicate: value.predicate,
            order_by: value.order_by,
            limit: value.limit,
            offset: value.offset,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DimensionColumnBuilder {
    path: Vec<PathElement>,
    column_name: FieldName,
    arguments: BTreeMap<ArgumentName, Argument>,
    field_path: Option<Vec<FieldName>>,
}

pub fn dimension_column(column: impl Into<Column>) -> DimensionColumnBuilder {
    let column = column.into();
    DimensionColumnBuilder {
        path: column.path,
        column_name: column.column,
        arguments: column.arguments,
        field_path: column.field_path,
    }
}

impl DimensionColumnBuilder {
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
            .map(|(name, argument)| (name.into(), argument.into()))
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
}

impl From<DimensionColumnBuilder> for Dimension {
    fn from(value: DimensionColumnBuilder) -> Self {
        Dimension::Column {
            path: value.path,
            column_name: value.column_name,
            arguments: value.arguments,
            field_path: value.field_path,
            extraction: None,
        }
    }
}

/// Produces a consistent ordering for up to 10 dimensions
pub fn ordered_dimensions() -> GroupOrderBy {
    GroupOrderBy {
        elements: (0..10)
            .map(|index| GroupOrderByElement {
                order_direction: OrderDirection::Asc,
                target: ndc_models::GroupOrderByTarget::Dimension { index },
            })
            .collect(),
    }
}
