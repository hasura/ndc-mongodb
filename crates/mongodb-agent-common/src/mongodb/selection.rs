use indexmap::IndexMap;
use mongodb::bson::{self, doc, Bson, Document};
use serde::{Deserialize, Serialize};

use crate::{
    interface_types::MongoAgentError,
    mongo_query_plan::{Field, NestedArray, NestedField, NestedObject, QueryPlan},
    mongodb::sanitize::get_field,
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
    field_selection: &IndexMap<ndc_models::FieldName, Field>,
) -> Result<Document, MongoAgentError> {
    field_selection
        .iter()
        .map(|(key, value)| Ok((key.to_string(), selection_for_field(parent_columns, value)?)))
        .collect()
}

/// Wraps column reference with an `$isNull` check. That catches cases where a field is missing
/// from a document, and substitutes a concrete null value. Otherwise the field would be omitted
/// from query results which leads to an error in the engine.
fn value_or_null(col_path: String) -> Bson {
    doc! { "$ifNull": [col_path, Bson::Null] }.into()
}

fn selection_for_field(parent_columns: &[&str], field: &Field) -> Result<Bson, MongoAgentError> {
    match field {
        Field::Column {
            column,
            fields: None,
            ..
        } => {
            let col_path = match parent_columns {
                [] => format!("${column}"),
                _ => format!("${}.{}", parent_columns.join("."), column),
            };
            let bson_col_path = value_or_null(col_path);
            Ok(bson_col_path)
        }
        Field::Column {
            column,
            fields: Some(NestedField::Object(NestedObject { fields })),
            ..
        } => {
            let nested_parent_columns = append_to_path(parent_columns, column.as_str());
            let nested_parent_col_path = format!("${}", nested_parent_columns.join("."));
            let nested_selection = from_query_request_helper(&nested_parent_columns, fields)?;
            Ok(doc! {"$cond": {"if": nested_parent_col_path, "then": nested_selection, "else": Bson::Null}}.into())
        }
        Field::Column {
            column,
            fields:
                Some(NestedField::Array(NestedArray {
                    fields: nested_field,
                })),
            ..
        } => selection_for_array(
            &append_to_path(parent_columns, column.as_str()),
            nested_field,
            0,
        ),
        Field::Relationship {
            relationship,
            aggregates,
            fields,
            ..
        } => {
            // The pipeline for the relationship has already selected the requested fields with the
            // appropriate aliases. At this point all we need to do is to prune the selection down
            // to requested fields, omitting fields of the relationship that were selected for
            // filtering and sorting.
            let field_selection: Option<Document> = fields.as_ref().map(|fields| {
                fields
                    .iter()
                    .map(|(field_name, _)| {
                        (
                            field_name.to_string(),
                            format!("$$this.{field_name}").into(),
                        )
                    })
                    .collect()
            });

            if let Some(aggregates) = aggregates {
                let aggregate_selecion: Document = aggregates
                    .iter()
                    .map(|(aggregate_name, _)| {
                        (
                            aggregate_name.to_string(),
                            format!("$$row_set.aggregates.{aggregate_name}").into(),
                        )
                    })
                    .collect();
                let mut new_row_set = doc! { "aggregates": aggregate_selecion };

                if let Some(field_selection) = field_selection {
                    new_row_set.insert(
                        "rows",
                        doc! {
                            "$map": {
                                "input": "$$row_set.rows",
                                "in": field_selection,
                            }
                        },
                    );
                }

                Ok(doc! {
                    "$let": {
                        "vars": { "row_set": { "$first": get_field(relationship.as_str()) } },
                        "in": new_row_set,
                    }
                }
                .into())
            } else if let Some(field_selection) = field_selection {
                Ok(doc! {
                    "rows": {
                        "$map": {
                            "input": get_field(relationship.as_str()),
                            "in": field_selection,
                        }
                    }
                }
                .into())
            } else {
                Ok(doc! { "rows": [] }.into())
            }
        }
    }
}

