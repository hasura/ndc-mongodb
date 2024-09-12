use mongodb::bson;
use serde::{Deserialize, Serialize};

/// Operators used with [Stage::Group]. This is a work-in-progress - add entries as we use them.
///
/// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/group/#std-label-accumulators-group
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum Accumulator {
    /// Returns an average of numerical values. Ignores non-numeric values.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/avg/#mongodb-group-grp.-avg
    #[serde(rename = "$avg")]
    Avg(bson::Bson),

    /// Returns the number of documents in a group. Distinct from the $count pipeline stage.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/count-accumulator/#mongodb-group-grp.-count
    #[serde(rename = "$count", with = "empty_object")]
    Count,

    /// Returns the lowest expression value for each group.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/min/#mongodb-group-grp.-min
    #[serde(rename = "$min")]
    Min(bson::Bson),

    /// Returns the highest expression value for each group.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/max/#mongodb-group-grp.-max
    #[serde(rename = "$max")]
    Max(bson::Bson),

    #[serde(rename = "$push")]
    Push(bson::Bson),

    /// Returns a sum of numerical values. Ignores non-numeric values.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/sum/#mongodb-group-grp.-sum
    #[serde(rename = "$sum")]
    Sum(bson::Bson),
}

mod empty_object {
    use std::collections::BTreeMap;

    use serde::{ser::SerializeMap, Deserialize};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<(), D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        BTreeMap::<(), ()>::deserialize(deserializer)?;
        Ok(())
    }

    pub fn serialize<S>(serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_map(Some(0))?.end()
    }
}
