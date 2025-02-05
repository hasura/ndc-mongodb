use indexmap::IndexMap;
use mongodb::bson::{doc, Bson, Document};
use mongodb_support::aggregate::Selection;
use ndc_models::FieldName;
use nonempty::NonEmpty;

use crate::{
    constants::{ROW_SET_AGGREGATES_KEY, ROW_SET_GROUPS_KEY, ROW_SET_ROWS_KEY},
    interface_types::MongoAgentError,
    mongo_query_plan::{Field, NestedArray, NestedField, NestedObject},
    mongodb::sanitize::get_field,
    query::{column_ref::ColumnRef, groups::selection_for_grouping},
};

use super::is_response_faceted::ResponseFacets;

/// Creates a document to use in a $replaceWith stage to limit query results to the specific fields
/// requested. Assumes that only fields are requested.
pub fn selection_for_fields(
    fields: Option<&IndexMap<FieldName, Field>>,
) -> Result<Selection, MongoAgentError> {
    let empty_map = IndexMap::new();
    let fields = if let Some(fs) = fields {
        fs
    } else {
        &empty_map
    };
    let doc = for_fields_helper(None, fields)?;
    Ok(Selection::new(doc))
}

fn for_fields_helper(
    parent: Option<ColumnRef<'_>>,
    field_selection: &IndexMap<ndc_models::FieldName, Field>,
) -> Result<Document, MongoAgentError> {
    field_selection
        .iter()
        .map(|(key, value)| Ok((key.to_string(), selection_for_field(parent.clone(), value)?)))
        .collect()
}

/// Wraps column reference with an `$isNull` check. That catches cases where a field is missing
/// from a document, and substitutes a concrete null value. Otherwise the field would be omitted
/// from query results which leads to an error in the engine.
fn value_or_null(value: Bson) -> Bson {
    doc! { "$ifNull": [value, Bson::Null] }.into()
}

fn selection_for_field(
    parent: Option<ColumnRef<'_>>,
    field: &Field,
) -> Result<Bson, MongoAgentError> {
    match field {
        Field::Column {
            column,
            fields: None,
            ..
        } => {
            let col_ref = nested_column_reference(parent, column);
            let col_ref_or_null = value_or_null(col_ref.into_aggregate_expression().into_bson());
            Ok(col_ref_or_null)
        }
        Field::Column {
            column,
            fields: Some(NestedField::Object(NestedObject { fields })),
            ..
        } => {
            let col_ref = nested_column_reference(parent, column);
            let nested_selection = for_fields_helper(Some(col_ref.clone()), fields)?;
            Ok(doc! {"$cond": {"if": col_ref.into_aggregate_expression(), "then": nested_selection, "else": Bson::Null}}.into())
        }
        Field::Column {
            column,
            fields:
                Some(NestedField::Array(NestedArray {
                    fields: nested_field,
                })),
            ..
        } => selection_for_array(nested_column_reference(parent, column), nested_field, 0),
        Field::Relationship {
            relationship,
            aggregates,
            fields,
            groups,
            ..
        } => {
            // TODO: If we get a unification of two relationship references where one selects only
            // fields, and the other selects only groups, we may end up in a broken state where the
            // response should be faceted but is not. Data will be populated correctly - the issue
            // is only here where we need to figure out whether to write a selection for faceted
            // data or not. Instead of referencing the [Field::Relationship] value to determine
            // faceting we need to reference the [Relationship] attached to the [Query] that
            // populated it.

            // The pipeline for the relationship has already selected the requested fields with the
            // appropriate aliases. At this point all we need to do is to prune the selection down
            // to requested fields, omitting fields of the relationship that were selected for
            // filtering and sorting.
            let field_selection = |fields: &IndexMap<FieldName, Field>| -> Document {
                fields
                    .iter()
                    .map(|(field_name, _)| {
                        (
                            field_name.to_string(),
                            ColumnRef::variable("this")
                                .into_nested_field(field_name)
                                .into_aggregate_expression()
                                .into_bson(),
                        )
                    })
                    .collect()
            };

            // // As in field_selection, we don't need all of the logic for grouping selection. We
            // // only need to prune down to the fields requested by this specific field reference.
            // let group_selection = |grouping: &Grouping| -> Document {
            //     grouping.aggregates
            // };

            // Field of the incoming pipeline document that contains data fetched for the
            // relationship.
            let relationship_field = get_field(relationship.as_str());

            let doc = match ResponseFacets::from_parameters(
                aggregates.as_ref(),
                fields.as_ref(),
                groups.as_ref(),
            ) {
                ResponseFacets::Combination {
                    aggregates,
                    fields,
                    groups,
                } => {
                    let aggregate_selection: Document = aggregates
                        .into_iter()
                        .flatten()
                        .map(|(aggregate_name, _)| {
                            (
                                aggregate_name.to_string(),
                                format!("$$row_set.{ROW_SET_AGGREGATES_KEY}.{aggregate_name}")
                                    .into(),
                            )
                        })
                        .collect();
                    let mut new_row_set = doc! { ROW_SET_AGGREGATES_KEY: aggregate_selection };

                    if let Some(fields) = fields {
                        new_row_set.insert(
                            ROW_SET_ROWS_KEY,
                            doc! {
                                "$map": {
                                    "input": format!("$$row_set.{ROW_SET_ROWS_KEY}"),
                                    "in": field_selection(fields),
                                }
                            },
                        );
                    }

                    if let Some(grouping) = groups {
                        new_row_set.insert(
                            ROW_SET_GROUPS_KEY,
                            doc! {
                                "$map": {
                                    "input": format!("$$row_set.{ROW_SET_GROUPS_KEY}"),
                                    "as": "CURRENT", // implicitly changes the document root in `in` to be the array element
                                    "in": selection_for_grouping(grouping),
                                }
                            },
                        );
                    }

                    doc! {
                        "$let": {
                            "vars": { "row_set": { "$first": relationship_field } },
                            "in": new_row_set,
                        }
                    }
                }
                ResponseFacets::FieldsOnly(fields) => doc! {
                    ROW_SET_ROWS_KEY: {
                        "$map": {
                            "input": relationship_field,
                            "in": field_selection(fields),
                        }
                    }
                },
                ResponseFacets::GroupsOnly(grouping) => doc! {
                    // We can reuse the grouping selection logic instead of writing a custom one
                    // like with `field_selection` because `selection_for_grouping` only selects
                    // top-level keys - it doesn't have logic that we don't want to duplicate like
                    // `selection_for_field` does.
                    ROW_SET_GROUPS_KEY: {
                        "$map": {
                            "input": relationship_field,
                            "as": "CURRENT", // implicitly changes the document root in `in` to be the array element
                            "in": selection_for_grouping(grouping),
                        }
                    }
                },
            };
            Ok(doc.into())
        }
    }
}

