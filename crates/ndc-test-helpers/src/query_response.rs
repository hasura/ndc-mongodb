use indexmap::IndexMap;
use ndc_models::{QueryResponse, RowFieldValue, RowSet};

#[derive(Clone, Debug, Default)]
pub struct QueryResponseBuilder {
    row_sets: Vec<RowSet>,
}

impl QueryResponseBuilder {
    pub fn build(self) -> QueryResponse {
        QueryResponse(self.row_sets)
    }

    pub fn row_set(mut self, row_set: impl Into<RowSet>) -> Self {
        self.row_sets.push(row_set.into());
        self
    }

    pub fn row_set_rows(
        mut self,
        rows: impl IntoIterator<
            Item = impl IntoIterator<Item = (impl ToString, impl Into<serde_json::Value>)>,
        >,
    ) -> Self {
        self.row_sets.push(row_set().rows(rows).into());
        self
    }

    pub fn empty_row_set(mut self) -> Self {
        self.row_sets.push(RowSet {
            aggregates: None,
            rows: Some(vec![]),
            groups: Default::default(),
        });
        self
    }
}

impl From<QueryResponseBuilder> for QueryResponse {
    fn from(value: QueryResponseBuilder) -> Self {
        value.build()
    }
}

#[derive(Clone, Debug, Default)]
pub struct RowSetBuilder {
    aggregates: IndexMap<ndc_models::FieldName, serde_json::Value>,
    rows: Vec<IndexMap<ndc_models::FieldName, RowFieldValue>>,
    groups: Option<Vec<ndc_models::Group>>,
}

impl RowSetBuilder {
    pub fn into_response(self) -> QueryResponse {
        QueryResponse(vec![self.into()])
    }

    pub fn aggregates(
        mut self,
        aggregates: impl IntoIterator<Item = (impl ToString, impl Into<serde_json::Value>)>,
    ) -> Self {
        self.aggregates.extend(
            aggregates
                .into_iter()
                .map(|(k, v)| (k.to_string().into(), v.into())),
        );
        self
    }

    pub fn rows(
        mut self,
        rows: impl IntoIterator<
            Item = impl IntoIterator<Item = (impl ToString, impl Into<serde_json::Value>)>,
        >,
    ) -> Self {
        self.rows.extend(rows.into_iter().map(|r| {
            r.into_iter()
                .map(|(k, v)| (k.to_string().into(), RowFieldValue(v.into())))
                .collect()
        }));
        self
    }

    pub fn row(
        mut self,
        row: impl IntoIterator<Item = (impl ToString, impl Into<serde_json::Value>)>,
    ) -> Self {
        self.rows.push(
            row.into_iter()
                .map(|(k, v)| (k.to_string().into(), RowFieldValue(v.into())))
                .collect(),
        );
        self
    }

    pub fn groups(
        mut self,
        groups: impl IntoIterator<Item = impl Into<ndc_models::Group>>,
    ) -> Self {
        self.groups = Some(groups.into_iter().map(Into::into).collect());
        self
    }
}

impl From<RowSetBuilder> for RowSet {
    fn from(
        RowSetBuilder {
            aggregates,
            rows,
            groups,
        }: RowSetBuilder,
    ) -> Self {
        RowSet {
            aggregates: if aggregates.is_empty() {
                None
            } else {
                Some(aggregates)
            },
            rows: if rows.is_empty() { None } else { Some(rows) },
            groups,
        }
    }
}

impl From<RowSetBuilder> for QueryResponse {
    fn from(value: RowSetBuilder) -> Self {
        value.into_response()
    }
}

pub fn query_response() -> QueryResponseBuilder {
    Default::default()
}

pub fn row_set() -> RowSetBuilder {
    Default::default()
}
