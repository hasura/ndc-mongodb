use indexmap::IndexMap;

use crate::{
    Aggregate, ConnectorTypes, Expression, Field, Grouping, OrderBy, OrderByElement, Query, Relationships, Scope
};

#[derive(Clone, Debug, Default)]
pub struct QueryBuilder<T: ConnectorTypes> {
    aggregates: Option<IndexMap<ndc_models::FieldName, Aggregate<T>>>,
    fields: Option<IndexMap<ndc_models::FieldName, Field<T>>>,
    limit: Option<u32>,
    offset: Option<u32>,
    order_by: Option<OrderBy<T>>,
    predicate: Option<Expression<T>>,
    groups: Option<Grouping<T>>,
    relationships: Relationships<T>,
    scope: Option<Scope>,
}

#[allow(dead_code)]
pub fn query<T: ConnectorTypes>() -> QueryBuilder<T> {
    QueryBuilder::new()
}

impl<T: ConnectorTypes> QueryBuilder<T> {
    pub fn new() -> Self {
        Self {
            fields: None,
            aggregates: Default::default(),
            limit: None,
            offset: None,
            order_by: None,
            predicate: None,
            groups: None,
            relationships: Default::default(),
            scope: None,
        }
    }

    pub fn fields(
        mut self,
        fields: impl IntoIterator<Item = (impl ToString, impl Into<Field<T>>)>,
    ) -> Self {
        self.fields = Some(
            fields
                .into_iter()
                .map(|(name, field)| (name.to_string().into(), field.into()))
                .collect(),
        );
        self
    }

    pub fn aggregates<const S: usize>(mut self, aggregates: [(&str, Aggregate<T>); S]) -> Self {
        self.aggregates = Some(
            aggregates
                .into_iter()
                .map(|(name, aggregate)| (name.into(), aggregate))
                .collect(),
        );
        self
    }

    pub fn limit(mut self, n: u32) -> Self {
        self.limit = Some(n);
        self
    }

    pub fn order_by(mut self, elements: Vec<OrderByElement<T>>) -> Self {
        self.order_by = Some(OrderBy { elements });
        self
    }

    pub fn predicate(mut self, expression: Expression<T>) -> Self {
        self.predicate = Some(expression);
        self
    }

    pub fn scope(mut self, scope: Scope) -> Self {
        self.scope = Some(scope);
        self
    }
}

impl<T: ConnectorTypes> From<QueryBuilder<T>> for Query<T> {
    fn from(value: QueryBuilder<T>) -> Self {
        Query {
            aggregates: value.aggregates,
            fields: value.fields,
            limit: value.limit,
            offset: value.offset,
            order_by: value.order_by,
            predicate: value.predicate,
            groups: value.groups,
            relationships: value.relationships,
            scope: value.scope,
        }
    }
}
