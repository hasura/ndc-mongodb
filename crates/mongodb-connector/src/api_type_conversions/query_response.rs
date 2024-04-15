use std::collections::BTreeMap;

use dc_api_types::{self as v2};
use ndc_sdk::models::{self as v3};

pub fn v2_to_v3_query_response(response: v2::QueryResponse) -> v3::QueryResponse {
    let rows: Vec<v3::RowSet> = match response {
        v2::QueryResponse::ForEach { rows } => rows
            .into_iter()
            .map(|foreach| v2_to_v3_row_set(foreach.query))
            .collect(),
        v2::QueryResponse::Single(row_set) => vec![v2_to_v3_row_set(row_set)],
    };
    v3::QueryResponse(rows)
}

fn v2_to_v3_row_set(row_set: v2::RowSet) -> v3::RowSet {
    let (aggregates, rows) = match row_set {
        v2::RowSet::Aggregate { aggregates, rows } => (Some(aggregates), rows),
        v2::RowSet::Rows { rows } => (None, Some(rows)),
    };

    v3::RowSet {
        aggregates: aggregates.map(hash_map_to_index_map),
        rows: rows.map(|xs| {
            xs.into_iter()
                .map(|field_values| {
                    field_values
                        .into_iter()
                        .map(|(name, value)| (name, v2_to_v3_field_value(value)))
                        .collect()
                })
                .collect()
        }),
    }
}

fn v2_to_v3_field_value(field_value: v2::ResponseFieldValue) -> v3::RowFieldValue {
    v3::RowFieldValue(serde_json::to_value(field_value).expect("serializing result field value"))
}

fn hash_map_to_index_map<K, V, InputMap, OutputMap>(xs: InputMap) -> OutputMap
where
    InputMap: IntoIterator<Item = (K, V)>,
    OutputMap: FromIterator<(K, V)>,
{
    xs.into_iter().collect::<OutputMap>()
}

pub fn v2_to_v3_explain_response(response: v2::ExplainResponse) -> v3::ExplainResponse {
    v3::ExplainResponse {
        details: BTreeMap::from_iter([
            ("plan".to_owned(), response.lines.join("\n")),
            ("query".to_owned(), response.query),
        ]),
    }
}
