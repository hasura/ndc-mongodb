use std::collections::HashMap;

use dc_api_types::{query_request::QueryRequest, Field, TableRelationships};
use mongodb::bson::{self, bson, doc, Bson, Document};
use serde::{Deserialize, Serialize};

use crate::{
    interface_types::MongoAgentError, mongodb::sanitize::get_field, query::is_response_faceted,
};

/// Wraps a BSON document that represents a MongoDB "expression" that constructs a document based
/// on the output of a previous aggregation pipeline stage. A Selection value is intended to be
/// used as the argument to a $replaceWith pipeline stage.
///
/// When we compose pipelines, we can pair each Pipeline with a Selection that extracts the data we
/// want, in the format we want it to provide to HGE. We can collect Selection values and merge
/// them to form one stage after all of the composed pipelines.
///
/// TODO: Do we need a deep/recursive merge for this type?
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Selection(pub bson::Document);

impl Selection {
    pub fn from_doc(doc: bson::Document) -> Self {
        Selection(doc)
    }

    pub fn from_query_request(query_request: &QueryRequest) -> Result<Selection, MongoAgentError> {
        // let fields = (&query_request.query.fields).flatten().unwrap_or_default();
        let empty_map = HashMap::new();
        let fields = if let Some(Some(fs)) = &query_request.query.fields {
            fs
        } else {
            &empty_map
        };
        let doc = from_query_request_helper(&query_request.relationships, &[], fields)?;
        Ok(Selection(doc))
    }
}

fn from_query_request_helper(
    table_relationships: &[TableRelationships],
    parent_columns: &[&str],
    field_selection: &HashMap<String, Field>,
) -> Result<Document, MongoAgentError> {
    field_selection
        .iter()
        .map(|(key, value)| {
            Ok((
                key.into(),
                selection_for_field(table_relationships, parent_columns, key, value)?,
            ))
        })
        .collect()
}

/// If column_type is date we want to format it as a string.
/// TODO: do we want to format any other BSON types in any particular way,
/// e.g. formated ObjectId as string?
pub fn format_col_path(col_path: String, column_type: &str) -> Bson {
    match column_type {
        "date" => bson!({"$dateToString": {"date": col_path}}),
        _ => bson!(col_path),
    }
}

fn selection_for_field(
    table_relationships: &[TableRelationships],
    parent_columns: &[&str],
    field_name: &str,
    field: &Field,
) -> Result<Bson, MongoAgentError> {
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
            Ok(bson_col_path)
        }
        Field::NestedObject { column, query } => {
            let nested_parent_columns = append_to_path(parent_columns, column);
            let nested_parent_col_path = format!("${}", nested_parent_columns.join("."));
            let fields = query.fields.clone().flatten().unwrap_or_default();
            let nested_selection =
                from_query_request_helper(table_relationships, &nested_parent_columns, &fields)?;
            Ok(doc! {"$cond": {"if": nested_parent_col_path, "then": nested_selection, "else": Bson::Null}}.into())
        }
        Field::NestedArray {
            field,
            // NOTE: We can use a $slice in our selection to do offsets and limits:
            // https://www.mongodb.com/docs/manual/reference/operator/projection/slice/#mongodb-projection-proj.-slice
            limit: _,
            offset: _,
            r#where: _,
        } => selection_for_array(table_relationships, parent_columns, field_name, field, 0),
        Field::Relationship { query, .. } => {
            if is_response_faceted(query) {
                Ok(doc! { "$first": get_field(field_name) }.into())
            } else {
                Ok(doc! { "rows": get_field(field_name) }.into())
            }
        }
    }
}

fn selection_for_array(
    table_relationships: &[TableRelationships],
    parent_columns: &[&str],
    field_name: &str,
    field: &Field,
    array_nesting_level: usize,
) -> Result<Bson, MongoAgentError> {
    match field {
        Field::NestedObject { column, query } => {
            let nested_parent_columns = append_to_path(parent_columns, column);
            let nested_parent_col_path = format!("${}", nested_parent_columns.join("."));
            let fields = query.fields.clone().flatten().unwrap_or_default();
            let mut nested_selection =
                from_query_request_helper(table_relationships, &["$this"], &fields)?;
            for _ in 0..array_nesting_level {
                nested_selection = doc! {"$map": {"input": "$$this", "in": nested_selection}}
            }
            let map_expression =
                doc! {"$map": {"input": &nested_parent_col_path, "in": nested_selection}};
            Ok(doc! {"$cond": {"if": &nested_parent_col_path, "then": map_expression, "else": Bson::Null}}.into())
        }
        Field::NestedArray {
            field,
            // NOTE: We can use a $slice in our selection to do offsets and limits:
            // https://www.mongodb.com/docs/manual/reference/operator/projection/slice/#mongodb-projection-proj.-slice
            limit: _,
            offset: _,
            r#where: _,
        } => selection_for_array(
            table_relationships,
            parent_columns,
            field_name,
            field,
            array_nesting_level + 1,
        ),
        _ => selection_for_field(table_relationships, parent_columns, field_name, field),
    }
}
fn append_to_path<'a, 'b, 'c>(parent_columns: &'a [&'b str], column: &'c str) -> Vec<&'c str>
where
    'b: 'c,
{
    parent_columns.iter().copied().chain(Some(column)).collect()
}

