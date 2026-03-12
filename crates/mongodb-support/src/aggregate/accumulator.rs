use mongodb::bson;
use serde::{Deserialize, Serialize};

/// Operators used with [Stage::Group]. This is a work-in-progress - add entries as we use them.
///
/// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/group/#std-label-accumulators-group
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum Accumulator {
    /// Returns an array of unique expression values for each group. Order of the array elements is undefined.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/addToSet/#mongodb-group-grp.-addToSet
    #[serde(rename = "$addToSet")]
    AddToSet(bson::Bson),

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

    /// Returns the value from the first document for each group.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/first/#mongodb-group-grp.-first
    #[serde(rename = "$first")]
    First(bson::Bson),

    /// Returns the value from the last document for each group.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/last/#mongodb-group-grp.-last
    #[serde(rename = "$last")]
    Last(bson::Bson),

    /// Returns the sample standard deviation of the input values.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/stdDevSamp/#mongodb-group-grp.-stdDevSamp
    #[serde(rename = "$stdDevSamp")]
    StdDevSamp(bson::Bson),

    /// Returns the population standard deviation of the input values.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/stdDevPop/#mongodb-group-grp.-stdDevPop
    #[serde(rename = "$stdDevPop")]
    StdDevPop(bson::Bson),

    /// Returns an approximation of the median value. MongoDB 7.0+.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/median/#mongodb-group-grp.-median
    #[serde(rename = "$median")]
    Median(bson::Document),

    /// Returns an approximation of specified percentile values. MongoDB 7.0+.
    ///
    /// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/percentile/#mongodb-group-grp.-percentile
    #[serde(rename = "$percentile")]
    Percentile(bson::Document),
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