fn selection_for_array(
    parent_columns: &[&str],
    field: &NestedField,
    array_nesting_level: usize,
) -> Result<Bson, MongoAgentError> {
    match field {
        NestedField::Object(NestedObject { fields }) => {
            let nested_parent_col_path = format!("${}", parent_columns.join("."));
            let mut nested_selection = from_query_request_helper(&["$this"], fields)?;
            for _ in 0..array_nesting_level {
                nested_selection = doc! {"$map": {"input": "$$this", "in": nested_selection}}
            }
            let map_expression =
                doc! {"$map": {"input": &nested_parent_col_path, "in": nested_selection}};
            Ok(doc! {"$cond": {"if": &nested_parent_col_path, "then": map_expression, "else": Bson::Null}}.into())
        }
        NestedField::Array(NestedArray {
            fields: nested_field,
        }) => selection_for_array(parent_columns, nested_field, array_nesting_level + 1),
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
    use configuration::Configuration;
    use mongodb::bson::{doc, Document};
    use ndc_query_plan::plan_for_query_request;
    use ndc_test_helpers::{
        array, array_of, collection, field, named_type, nullable, object, object_type, query,
        query_request, relation_field, relationship,
    };
    use pretty_assertions::assert_eq;

    use crate::mongo_query_plan::MongoConfiguration;

    use super::Selection;

    #[test]
    fn calculates_selection_for_query_request() -> Result<(), anyhow::Error> {
        let query_request = query_request()
            .collection("test")
            .query(query().fields([
                field!("foo"),
                field!("foo_again" => "foo"),
                field!("bar" => "bar", object!([
                    field!("baz"),
                    field!("baz_again" => "baz"),
                ])),
                field!("bar_again" => "bar", object!([
                    field!("baz"),
                ])),
                field!("array_of_scalars" => "xs"),
                field!("array_of_objects" => "os", array!(object!([
                    field!("cat")
                ]))),
                field!("array_of_arrays_of_objects" => "oss", array!(array!(object!([
                    field!("cat")
                ])))),
            ]))
            .into();

        let query_plan = plan_for_query_request(&foo_config(), query_request)?;

        let selection = Selection::from_query_request(&query_plan)?;
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
               "array_of_scalars": { "$ifNull": ["$xs", null] },
               "array_of_objects": {
                    "$cond": {
                        "if": "$os",
                        "then": {
                            "$map": {
                                "input": "$os",
                                "in": {"cat": { "$ifNull": ["$$this.cat", null] }}
                            }
                        },
                        "else": null
                    }
               },
               "array_of_arrays_of_objects": {
                    "$cond": {
                        "if": "$oss",
                        "then": {
                            "$map": {
                                "input": "$oss",
                                "in": {
                                    "$map": {
                                        "input": "$$this",
                                        "in": {"cat": { "$ifNull": ["$$this.cat", null] }}
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
        let query_request = query_request()
            .collection("classes")
            .query(query().fields([
                relation_field!("class_students" => "class_students", query().fields([
                    field!("name")
                ])),
                relation_field!("students" => "class_students", query().fields([
                    field!("student_name" => "name")
                ])),
            ]))
            .relationships([(
                "class_students",
                relationship("students", [("_id", "classId")]),
            )])
            .into();

        let query_plan = plan_for_query_request(&students_config(), query_request)?;

        // TODO: MDB-164 This selection illustrates that we end up looking up the relationship
        // twice (once with the key `class_students`, and then with the key `class_students_0`).
        // This is because the queries on the two relationships have different scope names. The
        // query would work with just one lookup. Can we do that optimization?
        let selection = Selection::from_query_request(&query_plan)?;
        assert_eq!(
            Into::<Document>::into(selection),
            doc! {
                "class_students": {
                    "rows": {
                        "$map": {
                            "input": { "$getField": { "$literal": "class_students" } },
                            "in": {
                                "name": "$$this.name"
                            },
                        },
                    },
                },
                "students": {
                    "rows": {
                        "$map": {
                            "input": { "$getField": { "$literal": "class_students_0" } },
                            "in": {
                                "student_name": "$$this.student_name"
                            },
                        },
                    },
                },
            }
        );
        Ok(())
    }

    fn students_config() -> MongoConfiguration {
        MongoConfiguration(Configuration {
            collections: [collection("classes"), collection("students")].into(),
            object_types: [
                (
                    "assignments".into(),
                    object_type([
                        ("_id", named_type("ObjectId")),
                        ("student_id", named_type("ObjectId")),
                        ("title", named_type("String")),
                    ]),
                ),
                (
                    "classes".into(),
                    object_type([
                        ("_id", named_type("ObjectId")),
                        ("title", named_type("String")),
                        ("year", named_type("Int")),
                    ]),
                ),
                (
                    "students".into(),
                    object_type([
                        ("_id", named_type("ObjectId")),
                        ("classId", named_type("ObjectId")),
                        ("gpa", named_type("Double")),
                        ("name", named_type("String")),
                        ("year", named_type("Int")),
                    ]),
                ),
            ]
            .into(),
            functions: Default::default(),
            procedures: Default::default(),
            native_mutations: Default::default(),
            native_queries: Default::default(),
            options: Default::default(),
        })
    }

    fn foo_config() -> MongoConfiguration {
        MongoConfiguration(Configuration {
            collections: [collection("test")].into(),
            object_types: [
                (
                    "test".into(),
                    object_type([
                        ("foo", nullable(named_type("String"))),
                        ("bar", nullable(named_type("bar"))),
                        ("xs", nullable(array_of(nullable(named_type("Int"))))),
                        ("os", nullable(array_of(nullable(named_type("os"))))),
                        (
                            "oss",
                            nullable(array_of(nullable(array_of(nullable(named_type("os")))))),
                        ),
                    ]),
                ),
                (
                    "bar".into(),
                    object_type([("baz", nullable(named_type("String")))]),
                ),
                (
                    "os".into(),
                    object_type([("cat", nullable(named_type("String")))]),
                ),
            ]
            .into(),
            functions: Default::default(),
            procedures: Default::default(),
            native_mutations: Default::default(),
            native_queries: Default::default(),
            options: Default::default(),
        })
    }
}
