use std::collections::BTreeMap;

use itertools::Itertools as _;
use mongodb::bson::{doc, Bson, Document};
use ndc_query_plan::VariableSet;

use crate::mongo_query_plan::{MongoConfiguration, Query, QueryPlan};
use crate::mongodb::sanitize::safe_name;
use crate::mongodb::Pipeline;
use crate::{
    interface_types::MongoAgentError,
    mongodb::{sanitize::variable, Stage},
};

use super::pipeline::pipeline_for_non_foreach;

type Result<T> = std::result::Result<T, MongoAgentError>;

/// Defines any necessary $lookup stages for the given section of the pipeline. This is called for
/// each sub-query in the plan.
pub fn pipeline_for_relations(
    config: &MongoConfiguration,
    variables: Option<&VariableSet>,
    query_plan: &QueryPlan,
) -> Result<Pipeline> {
    let QueryPlan { query, .. } = query_plan;
    let Query { relationships, .. } = query;

    // Lookup stages perform the join for each relationship, and assign the list of rows or mapping
    // of aggregate results to a field in the parent document.
    let lookup_stages = relationships
        .iter()
        .map(|(name, relationship)| {
            // Recursively build pipeline according to relation query
            let lookup_pipeline = pipeline_for_non_foreach(
                config,
                variables,
                &QueryPlan {
                    query: relationship.query.clone(),
                    collection: relationship.target_collection.clone(),
                    ..query_plan.clone()
                },
            )?;

            make_lookup_stage(
                relationship.target_collection.clone(),
                &relationship.column_mapping,
                name.to_owned(),
                lookup_pipeline,
            )
        })
        .try_collect()?;

    Ok(lookup_stages)
}

