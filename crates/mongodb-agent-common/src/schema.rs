use indexmap::IndexMap;
use mongodb_support::{BsonScalarType, BsonType};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct ValidatorSchema {
    #[serde(rename = "bsonType", alias = "type", default = "default_bson_type")]
    #[allow(dead_code)]
    pub bson_type: BsonType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub properties: IndexMap<String, Property>,
}

#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(tag = "bsonType", rename_all = "camelCase")]
pub enum Property {
    Object {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        required: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        properties: Option<IndexMap<String, Property>>,
    },
    Array {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        items: Box<Property>,
    },
    #[serde(untagged)]
    Scalar {
        #[serde(rename = "bsonType", default = "default_bson_scalar_type")]
        bson_type: BsonScalarType,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
}

pub fn get_property_description(p: &Property) -> Option<String> {
    match p {
        Property::Object {
            description,
            required: _,
            properties: _,
        } => description.clone(),
        Property::Array {
            description,
            items: _,
        } => description.clone(),
        Property::Scalar {
            bson_type: _,
            description,
        } => description.clone(),
    }
}

fn default_bson_scalar_type() -> BsonScalarType {
    BsonScalarType::Undefined
}

fn default_bson_type() -> BsonType {
    BsonType::Scalar(default_bson_scalar_type())
}

#[cfg(test)]
mod test {
    use indexmap::IndexMap;
    use mongodb::bson::{bson, from_bson};
    use mongodb_support::{BsonScalarType, BsonType};
    use pretty_assertions::assert_eq;

    use super::{Property, ValidatorSchema};

    #[test]
    fn parses_scalar_property() -> Result<(), anyhow::Error> {
        let input = bson!({
          "bsonType": "string",
          "description": "'title' must be a string and is required"
        });

        assert_eq!(
            from_bson::<Property>(input)?,
            Property::Scalar {
                bson_type: BsonScalarType::String,
                description: Some("'title' must be a string and is required".to_owned())
            }
        );

        Ok(())
    }

    #[test]
    fn parses_object_property() -> Result<(), anyhow::Error> {
        let input = bson!({
          "bsonType": "object",
          "description": "Name of places",
          "required": [ "name", "description" ],
          "properties": {
            "name": {
              "bsonType": "string",
              "description": "'name' must be a string and is required"
            },
            "description": {
              "bsonType": "string",
              "description": "'description' must be a string and is required"
            }
          }
        });

        assert_eq!(
            from_bson::<Property>(input)?,
            Property::Object {
                description: Some("Name of places".to_owned()),
                required: vec!["name".to_owned(), "description".to_owned()],
                properties: Some(IndexMap::from([
                    (
                        "name".to_owned(),
                        Property::Scalar {
                            bson_type: BsonScalarType::String,
                            description: Some("'name' must be a string and is required".to_owned())
                        }
                    ),
                    (
                        "description".to_owned(),
                        Property::Scalar {
                            bson_type: BsonScalarType::String,
                            description: Some(
                                "'description' must be a string and is required".to_owned()
                            )
                        }
                    )
                ]))
            }
        );

        Ok(())
    }

    #[test]
    fn parses_array_property() -> Result<(), anyhow::Error> {
        let input = bson!({
          "bsonType": "array",
          "description": "Location must be an array of objects",
          "uniqueItems": true,
          "items": {
            "bsonType": "object",
            "required": [ "name", "size" ],
            "properties": { "name": { "bsonType": "string" }, "size": { "bsonType": "number" } }
          }
        });

        assert_eq!(
            from_bson::<Property>(input)?,
            Property::Array {
                description: Some("Location must be an array of objects".to_owned()),
                items: Box::new(Property::Object {
                    description: None,
                    required: vec!["name".to_owned(), "size".to_owned()],
                    properties: Some(IndexMap::from([
                        (
                            "name".to_owned(),
                            Property::Scalar {
                                bson_type: BsonScalarType::String,
                                description: None
                            }
                        ),
                        (
                            "size".to_owned(),
                            Property::Scalar {
                                bson_type: BsonScalarType::Double,
                                description: None
                            }
                        )
                    ]))
                }),
            }
        );

        Ok(())
    }

