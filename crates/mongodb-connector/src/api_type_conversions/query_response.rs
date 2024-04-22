use std::collections::BTreeMap;

use dc_api_types::{self as v2};
use ndc_sdk::models::{self as v3};

pub fn v2_to_v3_explain_response(response: v2::ExplainResponse) -> v3::ExplainResponse {
    v3::ExplainResponse {
        details: BTreeMap::from_iter([
            ("plan".to_owned(), response.lines.join("\n")),
            ("query".to_owned(), response.query),
        ]),
    }
}
