use serde::{de, ser::SerializeMap, Deserialize, Serialize};

use crate::{GraphQLName, GqlName};

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum ColumnType {
    Scalar(String),
    #[serde(deserialize_with = "parse_object")]
    Object(GraphQLName),
    Array {
        element_type: Box<ColumnType>,
        nullable: bool,
    },
}

impl Serialize for ColumnType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            ColumnType::Scalar(s) => serializer.serialize_str(s),
            ColumnType::Object(s) => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "object")?;
                map.serialize_entry("name", s)?;
                map.end()
            }
            ColumnType::Array {
                element_type,
                nullable,
            } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("type", "array")?;
                map.serialize_entry("element_type", element_type)?;
                map.serialize_entry("nullable", nullable)?;
                map.end()
            }
        }
    }
}

fn parse_object<'de, D>(deserializer: D) -> Result<GraphQLName, D::Error>
where
    D: de::Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(deserializer)?;
    let obj = v.as_object().and_then(|o| o.get("name"));

    match obj {
        Some(name) => match name.as_str() {
            Some(s) => Ok(GqlName::from_trusted_safe_str(s).into_owned()),
            None => Err(de::Error::custom("invalid value")),
        },
        _ => Err(de::Error::custom("invalid value")),
    }
}

#[cfg(test)]
mod test {
    use mongodb::bson::{bson, from_bson, to_bson};

    use super::ColumnType;

    #[test]
    fn serialize_scalar() -> Result<(), anyhow::Error> {
        let input = ColumnType::Scalar("string".to_owned());
        assert_eq!(to_bson(&input)?, bson!("string".to_owned()));
        Ok(())
    }

    #[test]
    fn serialize_object() -> Result<(), anyhow::Error> {
        let input = ColumnType::Object("documents_place".into());
        assert_eq!(
            to_bson(&input)?,
            bson!({"type": "object".to_owned(), "name": "documents_place".to_owned()})
        );
        Ok(())
    }

    #[test]
    fn serialize_array() -> Result<(), anyhow::Error> {
        let input = ColumnType::Array {
            element_type: Box::new(ColumnType::Scalar("string".to_owned())),
            nullable: false,
        };
        assert_eq!(
            to_bson(&input)?,
            bson!(
                {
                    "type": "array".to_owned(),
                    "element_type": "string".to_owned(),
                    "nullable": false
                }
            )
        );
        Ok(())
    }

    #[test]
    fn parses_scalar() -> Result<(), anyhow::Error> {
        let input = bson!("string".to_owned());
        assert_eq!(
            from_bson::<ColumnType>(input)?,
            ColumnType::Scalar("string".to_owned())
        );
        Ok(())
    }

    #[test]
    fn parses_object() -> Result<(), anyhow::Error> {
        let input = bson!({"type": "object".to_owned(), "name": "documents_place".to_owned()});
        assert_eq!(
            from_bson::<ColumnType>(input)?,
            ColumnType::Object("documents_place".into())
        );
        Ok(())
    }

    #[test]
    fn parses_array() -> Result<(), anyhow::Error> {
        let input = bson!(
            {
            "type": "array".to_owned(),
            "element_type": "string".to_owned(),
            "nullable": false
            }
        );
        assert_eq!(
            from_bson::<ColumnType>(input)?,
            ColumnType::Array {
                element_type: Box::new(ColumnType::Scalar("string".to_owned())),
                nullable: false,
            }
        );
        Ok(())
    }
}
