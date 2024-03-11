use serde::{Deserialize, Serialize};

use crate::ComparisonColumn;

/// Types for values in the `values` field of `ApplyBinaryArrayComparison`. The v2 DC API
/// interprets all such values as scalars, so we want to parse whatever is given as
/// a serde_json::Value. But the v3 NDC API allows column references or variable references here.
/// So this enum is present to support queries translated from the v3 API.
///
/// For compatibility with the v2 API the enum is designed so that it will always deserialize to
/// the Scalar variant, and other variants will fail to serialize.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArrayComparisonValue {
    Scalar(serde_json::Value),
    #[serde(skip)]
    Column(ComparisonColumn),
    #[serde(skip)]
    Variable(String),
}
