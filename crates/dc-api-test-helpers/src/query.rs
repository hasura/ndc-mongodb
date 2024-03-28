use std::collections::HashMap;

use dc_api_types::{Aggregate, Expression, Field, OrderBy, Query};

#[derive(Clone, Debug, Default)]
pub struct QueryBuilder {
    aggregates: Option<Option<HashMap<String, Aggregate>>>,
    aggregates_limit: Option<Option<i64>>,
    fields: Option<Option<HashMap<String, Field>>>,
    limit: Option<Option<i64>>,
    offset: Option<Option<u64>>,
    order_by: Option<Option<OrderBy>>,
    predicate: Option<Expression>,
}

pub fn query() -> QueryBuilder {
    Default::default()
}

impl QueryBuilder {
    pub fn fields<I>(mut self, fields: I) -> Self
    where
        I: IntoIterator<Item = (String, Field)>,
    {
        self.fields = Some(Some(fields.into_iter().collect()));
        self
    }

    pub fn predicate(mut self, predicate: Expression) -> Self {
        self.predicate = Some(predicate);
        self
    }

    pub fn order_by(mut self, order_by: OrderBy) -> Self {
        self.order_by = Some(Some(order_by));
        self
    }
}

impl From<QueryBuilder> for Query {
    fn from(builder: QueryBuilder) -> Self {
        Query {
            aggregates: builder.aggregates,
            aggregates_limit: builder.aggregates_limit,
            fields: builder.fields,
            limit: builder.limit,
            offset: builder.offset,
            order_by: builder.order_by,
            r#where: builder.predicate,
        }
    }
}