fn selection_for_array(
    parent: ColumnRef<'_>,
    field: &NestedField,
    array_nesting_level: usize,
) -> Result<Bson, MongoAgentError> {
    match field {
        NestedField::Object(NestedObject { fields }) => {
            let mut nested_selection =
                for_fields_helper(Some(ColumnRef::variable("this")), fields)?;
            for _ in 0..array_nesting_level {
                nested_selection = doc! {"$map": {"input": "$$this", "in": nested_selection}}
            }
            let map_expression = doc! {"$map": {"input": parent.clone().into_aggregate_expression(), "in": nested_selection}};
            Ok(doc! {"$cond": {"if": parent.into_aggregate_expression(), "then": map_expression, "else": Bson::Null}}.into())
        }
        NestedField::Array(NestedArray {
            fields: nested_field,
        }) => selection_for_array(parent, nested_field, array_nesting_level + 1),
    }
}

fn nested_column_reference<'a>(
    parent: Option<ColumnRef<'a>>,
    column: &'a FieldName,
) -> ColumnRef<'a> {
    match parent {
        Some(parent) => parent.into_nested_field(column),
        None => ColumnRef::from_field_path(NonEmpty::singleton(column)),
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

    use super::*;

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

        let selection = selection_for_fields(query_plan.query.fields.as_ref())?;
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
                                "in": {
                                    "cat": {
                                        "$ifNull": ["$$this.cat", null]
                                    }
                                }
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
                                        "in": {
                                            "cat": {
                                                "$ifNull": ["$$this.cat", null]
                                            }
                                        }
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
                relationship("students", [("_id", &["classId"])]),
            )])
            .into();

        let query_plan = plan_for_query_request(&students_config(), query_request)?;

        // TODO: MDB-164 This selection illustrates that we end up looking up the relationship
        // twice (once with the key `class_students`, and then with the key `class_students_0`).
        // This is because the queries on the two relationships have different scope names. The
        // query would work with just one lookup. Can we do that optimization?
        let selection = selection_for_fields(query_plan.query.fields.as_ref())?;
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