fn make_lookup_stage(
    from: String,
    column_mapping: &BTreeMap<String, String>,
    r#as: String,
    lookup_pipeline: Pipeline,
) -> Result<Stage> {
    // If we are mapping a single field in the source collection to a single field in the target
    // collection then we can use the correlated subquery syntax.
    if column_mapping.len() == 1 {
        // Safe to unwrap because we just checked the hashmap size
        let (source_selector, target_selector) = column_mapping.iter().next().unwrap();
        single_column_mapping_lookup(
            from,
            source_selector,
            target_selector,
            r#as,
            lookup_pipeline,
        )
    } else {
        multiple_column_mapping_lookup(from, column_mapping, r#as, lookup_pipeline)
    }
}

fn single_column_mapping_lookup(
    from: String,
    source_selector: &str,
    target_selector: &str,
    r#as: String,
    lookup_pipeline: Pipeline,
) -> Result<Stage> {
    Ok(Stage::Lookup {
        from: Some(from),
        local_field: Some(safe_name(source_selector)?.into_owned()),
        foreign_field: Some(safe_name(target_selector)?.into_owned()),
        r#let: None,
        pipeline: if lookup_pipeline.is_empty() {
            None
        } else {
            Some(lookup_pipeline)
        },
        r#as,
    })
}

fn multiple_column_mapping_lookup(
    from: String,
    column_mapping: &BTreeMap<String, String>,
    r#as: String,
    lookup_pipeline: Pipeline,
) -> Result<Stage> {
    let let_bindings: Document = column_mapping
        .keys()
        .map(|local_field| {
            Ok((
                variable(local_field)?,
                Bson::String(format!("${}", safe_name(local_field)?.into_owned())),
            ))
        })
        .collect::<Result<_>>()?;

    // Creating an intermediate Vec and sorting it is done just to help with testing.
    // A stable order for matchers makes it easier to assert equality between actual
    // and expected pipelines.
    let mut column_pairs: Vec<(&String, &String)> = column_mapping.iter().collect();
    column_pairs.sort();

    let matchers: Vec<Document> = column_pairs
        .into_iter()
        .map(|(local_field, remote_field)| {
            Ok(doc! { "$eq": [
                format!("$${}", variable(local_field)?),
                format!("${}", safe_name(remote_field)?)
            ] })
        })
        .collect::<Result<_>>()?;

    // Match only documents on the right side of the join that match the column-mapping
    // criteria. In the case where we have only one column mapping using the $lookup stage's
    // `local_field` and `foreign_field` shorthand would give better performance (~10%), but that
    // locks us into MongoDB v5.0 or later.
    let mut pipeline = Pipeline::from_iter([Stage::Match(if matchers.len() == 1 {
        doc! { "$expr": matchers.into_iter().next().unwrap() }
    } else {
        doc! { "$expr": { "$and": matchers } }
    })]);
    pipeline.append(lookup_pipeline);
    let pipeline: Option<Pipeline> = pipeline.into();

    Ok(Stage::Lookup {
        from: Some(from),
        local_field: None,
        foreign_field: None,
        r#let: let_bindings.into(),
        pipeline,
        r#as,
    })
}

#[cfg(test)]
mod tests {
    use configuration::Configuration;
    use mongodb::bson::{bson, Bson};
    use ndc_test_helpers::{
        binop, collection, exists, field, named_type, object_type, query, query_request,
        relation_field, relationship, row_set, star_count_aggregate, target, value,
    };
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::super::execute_query_request;
    use crate::{
        mongo_query_plan::MongoConfiguration,
        mongodb::test_helpers::mock_collection_aggregate_response_for_pipeline,
        test_helpers::mflix_config,
    };

    #[tokio::test]
    async fn looks_up_an_array_relation() -> Result<(), anyhow::Error> {
        let query_request = query_request()
            .collection("classes")
            .query(query().fields([
                field!("class_title" => "title"),
                relation_field!("students" => "class_students", query().fields([
                    field!("student_name" => "name")
                ])),
            ]))
            .relationships([(
                "class_students",
                relationship("students", [("_id", "classId")]),
            )])
            .into();

        let expected_response = row_set()
            .row([
                ("class_title", json!("MongoDB 101")),
                (
                    "students",
                    json!({ "rows": [
                        { "student_name": "Alice" },
                        { "student_name": "Bob" },
                    ]}),
                ),
            ])
            .into_response();

        let expected_pipeline = bson!([
            {
                "$lookup": {
                    "from": "students",
                    "localField": "_id",
                    "foreignField": "classId",
                    "pipeline": [
                        {
                            "$replaceWith": {
                                "student_name": { "$ifNull": ["$name", null] },
                            },
                        }
                    ],
                    "as": "class_students",
                },
            },
            {
                "$replaceWith": {
                    "class_title": { "$ifNull": ["$title", null] },
                    "students": {
                        "rows": {
                            "$map": {
                                "input": { "$getField": { "$literal": "class_students" } },
                                "in": {
                                    "student_name": "$$this.student_name"
                                }
                            }
                        }
                    },
                },
            },
        ]);

        let db = mock_collection_aggregate_response_for_pipeline(
            "classes",
            expected_pipeline,
            bson!([{
                "class_title": "MongoDB 101",
                "students": { "rows": [
                    { "student_name": "Alice" },
                    { "student_name": "Bob" },
                ] },
            }]),
        );

        let result = execute_query_request(db, &students_config(), query_request).await?;
        assert_eq!(expected_response, result);

        Ok(())
    }

    #[tokio::test]
    async fn looks_up_an_object_relation() -> Result<(), anyhow::Error> {
        let query_request = query_request()
            .collection("students")
            .query(query().fields([
                field!("student_name" => "name"),
                relation_field!("class" => "student_class", query().fields([
                    field!("class_title" => "title")
                ])),
            ]))
            .relationships([(
                "student_class",
                relationship("classes", [("classId", "_id")]),
            )])
            .into();

        let expected_response = row_set()
            .rows([
                [
                    ("student_name", json!("Alice")),
                    (
                        "class",
                        json!({ "rows": [{ "class_title": "MongoDB 101" }] }),
                    ),
                ],
                [
                    ("student_name", json!("Bob")),
                    (
                        "class",
                        json!({ "rows": [{ "class_title": "MongoDB 101" }] }),
                    ),
                ],
            ])
            .into_response();

        let expected_pipeline = bson!([
            {
                "$lookup": {
                    "from": "classes",
                    "localField": "classId",
                    "foreignField": "_id",
                    "pipeline": [
                        {
                            "$replaceWith": {
                                "class_title": { "$ifNull": ["$title", null] },
                            },
                        }
                    ],
                    "as": "student_class",
                },
            },
            {
                "$replaceWith": {
                    "student_name": { "$ifNull": ["$name", null] },
                    "class": {
                        "rows": {
                            "$map": {
                                "input": { "$getField": { "$literal": "student_class" } },
                                "in": {
                                    "class_title": "$$this.class_title"
                                }
                            }
                        }
                    },
                },
            },
        ]);

        let db = mock_collection_aggregate_response_for_pipeline(
            "students",
            expected_pipeline,
            bson!([
                {
                    "student_name": "Alice",
                    "class": { "rows": [{ "class_title": "MongoDB 101" }] },
                },
                {
                    "student_name": "Bob",
                    "class": { "rows": [{ "class_title": "MongoDB 101" }] },
                },
            ]),
        );

        let result = execute_query_request(db, &students_config(), query_request).await?;
        assert_eq!(expected_response, result);

        Ok(())
    }

    #[tokio::test]
    async fn looks_up_a_relation_with_multiple_column_mappings() -> Result<(), anyhow::Error> {
        let query_request = query_request()
            .collection("classes")
            .query(query().fields([
                field!("class_title" => "title"),
                relation_field!("students" => "students", query().fields([
                    field!("student_name" => "name")
                ])),
            ]))
            .relationships([(
                "students",
                relationship("students", [("title", "class_title"), ("year", "year")]),
            )])
            .into();

        let expected_response = row_set()
            .row([
                ("class_title", json!("MongoDB 101")),
                (
                    "students",
                    json!({ "rows": [
                        { "student_name": "Alice" },
                        { "student_name": "Bob" },
                    ]}),
                ),
            ])
            .into_response();

        let expected_pipeline = bson!([
            {
                "$lookup": {
                    "from": "students",
                    "let": {
                        "v_year": "$year",
                        "v_title": "$title",
                    },
                    "pipeline": [
                        {
                            "$match": { "$expr": {
                                "$and": [
                                    { "$eq": ["$$v_title", "$class_title"] },
                                    { "$eq": ["$$v_year", "$year"] },
                                ],
                            } },
                        },
                        {
                            "$replaceWith": {
                                "student_name": { "$ifNull": ["$name", null] },
                            },
                        },
                    ],
                    "as": "students",
                },
            },
            {
                "$replaceWith": {
                    "class_title": { "$ifNull": ["$title", null] },
                    "students": {
                        "rows": {
                            "$map": {
                                "input": { "$getField": { "$literal": "students" } },
                                "in": {
                                    "student_name": "$$this.student_name"
                                }
                            }
                        }
                    },
                },
            },
        ]);

        let db = mock_collection_aggregate_response_for_pipeline(
            "classes",
            expected_pipeline,
            bson!([{
                "class_title": "MongoDB 101",
                "students": { "rows": [
                    { "student_name": "Alice" },
                    { "student_name": "Bob" },
                ] },
            }]),
        );

        let result = execute_query_request(db, &students_config(), query_request).await?;
        assert_eq!(expected_response, result);

        Ok(())
    }

    #[tokio::test]
    async fn makes_recursive_lookups_for_nested_relations() -> Result<(), anyhow::Error> {
        let query_request = query_request()
            .collection("classes")
            .query(query().fields([
                field!("class_title" => "title"),
                relation_field!("students" => "students", query().fields([
                    field!("student_name" => "name"),
                    relation_field!("assignments" => "assignments", query().fields([
                        field!("assignment_title" => "title")
                    ]))
                ])),
            ]))
            .relationships([
                ("students", relationship("students", [("_id", "class_id")])),
                (
                    "assignments",
                    relationship("assignments", [("_id", "student_id")]),
                ),
            ])
            .into();

        let expected_response = row_set()
            .row([
                ("class_title", json!("MongoDB 101")),
                (
                    "students",
                    json!({ "rows": [
                        {
                            "student_name": "Alice",
                            "assignments": { "rows": [
                                { "assignment_title": "read chapter 2" },
                            ]}
                        },
                        {
                            "student_name": "Bob",
                            "assignments": { "rows": [
                                { "assignment_title": "JSON Basics" },
                                { "assignment_title": "read chapter 2" },
                            ]}
                        },
                    ]}),
                ),
            ])
            .into_response();

        let expected_pipeline = bson!([
            {
                "$lookup": {
                    "from": "students",
                    "localField": "_id",
                    "foreignField": "class_id",
                    "pipeline": [
                        {
                            "$lookup": {
                                "from": "assignments",
                                "localField": "_id",
                                "foreignField": "student_id",
                                "pipeline": [
                                    {
                                        "$replaceWith": {
                                            "assignment_title": { "$ifNull": ["$title", null] },
                                        },
                                    },
                                ],
                                "as": "assignments",
                            }
                        },
                        {
                            "$replaceWith": {
                                "assignments": {
                                    "rows": {
                                        "$map": {
                                            "input": { "$getField": { "$literal": "assignments" } },
                                            "in": {
                                                "assignment_title": "$$this.assignment_title"
                                            }
                                        }
                                    }
                                },
                                "student_name": { "$ifNull": ["$name", null] },
                            },
                        },
                    ],
                    "as": "students",
                },
            },
            {
                "$replaceWith": {
                    "class_title": { "$ifNull": ["$title", null] },
                    "students": {
                        "rows": {
                            "$map": {
                                "input": { "$getField": { "$literal": "students" } },
                                "in": {
                                    "assignments": "$$this.assignments",
                                    "student_name": "$$this.student_name",
                                }
                            }
                        }
                    },
                },
            },
        ]);

        let db = mock_collection_aggregate_response_for_pipeline(
            "classes",
            expected_pipeline,
            bson!([{
                "class_title": "MongoDB 101",
                "students": {
                    "rows": [
                        {
                            "student_name": "Alice",
                            "assignments": {
                                "rows": [
                                    { "assignment_title": "read chapter 2" },
                                ],
                            }
                        },
                        {
                            "student_name": "Bob",
                            "assignments": {
                                "rows": [
                                    { "assignment_title": "JSON Basics" },
                                    { "assignment_title": "read chapter 2" },
                                ],
                            }
                        },
                    ]
                },
            }]),
        );

        let result = execute_query_request(db, &students_config(), query_request).await?;
        assert_eq!(expected_response, result);

        Ok(())
    }

    #[tokio::test]
    async fn executes_aggregation_in_relation() -> Result<(), anyhow::Error> {
        let query_request = query_request()
            .collection("classes")
            .query(query().fields([
                relation_field!("students_aggregate" => "students", query().aggregates([
                    star_count_aggregate!("aggregate_count")
                ])),
            ]))
            .relationships([("students", relationship("students", [("_id", "classId")]))])
            .into();

        let expected_response = row_set()
            .row([(
                "students_aggregate",
                json!({
                    "aggregates": {
                        "aggregate_count": { "$numberInt": "2" }
                    }
                }),
            )])
            .into_response();

        let expected_pipeline = bson!([
            {
                "$lookup": {
                    "from": "students",
                    "localField": "_id",
                    "foreignField": "classId",
                    "pipeline": [
                        {
                            "$facet": {
                                "aggregate_count": [
                                    { "$count": "result" },
                                ],
                            }
                        },
                        {
                            "$replaceWith": {
                                "aggregates": {
                                    "aggregate_count": {
                                        "$getField": {
                                            "field": "result",
                                            "input": { "$first": { "$getField": { "$literal": "aggregate_count" } } },
                                        },
                                    },
                                },
                            },
                        }
                    ],
                    "as": "students",
                },
            },
            {
                "$replaceWith": {
                    "students_aggregate": {
                        "$let": {
                            "vars": {
                                "row_set": { "$first": { "$getField": { "$literal": "students" } } }
                            },
                            "in": {
                                "aggregates": {
                                    "aggregate_count": "$$row_set.aggregates.aggregate_count"
                                }
                            }
                        }
                    }
                },
            },
        ]);

        let db = mock_collection_aggregate_response_for_pipeline(
            "classes",
            expected_pipeline,
            bson!([{
                "students_aggregate": {
                    "aggregates": {
                        "aggregate_count": 2,
                    },
                },
            }]),
        );

        let result = execute_query_request(db, &students_config(), query_request).await?;
        assert_eq!(result, expected_response);

        Ok(())
    }

    #[tokio::test]
    async fn filters_by_field_of_related_collection_using_exists() -> Result<(), anyhow::Error> {
        let query_request = query_request()
            .collection("comments")
            .query(
                query()
                    .fields([
                        relation_field!("movie" => "movie", query().fields([
                            field!("title"),
                            field!("year"),
                        ])),
                        field!("name"),
                    ])
                    .limit(50)
                    .predicate(exists(
                        ndc_models::ExistsInCollection::Related {
                            relationship: "movie".into(),
                            arguments: Default::default(),
                        },
                        binop(
                            "_eq",
                            target!("title"),
                            value!("The Land Beyond the Sunset"),
                        ),
                    )),
            )
            .relationships([(
                "movie",
                relationship("movies", [("movie_id", "_id")]).object_type(),
            )])
            .into();

        let expected_response = row_set()
            .row([
                ("name", json!("Mercedes Tyler")),
                (
                    "movie",
                    json!({ "rows": [{
                        "title": "The Land Beyond the Sunset",
                        "year": 1912
                    }]}),
                ),
            ])
            .into_response();

        let expected_pipeline = bson!([
          {
            "$lookup": {
              "from": "movies",
              "localField": "movie_id",
              "foreignField": "_id",
              "pipeline": [
                {
                  "$replaceWith": {
                    "year": { "$ifNull": ["$year", null] },
                    "title": { "$ifNull": ["$title", null] }
                  }
                }
              ],
              "as": "movie"
            }
          },
          {
            "$match": {
              "movie": {
                "$elemMatch": { "title": { "$eq": "The Land Beyond the Sunset" } }
              }
            }
          },
          {
            "$limit": Bson::Int64(50),
          },
          {
            "$replaceWith": {
              "movie": {
                "rows": {
                  "$map": {
                    "input": { "$getField": { "$literal": "movie" } },
                    "in": {
                        "year": "$$this.year",
                        "title": "$$this.title",
                    }
                  }  
                }
              },
              "name": { "$ifNull": ["$name", null] }
            }
          },
        ]);

        let db = mock_collection_aggregate_response_for_pipeline(
            "comments",
            expected_pipeline,
            bson!([{
                "name": "Mercedes Tyler",
                "movie": { "rows": [{
                    "title": "The Land Beyond the Sunset",
                    "year": 1912
                }] },
            }]),
        );

        let result = execute_query_request(db, &mflix_config(), query_request).await?;
        assert_eq!(result, expected_response);

        Ok(())
    }

    // TODO: This test requires updated ndc_models that add `field_path` to
    // [ndc::ComparisonTarget::Column]
    // #[tokio::test]
    // async fn filters_by_field_nested_in_object_in_related_collection() -> Result<(), anyhow::Error>
    // {
    //     let query_request = query_request()
    //         .collection("comments")
    //         .query(
    //             query()
    //                 .fields([relation_field!("movie" => "movie", query().fields([
    //                     field!("credits" => "credits", object!([
    //                         field!("director"),
    //                     ])),
    //                 ]))])
    //                 .limit(50)
    //                 .predicate(exists(
    //                     ndc_models::ExistsInCollection::Related {
    //                         relationship: "movie".into(),
    //                         arguments: Default::default(),
    //                     },
    //                     binop(
    //                         "_eq",
    //                         target!("credits", field_path: ["director"]),
    //                         value!("Martin Scorsese"),
    //                     ),
    //                 )),
    //         )
    //         .relationships([("movie", relationship("movies", [("movie_id", "_id")]))])
    //         .into();
    //
    //     let expected_response = row_set()
    //         .row([
    //             ("name", "Beric Dondarrion"),
    //             (
    //                 "movie",
    //                 json!({ "rows": [{
    //                     "credits": {
    //                         "director": "Martin Scorsese",
    //                     }
    //                 }]}),
    //             ),
    //         ])
    //         .into();
    //
    //     let expected_pipeline = bson!([
    //         {
    //             "$lookup": {
    //                 "from": "movies",
    //                 "localField": "movie_id",
    //                 "foreignField": "_id",
    //                 "pipeline": [
    //                     {
    //                         "$replaceWith": {
    //                             "credits": {
    //                                 "$cond": {
    //                                     "if": "$credits",
    //                                     "then": { "director": { "$ifNull": ["$credits.director", null] } },
    //                                     "else": null,
    //                                 }
    //                             },
    //                         }
    //                     }
    //                 ],
    //                 "as": "movie"
    //             }
    //         },
    //         {
    //             "$match": {
    //                 "movie.credits.director": {
    //                     "$eq": "Martin Scorsese"
    //                 }
    //             }
    //         },
    //         {
    //             "$limit": Bson::Int64(50),
    //         },
    //         {
    //             "$replaceWith": {
    //                 "name": { "$ifNull": ["$name", null] },
    //                 "movie": {
    //                     "rows": {
    //                         "$getField": {
    //                             "$literal": "movie"
    //                         }
    //                     }
    //                 },
    //             }
    //         },
    //     ]);
    //
    //     let db = mock_collection_aggregate_response_for_pipeline(
    //         "comments",
    //         expected_pipeline,
    //         bson!([{
    //             "name": "Beric Dondarrion",
    //             "movie": { "rows": [{
    //                 "credits": {
    //                     "director": "Martin Scorsese"
    //                 }
    //             }] },
    //         }]),
    //     );
    //
    //     let result = execute_query_request(db, &mflix_config(), query_request).await?;
    //     assert_eq!(expected_response, result);
    //
    //     Ok(())
    // }

    fn students_config() -> MongoConfiguration {
        MongoConfiguration(Configuration {
            collections: [
                collection("assignments"),
                collection("classes"),
                collection("students"),
            ]
            .into(),
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
}
