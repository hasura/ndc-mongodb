use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Target {
    #[serde(rename = "table")]
    TTable {
        /// The fully qualified name of a table, where the last item in the array is the table name and any earlier items represent the namespacing of the table name
        #[serde(rename = "name")]
        name: Vec<String>,
    }, // TODO: variants TInterpolated and TFunction should be immplemented if/when we add support for (interpolated) native queries and functions
}

impl Target {
    pub fn name(&self) -> &Vec<String> {
        match self {
            Target::TTable { name } => name,
        }
    }
}

// Allow a table name (represented as a Vec<String>) to be deserialized into a Target::TTable.
// This provides backwards compatibility with previous version of DC API.
pub fn target_or_table_name<'de, D>(deserializer: D) -> Result<Target, D::Error>
where
    D: Deserializer<'de>,
{
    struct TargetOrTableName;

    impl<'de> Visitor<'de> for TargetOrTableName {
        type Value = Target;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("Target or TableName")
        }

        fn visit_seq<A>(self, seq: A) -> Result<Target, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let name = Deserialize::deserialize(de::value::SeqAccessDeserializer::new(seq))?;
            Ok(Target::TTable { name })
        }

        fn visit_map<M>(self, map: M) -> Result<Target, M::Error>
        where
            M: MapAccess<'de>,
        {
            Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_any(TargetOrTableName)
}
