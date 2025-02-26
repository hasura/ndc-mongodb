use std::collections::BTreeMap;

use itertools::Itertools as _;
use mongodb::bson::{doc, Document};
use mongodb_support::aggregate::{Pipeline, Stage};
use ndc_query_plan::Scope;
use nonempty::NonEmpty;

use crate::mongo_query_plan::{MongoConfiguration, Query, QueryPlan};
use crate::query::column_ref::name_from_scope;
use crate::{interface_types::MongoAgentError, mongodb::sanitize::variable};

use super::column_ref::ColumnRef;
use super::pipeline::pipeline_for_non_foreach;
use super::query_level::QueryLevel;

type Result<T> = std::result::Result<T, MongoAgentError>;

/// Defines any necessary $lookup stages for the given section of the pipeline. This is called for
/// each sub-query in the plan.
pub fn pipeline_for_relations(
    config: &MongoConfiguration,
    query_plan: &QueryPlan,
) -> Result<Pipeline> {
    let QueryPlan { query, .. } = query_plan;
    let Query {
        relationships,
        scope,
        ..
    } = query;

    // Lookup stages perform the join for each relationship, and assign the list of rows or mapping
    // of aggregate results to a field in the parent document.
    let lookup_stages = relationships
        .iter()
        .map(|(name, relationship)| {
            // Recursively build pipeline according to relation query
            let lookup_pipeline = pipeline_for_non_foreach(
                config,
                &QueryPlan {
                    query: relationship.query.clone(),
                    collection: relationship.target_collection.clone(),
                    ..query_plan.clone()
                },
                QueryLevel::Relationship,
            )?;

            Ok(make_lookup_stage(
                relationship.target_collection.clone(),
                &relationship.column_mapping,
                name.to_owned(),
                lookup_pipeline,
                scope.as_ref(),
            )) as Result<_>
        })
        .try_collect()?;

    Ok(lookup_stages)
}

fn make_lookup_stage(
    from: ndc_models::CollectionName,
    column_mapping: &BTreeMap<ndc_models::FieldName, NonEmpty<ndc_models::FieldName>>,
    r#as: ndc_models::RelationshipName,
    lookup_pipeline: Pipeline,
    scope: Option<&Scope>,
) -> Stage {
    // If there is a single column mapping, and the source and target field references can be
    // expressed as match keys (we don't need to escape field names), then we can use a concise
    // correlated subquery. Otherwise we need to fall back to an uncorrelated subquery.
    let single_mapping = if column_mapping.len() == 1 {
        column_mapping.iter().next()
    } else {
        None
    };
    let source_selector = single_mapping.map(|(field_name, _)| field_name);
    let target_selector = single_mapping.map(|(_, target_path)| target_path);

    let source_key =
        source_selector.and_then(|f| ColumnRef::from_field(f.as_ref()).into_match_key());
    let target_key =
        target_selector.and_then(|path| ColumnRef::from_field_path(path.as_ref()).into_match_key());

    match (source_key, target_key) {
        (Some(source_key), Some(target_key)) => lookup_with_concise_correlated_subquery(
            from,
            source_key.into_owned(),
            target_key.into_owned(),
            r#as,
            lookup_pipeline,
            scope,
        ),

        _ => lookup_with_uncorrelated_subquery(from, column_mapping, r#as, lookup_pipeline, scope),
    }
}

fn lookup_with_concise_correlated_subquery(
    from: ndc_models::CollectionName,
    source_selector_key: String,
    target_selector_key: String,
    r#as: ndc_models::RelationshipName,
    lookup_pipeline: Pipeline,
    scope: Option<&Scope>,
) -> Stage {
    Stage::Lookup {
        from: Some(from.to_string()),
        local_field: Some(source_selector_key),
        foreign_field: Some(target_selector_key),
        r#let: scope.map(|scope| {
            doc! {
                name_from_scope(scope): "$$ROOT"
            }
        }),
        pipeline: if lookup_pipeline.is_empty() {
            None
        } else {
            Some(lookup_pipeline)
        },
        r#as: r#as.to_string(),
    }
}