    #[test]
    fn parses_validator_with_alias_field_name() -> Result<(), anyhow::Error> {
        let input = bson!({
            "bsonType": "object",
            "properties": {
                "count": {
                    "bsonType": "number",
                },
            },
            "required": ["count"],
        });

        assert_eq!(
            from_bson::<ValidatorSchema>(input)?,
            ValidatorSchema {
                bson_type: BsonType::Object,
                description: None,
                required: vec!["count".to_owned()],
                properties: IndexMap::from([(
                    "count".to_owned(),
                    Property::Scalar {
                        bson_type: BsonScalarType::Double,
                        description: None,
                    }
                )])
            }
        );
        Ok(())
    }

    #[test]
    fn parses_validator_property_as_object() -> Result<(), anyhow::Error> {
        let input = bson!({
            "bsonType": "object",
            "properties": {
                "counts": {
                    "bsonType": "object",
                    "properties": {
                        "xs": { "bsonType": "number" },
                        "os": { "bsonType": "number" },
                    },
                    "required": ["xs"],
                },
            },
            "required": ["counts"],
        });

        assert_eq!(
            from_bson::<ValidatorSchema>(input)?,
            ValidatorSchema {
                bson_type: BsonType::Object,
                description: None,
                required: vec!["counts".to_owned()],
                properties: IndexMap::from([(
                    "counts".to_owned(),
                    Property::Object {
                        description: None,
                        required: vec!["xs".to_owned()],
                        properties: Some(IndexMap::from([
                            (
                                "xs".to_owned(),
                                Property::Scalar {
                                    bson_type: BsonScalarType::Double,
                                    description: None
                                }
                            ),
                            (
                                "os".to_owned(),
                                Property::Scalar {
                                    bson_type: BsonScalarType::Double,
                                    description: None
                                }
                            ),
                        ]))
                    }
                )])
            }
        );
        Ok(())
    }

    /// This validator is from a test collection that the frontend team uses.
    /// https://github.com/hasura/graphql-engine-mono/blob/main/frontend/docker/DataSources/mongo/init.js
    #[test]
    fn parses_frontend_team_test_validator_students() -> Result<(), anyhow::Error> {
        let input = bson!({
            "bsonType": "object",
            "title": "Student Object Validation",
            "required": ["address", "gpa", "name", "year"],
            "properties": {
                "name": {
                    "bsonType": "string",
                    "description": "\"name\" must be a string and is required"
                },
                "year": {
                    "bsonType": "int",
                    "minimum": 2017,
                    "maximum": 3017,
                    "description": "\"year\" must be an integer in [ 2017, 3017 ] and is required"
                },
                "gpa": {
                    "bsonType": ["double"],
                    "description": "\"gpa\" must be a double if the field exists"
                },
                "address": {
                    "bsonType": "object",
                    "properties": {
                        "city": { "bsonType": "string" },
                        "street": { "bsonType": "string" }
                    },
                },
            },
          }
        );
        assert_eq!(
            from_bson::<ValidatorSchema>(input)?,
            ValidatorSchema {
                bson_type: BsonType::Object,
                description: None,
                required: ["address", "gpa", "name", "year"]
                    .into_iter()
                    .map(|s| s.to_owned())
                    .collect(),
                properties: IndexMap::from([
                    (
                        "name".to_owned(),
                        Property::Scalar {
                            bson_type: BsonScalarType::String,
                            description: Some(
                                "\"name\" must be a string and is required".to_owned()
                            ),
                        }
                    ),
                    (
                        "year".to_owned(),
                        Property::Scalar {
                            bson_type: BsonScalarType::Int,
                            description: Some(
                                "\"year\" must be an integer in [ 2017, 3017 ] and is required"
                                    .to_owned()
                            ),
                        }
                    ),
                    (
                        "gpa".to_owned(),
                        Property::Scalar {
                            bson_type: BsonScalarType::Double,
                            description: Some(
                                "\"gpa\" must be a double if the field exists".to_owned()
                            ),
                        }
                    ),
                    (
                        "address".to_owned(),
                        Property::Object {
                            description: None,
                            required: vec![],
                            properties: Some(IndexMap::from([
                                (
                                    "city".to_owned(),
                                    Property::Scalar {
                                        bson_type: BsonScalarType::String,
                                        description: None,
                                    }
                                ),
                                (
                                    "street".to_owned(),
                                    Property::Scalar {
                                        bson_type: BsonScalarType::String,
                                        description: None,
                                    }
                                )
                            ]))
                        }
                    )
                ]),
            }
        );
        Ok(())
    }
}
