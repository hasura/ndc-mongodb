use indexmap::IndexMap;
use mongodb::bson::{self, doc, Bson, Document};
use serde::{Deserialize, Serialize};

use crate::{
    interface_types::MongoAgentError,
    mongo_query_plan::{Field, QueryPlan, Type},
    mongodb::sanitize::get_field,
    query::{is_response_faceted, serialization::is_nullable},
};

/// Wraps a BSON document that represents a MongoDB "expression" that constructs a document based
/// on the output of a previous aggregation pipeline stage. A Selection value is intended to be
/// used as the argument to a $replaceWith pipeline stage.
///
/// When we compose pipelines, we can pair each Pipeline with a Selection that extracts the data we
/// want, in the format we want it to provide to HGE. We can collect Selection values and merge
/// them to form one stage after all of the composed pipelines.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Selection(pub bson::Document);

impl Selection {
    pub fn from_doc(doc: bson::Document) -> Self {
        Selection(doc)
    }

    pub fn from_query_request(query_request: &QueryPlan) -> Result<Selection, MongoAgentError> {
        // let fields = (&query_request.query.fields).flatten().unwrap_or_default();
        let empty_map = IndexMap::new();
        let fields = if let Some(fs) = &query_request.query.fields {
            fs
        } else {
            &empty_map
        };
        let doc = from_query_request_helper(&[], fields)?;
        Ok(Selection(doc))
    }
}

fn from_query_request_helper(
    parent_columns: &[&str],
    field_selection: &IndexMap<String, Field>,
) -> Result<Document, MongoAgentError> {
    field_selection
        .iter()
        .map(|(key, value)| Ok((key.into(), selection_for_field(parent_columns, key, value)?)))
        .collect()
}

/// Wraps column reference with an `$isNull` check. That catches cases where a field is missing
/// from a document, and substitutes a concrete null value. Otherwise the field would be omitted
/// from query results which leads to an error in the engine.
pub fn value_or_null(col_path: String, column_type: &Type) -> Bson {
    if is_nullable(column_type) {
        doc! { "$ifNull": [col_path, Bson::Null] }.into()
    } else {
        col_path.into()
    }
}

fn selection_for_field(
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
            let bson_col_path = value_or_null(col_path, column_type);
            Ok(bson_col_path)
        }
        Field::NestedObject { column, query } => {
            let nested_parent_columns = append_to_path(parent_columns, column);
            let nested_parent_col_path = format!("${}", nested_parent_columns.join("."));
            let fields = query.fields.clone().unwrap_or_default();
            let nested_selection = from_query_request_helper(&nested_parent_columns, &fields)?;
            Ok(doc! {"$cond": {"if": nested_parent_col_path, "then": nested_selection, "else": Bson::Null}}.into())
        }
        Field::NestedArray {
            field,
            // NOTE: We can use a $slice in our selection to do offsets and limits:
            // https://www.mongodb.com/docs/manual/reference/operator/projection/slice/#mongodb-projection-proj.-slice
            limit: _,
            offset: _,
            predicate: _,
        } => selection_for_array(parent_columns, field_name, field, 0),
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
    parent_columns: &[&str],
    field_name: &str,
    field: &Field,
    array_nesting_level: usize,
) -> Result<Bson, MongoAgentError> {
    match field {
        Field::NestedObject { column, query } => {
            let nested_parent_columns = append_to_path(parent_columns, column);
            let nested_parent_col_path = format!("${}", nested_parent_columns.join("."));
            let fields = query.fields.clone().unwrap_or_default();
            let mut nested_selection = from_query_request_helper(&["$this"], &fields)?;
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
            predicate: _,
        } => selection_for_array(parent_columns, field_name, field, array_nesting_level + 1),
        _ => selection_for_field(parent_columns, field_name, field),
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
                fields: Some(fields),
                ..Default::default()
            }),
            foreach: None,
            variables: None,
            target: Target::TTable {
                name: vec!["test".to_owned()],
                arguments: Default::default(),
            },
            relationships: vec![],
        };

        let selection = Selection::from_query_request(&query_request)?;
        assert_eq!(
            Into::<Document>::into(selection),
            doc! {
               "foo": { "$ifNull": ["$foo", null] },
               "foo_again": { "$ifNull": ["$foo", null] },
               "bar": {
                   "$cond": {
                        "if": "$bar",
                        "then":  {
                            "baz": { "$ifNull": ["$bar.baz", null] },
                            "baz_again": { "$ifNull": ["$bar.baz", null] }
                        },
                        "else": null
                   }
               },
               "bar_again": {
                    "$cond": {
                        "if": "$bar",
                        "then": {
                            "baz": { "$ifNull": ["$bar.baz", null] }
                        },
                        "else": null
                    }
               },
               "my_date": {
                    "$dateToString": {
                        "date": { "$ifNull": ["$my_date", null] }
                    }
               },
               "array_of_scalars": { "$ifNull": ["$foo", null] },
               "array_of_objects": {
                    "$cond": {
                        "if": "$foo",
                        "then": {
                            "$map": {
                                "input": "$foo",
                                "in": {"baz": { "$ifNull": ["$$this.baz", null] }}
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
                                        "in": {"baz": { "$ifNull": ["$$this.baz", null] }}
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
