use std::collections::BTreeMap;

use mongodb::bson::{self};
use serde::Serialize;

use dc_api_types::Field;

use crate::mongodb::selection::format_col_path;

/// A projection determines which fields to request from the result of a query.
///
/// See https://www.mongodb.com/docs/manual/reference/operator/aggregation/project/#mongodb-pipeline-pipe.-project
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Projection {
    pub field_projections: BTreeMap<String, ProjectAs>,
}

impl Projection {
    pub fn new<K, T>(fields: T) -> Projection
    where
        T: IntoIterator<Item = (K, ProjectAs)>,
        K: Into<String>,
    {
        Projection {
            field_projections: fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        }
    }

    pub fn for_field_selection<K, T>(field_selection: T) -> Projection
    where
        T: IntoIterator<Item = (K, Field)>,
        K: Into<String>,
    {
        for_field_selection_helper(&[], field_selection)
    }
}

fn for_field_selection_helper<K, T>(parent_columns: &[&str], field_selection: T) -> Projection
where
    T: IntoIterator<Item = (K, Field)>,
    K: Into<String>,
{
    Projection::new(
        field_selection
            .into_iter()
            .map(|(key, value)| (key.into(), project_field_as(parent_columns, &value))),
    )
}

fn project_field_as(parent_columns: &[&str], field: &Field) -> ProjectAs {
    match field {
        Field::Column {
            column,
            column_type,
        } => {
            let col_path = match parent_columns {
                [] => format!("${column}"),
                _ => format!("${}.{}", parent_columns.join("."), column),
            };
            let bson_col_path = format_col_path(col_path, column_type);
            ProjectAs::Expression(bson_col_path)
        }
        Field::NestedObject { column, query } => {
            let nested_parent_columns = append_to_path(parent_columns, column);
            let fields = query.fields.clone().flatten().unwrap_or_default();
            ProjectAs::Nested(for_field_selection_helper(&nested_parent_columns, fields))
        }
        Field::NestedArray {
            field,
            // NOTE: We can use a $slice in our projection to do offsets and limits:
            // https://www.mongodb.com/docs/manual/reference/operator/projection/slice/#mongodb-projection-proj.-slice
            limit: _,
            offset: _,
            r#where: _,
        } => project_field_as(parent_columns, field),
        Field::Relationship {
            query,
            relationship,
        } => {
            // TODO: Need to determine whether the relation type is "object" or "array" and project
            // accordingly
            let nested_parent_columns = append_to_path(parent_columns, relationship);
            let fields = query.fields.clone().flatten().unwrap_or_default();
            ProjectAs::Nested(for_field_selection_helper(&nested_parent_columns, fields))
        }
    }
}

fn append_to_path<'a, 'b, 'c>(parent_columns: &'a [&'b str], column: &'c str) -> Vec<&'c str>
where
    'b: 'c,
{
    parent_columns.iter().copied().chain(Some(column)).collect()
}

impl TryFrom<&Projection> for bson::Document {
    type Error = bson::ser::Error;
    fn try_from(value: &Projection) -> Result<Self, Self::Error> {
        bson::to_document(value)
    }
}

impl TryFrom<Projection> for bson::Document {
    type Error = bson::ser::Error;
    fn try_from(value: Projection) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProjectAs {
    #[allow(dead_code)]
    Included,
    Excluded,
    Expression(bson::Bson),
    Nested(Projection),
}

impl Serialize for ProjectAs {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            ProjectAs::Included => serializer.serialize_u8(1),
            ProjectAs::Excluded => serializer.serialize_u8(0),
            ProjectAs::Expression(v) => v.serialize(serializer),
            ProjectAs::Nested(projection) => projection.serialize(serializer),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use mongodb::bson::{bson, doc, to_bson, to_document};
    use pretty_assertions::assert_eq;
    use serde_json::{from_value, json};

    use super::{ProjectAs, Projection};
    use dc_api_types::{Field, QueryRequest};

    #[test]
    fn serializes_a_projection() -> Result<(), anyhow::Error> {
        let projection = Projection {
            field_projections: [
                ("foo".to_owned(), ProjectAs::Included),
                (
                    "bar".to_owned(),
                    ProjectAs::Nested(Projection {
                        field_projections: [("baz".to_owned(), ProjectAs::Included)].into(),
                    }),
                ),
            ]
            .into(),
        };
        assert_eq!(
            to_bson(&projection)?,
            bson!({
                "foo": 1,
                "bar": {
                    "baz": 1
                }
            })
        );
        Ok(())
    }

    #[test]
    fn calculates_projection_for_fields() -> Result<(), anyhow::Error> {
        let fields: HashMap<String, Field> = from_value(json!({
            "foo": { "type": "column", "column": "foo", "column_type": "String" },
            "foo_again": { "type": "column", "column": "foo", "column_type": "String" },
            "bar": {
                "type": "object",
                "column": "bar",
                "query": {
                    "fields": {
                        "baz": { "type": "column", "column": "baz", "column_type": "String" },
                        "baz_again": { "type": "column", "column": "baz", "column_type": "String" },
                    },
                },
            },
            "bar_again": {
                "type": "object",
                "column": "bar",
                "query": {
                    "fields": {
                        "baz": { "type": "column", "column": "baz", "column_type": "String" },
                    },
                },
            },
            "my_date": { "type": "column", "column": "my_date", "column_type": "date"},
        }))?;
        let projection = Projection::for_field_selection(fields);
        assert_eq!(
            to_document(&projection)?,
            doc! {
               "foo": "$foo",
               "foo_again": "$foo",
               "bar": {
                   "baz": "$bar.baz",
                   "baz_again": "$bar.baz"
               },
               "bar_again": {
                   "baz": "$bar.baz"
               },
               "my_date": {
                    "$dateToString": {
                        "date": "$my_date"
                    }
               }
            }
        );
        Ok(())
    }

    #[test]
    fn produces_projection_for_relation() -> Result<(), anyhow::Error> {
        let query_request: QueryRequest = from_value(json!({
            "query": {
                "fields": {
                    "class_students": {
                        "type": "relationship",
                        "query": {
                            "fields": {
                                "name": { "type": "column", "column": "name", "column_type": "string" },
                            },
                        },
                        "relationship": "class_students",
                    },
                    "students": {
                        "type": "relationship",
                        "query": {
                            "fields": {
                                "student_name": { "type": "column", "column": "name", "column_type": "string" },
                            },
                        },
                        "relationship": "class_students",
                    },
                },
            },
            "target": { "name": ["classes"], "type": "table" },
            "relationships": [{
                "source_table": ["classes"],
                "relationships": {
                    "class_students": {
                        "column_mapping": { "_id": "classId" },
                        "relationship_type": "array",
                        "target": {"name": ["students"], "type": "table"},
                    },
                },
            }],
        }))?;
        let projection =
            Projection::for_field_selection(query_request.query.fields.flatten().unwrap());
        assert_eq!(
            to_document(&projection)?,
            doc! {
                "class_students": {
                    "name": "$class_students.name",
                },
                "students": {
                    "student_name": "$class_students.name",
                },
            }
        );
        Ok(())
    }
}
