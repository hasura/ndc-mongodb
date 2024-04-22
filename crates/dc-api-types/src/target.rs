use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Target {
    #[serde(rename = "table")]
    TTable {
        /// The fully qualified name of a table, where the last item in the array is the table name and any earlier items represent the namespacing of the table name
        #[serde(rename = "name")]
        name: Vec<String>,

        /// This field is not part of the v2 DC Agent API - it is included to support queries
        /// translated from the v3 NDC API. These arguments correspond to `arguments` fields on the
        /// v3 `QueryRequest` and `Relationship` types.
        #[serde(skip, default)]
        arguments: HashMap<String, Argument>,
    }, // TODO: variants TInterpolated and TFunction should be immplemented if/when we add support for (interpolated) native queries and functions
}

impl Target {
    pub fn name(&self) -> &Vec<String> {
        match self {
            Target::TTable { name, .. } => name,
        }
    }

    pub fn arguments(&self) -> &HashMap<String, Argument> {
        match self {
            Target::TTable { arguments, .. } => arguments,
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
            Ok(Target::TTable {
                name,
                arguments: Default::default(),
            })
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

/// Optional arguments to the target of a query request or a relationship. This is a v3 feature
/// which corresponds to the `Argument` and `RelationshipArgument` ndc-client types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Argument {
    /// The argument is provided by reference to a variable
    Variable {
        name: String,
    },
    /// The argument is provided as a literal value
    Literal {
        value: serde_json::Value,
    },
    // The argument is provided based on a column of the source collection
    Column {
        name: String,
    },
}