/// The extend implementation provides a shallow merge.
impl Extend<(String, Bson)> for Selection {
    fn extend<T: IntoIterator<Item = (String, Bson)>>(&mut self, iter: T) {
        self.0.extend(iter);
    }
}

impl From<Selection> for bson::Document {
    fn from(value: Selection) -> Self {
        value.0
    }
}

// This won't fail, but it might in the future if we add some sort of validation or parsing.
impl TryFrom<bson::Document> for Selection {
    type Error = anyhow::Error;
    fn try_from(value: bson::Document) -> Result<Self, Self::Error> {
        Ok(Selection(value))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use mongodb::bson::{doc, Document};
    use pretty_assertions::assert_eq;
    use serde_json::{from_value, json};

    use super::Selection;
    use dc_api_types::{Field, Query, QueryRequest, Target};

    #[test]
    fn calculates_selection_for_query_request() -> Result<(), anyhow::Error> {
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
            "array_of_scalars": {"type": "array", "field": { "type": "column", "column": "foo", "column_type": "String"}},
            "array_of_objects": {
                "type": "array",
                "field": {
                    "type": "object",
                     "column": "foo",
                     "query": {
                        "fields": {
                            "baz": {"type": "column", "column": "baz", "column_type": "String"}
                        }
                     }
                }
            },
            "array_of_arrays_of_objects": {
                "type": "array",
                "field": {
                    "type": "array",
                    "field": {
                        "type": "object",
                        "column": "foo",
                        "query": {
                            "fields": {
                                "baz": {"type": "column", "column": "baz", "column_type": "String"}
                            }
                        }
                    }
                }
            }
        }))?;

        let query_request = QueryRequest {
            query: Box::new(Query {
                fields: Some(Some(fields)),
                ..Default::default()
            }),
            foreach: None,
            variables: None,
            target: Target::TTable {
                name: vec!["test".to_owned()],
            },
            relationships: vec![],
        };

        let selection = Selection::from_query_request(&query_request)?;
        assert_eq!(
            Into::<Document>::into(selection),
            doc! {
               "foo": "$foo",
               "foo_again": "$foo",
               "bar": {
                   "$cond": {
                        "if": "$bar",
                        "then":  {
                            "baz": "$bar.baz",
                            "baz_again": "$bar.baz"
                        },
                        "else": null
                   }
               },
               "bar_again": {
                    "$cond": {
                        "if": "$bar",
                        "then": {
                            "baz": "$bar.baz"
                        },
                        "else": null
                    }
               },
               "my_date": {
                    "$dateToString": {
                        "date": "$my_date"
                    }
               },
               "array_of_scalars": "$foo",
               "array_of_objects": {
                    "$cond": {
                        "if": "$foo",
                        "then": {
                            "$map": {
                                "input": "$foo",
                                "in": {"baz": "$$this.baz"}
                            }
                        },
                        "else": null
                    }
               },
               "array_of_arrays_of_objects": {
                    "$cond": {
                        "if": "$foo",
                        "then": {
                            "$map": {
                                "input": "$foo",
                                "in": {
                                    "$map": {
                                        "input": "$$this",
                                        "in": {"baz": "$$this.baz"}
                                    }
                                }
                            }
                        },
                        "else": null
                    }
               },
            }
        );
        Ok(())
    }

    #[test]
    fn produces_selection_for_relation() -> Result<(), anyhow::Error> {
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
            "target": {"name": ["classes"], "type": "table"},
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
        let selection = Selection::from_query_request(&query_request)?;
        assert_eq!(
            Into::<Document>::into(selection),
            doc! {
                "class_students": {
                    "rows": {
                        "$getField": { "$literal": "class_students" }
                    },
                },
                "students": {
                    "rows": {
                        "$getField": { "$literal": "students" }
                    },
                },
            }
        );
        Ok(())
    }

    // Same test as above, but using the old query format to test for backwards compatibility
    #[test]
    fn produces_selection_for_relation_compat() -> Result<(), anyhow::Error> {
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
            "table": ["classes"],
            "table_relationships": [{
                "source_table": ["classes"],
                "relationships": {
                    "class_students": {
                        "column_mapping": { "_id": "classId" },
                        "relationship_type": "array",
                        "target_table": ["students"],
                    },
                },
            }],
        }))?;
        let selection = Selection::from_query_request(&query_request)?;
        assert_eq!(
            Into::<Document>::into(selection),
            doc! {
                "class_students": {
                    "rows": {
                        "$getField": { "$literal": "class_students" }
                    },
                },
                "students": {
                    "rows": {
                        "$getField": { "$literal": "students" }
                    },
                },
            }
        );
        Ok(())
    }
}