/// The concise correlated subquery syntax with `localField` and `foreignField` only works when
/// joining on one field. To join on multiple fields it is necessary to bind variables to fields on
/// the left side of the join, and to emit a custom `$match` stage to filter the right side of the
/// join. This version also allows comparing arbitrary expressions for the join which we need for
/// cases like joining on field names that require escaping.
fn lookup_with_uncorrelated_subquery(
    from: ndc_models::CollectionName,
    column_mapping: &BTreeMap<ndc_models::FieldName, NonEmpty<ndc_models::FieldName>>,
    r#as: ndc_models::RelationshipName,
    lookup_pipeline: Pipeline,
    scope: Option<&Scope>,
) -> Stage {
    let mut let_bindings: Document = column_mapping
        .keys()
        .map(|local_field| {
            (
                variable(local_field.as_str()),
                ColumnRef::from_field(local_field.as_ref())
                    .into_aggregate_expression()
                    .into_bson(),
            )
        })
        .collect();

    if let Some(scope) = scope {
        let_bindings.insert(name_from_scope(scope), "$$ROOT");
    }

    // Creating an intermediate Vec and sorting it is done just to help with testing.
    // A stable order for matchers makes it easier to assert equality between actual
    // and expected pipelines.
    let mut column_pairs: Vec<(&ndc_models::FieldName, &NonEmpty<ndc_models::FieldName>)> =
        column_mapping.iter().collect();
    column_pairs.sort();

    let matchers: Vec<Document> = column_pairs
        .into_iter()
        .map(|(local_field, remote_field_path)| {
            doc! { "$eq": [
                ColumnRef::variable(variable(local_field.as_str())).into_aggregate_expression(),
                ColumnRef::from_field_path(remote_field_path.as_ref()).into_aggregate_expression(),
            ] }
        })
        .collect();

    let mut pipeline = Pipeline::from_iter([Stage::Match(if matchers.len() == 1 {
        doc! { "$expr": matchers.into_iter().next().unwrap() }
    } else {
        doc! { "$expr": { "$and": matchers } }
    })]);
    pipeline.append(lookup_pipeline);
    let pipeline: Option<Pipeline> = pipeline.into();

    Stage::Lookup {
        from: Some(from.to_string()),
        local_field: None,
        foreign_field: None,
        r#let: let_bindings.into(),
        pipeline,
        r#as: r#as.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use configuration::Configuration;
    use mongodb::bson::{bson, Bson};
    use ndc_models::{FieldName, QueryResponse};
    use ndc_test_helpers::{
        binop, collection, exists, field, named_type, object, object_type, query, query_request,
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
                relationship("students", [("_id", &["classId"])]),
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
                    "let": {
                        "scope_root": "$$ROOT",
                    },
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
                                "input": "$class_students",
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
                relationship("classes", [("classId", &["_id"])]),
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
                    "let": {
                        "scope_root": "$$ROOT",
                    },
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
                                "input": "$student_class",
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
                relationship(
                    "students",
                    [("title", &["class_title"]), ("year", &["year"])],
                ),
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
                        "year": "$year",
                        "title": "$title",
                        "scope_root": "$$ROOT",
                    },
                    "pipeline": [
                        {
                            "$match": { "$expr": {
                                "$and": [
                                    { "$eq": ["$$title", "$class_title"] },
                                    { "$eq": ["$$year", "$year"] },
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
                                "input": "$students",
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
    async fn escapes_column_mappings_names_if_necessary() -> Result<(), anyhow::Error> {
        let query_request = query_request()
            .collection("weird_field_names")
            .query(query().fields([
                field!("invalid_name" => "$invalid.name"),
                relation_field!("join" => "join", query().fields([
                  field!("invalid_name" => "$invalid.name")
                ])),
            ]))
            .relationships([(
                "join",
                relationship("weird_field_names", [("$invalid.name", &["$invalid.name"])]),
            )])
            .into();

        let expected_pipeline = bson!([
            {
                "$lookup": {
                    "from": "weird_field_names",
                    "let": {
                        "v_·24invalid·2ename": { "$getField": { "$literal": "$invalid.name" } },
                        "scope_root": "$$ROOT",
                    },
                    "pipeline": [
                        {
                            "$match": { "$expr": {
                                "$eq": [
                                    "$$v_·24invalid·2ename",
                                    { "$getField": { "$literal": "$invalid.name" } }
                                ]
                            } },
                        },
                        {
                            "$replaceWith": {
                                "invalid_name": { "$ifNull": [{ "$getField": { "$literal": "$invalid.name" } }, null] },
                            },
                        },
                    ],
                    "as": "join",
                },
            },
            {
                "$replaceWith": {
                    "invalid_name": { "$ifNull": [{ "$getField": { "$literal": "$invalid.name" } }, null] },
                    "join": {
                        "rows": {
                            "$map": {
                                "input": "$join",
                                "in": {
                                    "invalid_name": "$$this.invalid_name",
                                }
                            }
                        }
                    },
                },
            },
        ]);

        let db = mock_collection_aggregate_response_for_pipeline(
            "weird_field_names",
            expected_pipeline,
            bson!([]),
        );

        execute_query_request(db, &test_cases_config(), query_request).await?;
        // assert_eq!(expected_response, result);

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
                (
                    "students",
                    relationship("students", [("_id", &["class_id"])]),
                ),
                (
                    "assignments",
                    relationship("assignments", [("_id", &["student_id"])]),
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
                    "let": {
                        "scope_root": "$$ROOT",
                    },
                    "pipeline": [
                        {
                            "$lookup": {
                                "from": "assignments",
                                "localField": "_id",
                                "foreignField": "student_id",
                                "let": {
                                    "scope_0": "$$ROOT",
                                },
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
                                "assignments": "$assignments",
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
                                "input": "$students",
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
        assert_eq!(result, expected_response);

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
            .relationships([(
                "students",
                relationship("students", [("_id", &["classId"])]),
            )])
            .into();

        let expected_response = row_set()
            .row([(
                "students_aggregate",
                json!({
                    "aggregates": {
                        "aggregate_count": 2
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
                    "let": {
                        "scope_root": "$$ROOT",
                    },
                    "pipeline": [
                        {
                            "$group": {
                                "_id": null,
                                "aggregate_count": { "$sum": 1 },
                            }
                        },
                        {
                            "$replaceWith": {
                                "aggregate_count": { "$ifNull": ["$aggregate_count", 0] },
                            },
                        }
                    ],
                    "as": "students",
                },
            },
            {
                "$replaceWith": {
                    "students_aggregate": {
                        "aggregates": {
                            "$let": {
                                "vars": {
                                    "aggregates": { "$first": "$students" }
                                },
                                "in": {
                                    "aggregate_count": { "$ifNull": ["$$aggregates.aggregate_count", 0] }
                                }
                            }
                        },
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
                            field_path: Default::default(),
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
                relationship("movies", [("movie_id", &["_id"])]).object_type(),
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
              "let": {
                "scope_root": "$$ROOT",
              },
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
            "$limit": Bson::Int32(50),
          },
          {
            "$replaceWith": {
              "movie": {
                "rows": {
                  "$map": {
                    "input": "$movie",
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

    #[tokio::test]
    async fn filters_by_field_nested_in_object_in_related_collection() -> Result<(), anyhow::Error>
    {
        let query_request = query_request()
            .collection("comments")
            .query(
                query()
                    .fields([
                        field!("name"),
                        relation_field!("movie" => "movie", query().fields([
                            field!("credits" => "credits", object!([
                                field!("director"),
                            ])),
                        ])),
                    ])
                    .limit(50)
                    .predicate(exists(
                        ndc_models::ExistsInCollection::Related {
                            relationship: "movie".into(),
                            arguments: Default::default(),
                            field_path: Default::default(),
                        },
                        binop(
                            "_eq",
                            target!("credits", field_path: [Some(FieldName::from("director"))]),
                            value!("Martin Scorsese"),
                        ),
                    )),
            )
            .relationships([("movie", relationship("movies", [("movie_id", &["_id"])]))])
            .into();

        let expected_response: QueryResponse = row_set()
            .row([
                ("name", json!("Beric Dondarrion")),
                (
                    "movie",
                    json!({ "rows": [{
                        "credits": {
                            "director": "Martin Scorsese",
                        }
                    }]}),
                ),
            ])
            .into();

        let expected_pipeline = bson!([
            {
                "$lookup": {
                    "from": "movies",
                    "localField": "movie_id",
                    "foreignField": "_id",
                    "let": {
                        "scope_root": "$$ROOT",
                    },
                    "pipeline": [
                        {
                            "$replaceWith": {
                                "credits": {
                                    "$cond": {
                                        "if": "$credits",
                                        "then": { "director": { "$ifNull": ["$credits.director", null] } },
                                        "else": null,
                                    }
                                },
                            }
                        }
                    ],
                    "as": "movie"
                }
            },
            {
                "$match": {
                    "movie": {
                        "$elemMatch": {
                            "credits.director": {
                                "$eq": "Martin Scorsese"
                            }
                        }
                    }
                }
            },
            {
                "$limit": Bson::Int32(50),
            },
            {
                "$replaceWith": {
                    "name": { "$ifNull": ["$name", null] },
                    "movie": {
                        "rows": {
                            "$map": {
                                "input": "$movie",
                                "in": {
                                    "credits": "$$this.credits",
                                }
                            }
                        }
                    },
                }
            },
        ]);

        let db = mock_collection_aggregate_response_for_pipeline(
            "comments",
            expected_pipeline,
            bson!([{
                "name": "Beric Dondarrion",
                "movie": { "rows": [{
                    "credits": {
                        "director": "Martin Scorsese"
                    }
                }] },
            }]),
        );

        let result = execute_query_request(db, &mflix_config(), query_request).await?;
        assert_eq!(expected_response, result);

        Ok(())
    }

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

    fn test_cases_config() -> MongoConfiguration {
        MongoConfiguration(Configuration {
            collections: [collection("weird_field_names")].into(),
            object_types: [(
                "weird_field_names".into(),
                object_type([
                    ("_id", named_type("ObjectId")),
                    ("$invalid.name", named_type("Int")),
                ]),
            )]
            .into(),
            functions: Default::default(),
            procedures: Default::default(),
            native_mutations: Default::default(),
            native_queries: Default::default(),
            options: Default::default(),
        })
    }
}
