use std::collections::HashMap;

use anyhow::anyhow;
use configuration::Configuration;
use dc_api_types::comparison_column::ColumnSelector;
use dc_api_types::relationship::ColumnMapping;
use dc_api_types::{Field, QueryRequest, Relationship, VariableSet};
use mongodb::bson::{doc, Bson, Document};

use crate::mongodb::sanitize::safe_column_selector;
use crate::mongodb::Pipeline;
use crate::{
    interface_types::MongoAgentError,
    mongodb::{sanitize::variable, Stage},
};

use super::pipeline::pipeline_for_non_foreach;

pub fn pipeline_for_relations(
    config: &Configuration,
    variables: Option<&VariableSet>,
    query_request: &QueryRequest,
) -> Result<Pipeline, MongoAgentError> {
    let QueryRequest {
        target,
        relationships,
        query,
        ..
    } = query_request;

    let empty_field_map = HashMap::new();
    let fields = if let Some(Some(fs)) = &query.fields {
        fs
    } else {
        &empty_field_map
    };

    let empty_relation_map = HashMap::new();
    let relationships = &relationships
        .iter()
        .find_map(|rels| {
            if &rels.source_table == target.name() {
                Some(&rels.relationships)
            } else {
                None
            }
        })
        .unwrap_or(&empty_relation_map);

    let stages = lookups_for_fields(config, query_request, variables, relationships, &[], fields)?;
    Ok(Pipeline::new(stages))
}

/// Produces $lookup stages for any necessary joins
fn lookups_for_fields(
    config: &Configuration,
    query_request: &QueryRequest,
    variables: Option<&VariableSet>,
    relationships: &HashMap<String, Relationship>,
    parent_columns: &[&str],
    fields: &HashMap<String, Field>,
) -> Result<Vec<Stage>, MongoAgentError> {
    let stages = fields
        .iter()
        .map(|(field_name, field)| {
            lookups_for_field(
                config,
                query_request,
                variables,
                relationships,
                parent_columns,
                field_name,
                field,
            )
        })
        .collect::<Result<Vec<Vec<_>>, MongoAgentError>>()?
        .into_iter()
        .flatten()
        .collect();
    Ok(stages)
}

/// Produces $lookup stages for any necessary joins
fn lookups_for_field(
    config: &Configuration,
    query_request: &QueryRequest,
    variables: Option<&VariableSet>,
    relationships: &HashMap<String, Relationship>,
    parent_columns: &[&str],
    field_name: &str,
    field: &Field,
) -> Result<Vec<Stage>, MongoAgentError> {
    match field {
        Field::Column { .. } => Ok(vec![]),
        Field::NestedObject { column, query } => {
            let nested_parent_columns = append_to_path(parent_columns, column);
            let fields = query.fields.clone().flatten().unwrap_or_default();
            lookups_for_fields(
                config,
                query_request,
                variables,
                relationships,
                &nested_parent_columns,
                &fields,
            )
            .map(Into::into)
        }
        Field::NestedArray {
            field,
            // NOTE: We can use a $slice in our selection to do offsets and limits:
            // https://www.mongodb.com/docs/manual/reference/operator/projection/slice/#mongodb-projection-proj.-slice
            limit: _,
            offset: _,
            r#where: _,
        } => lookups_for_field(
            config,
            query_request,
            variables,
            relationships,
            parent_columns,
            field_name,
            field,
        ),
        Field::Relationship {
            query,
            relationship: relationship_name,
        } => {
            let r#as = match parent_columns {
                [] => field_name.to_owned(),
                _ => format!("{}.{}", parent_columns.join("."), field_name),
            };

            let Relationship {
                column_mapping,
                target,
                ..
            } = get_relationship(relationships, relationship_name)?;
            let from = collection_reference(target.name())?;

            // Recursively build pipeline according to relation query
            let (lookup_pipeline, _) = pipeline_for_non_foreach(
                config,
                variables,
                &QueryRequest {
                    query: query.clone(),
                    target: target.clone(),
                    ..query_request.clone()
                },
            )?;

            let lookup = make_lookup_stage(from, column_mapping, r#as, lookup_pipeline)?;

            Ok(vec![lookup])
        }
    }
}

fn make_lookup_stage(
    from: String,
    column_mapping: &ColumnMapping,
    r#as: String,
    lookup_pipeline: Pipeline,
) -> Result<Stage, MongoAgentError> {
    let let_bindings: Document = column_mapping
        .0
        .keys()
        .map(|local_field| {
            Ok((
                variable(&local_field.as_var())?,
                Bson::String(format!("${}", safe_column_selector(local_field)?)),
            ))
        })
        .collect::<Result<_, MongoAgentError>>()?;

    // Creating an intermediate Vec and sorting it is done just to help with testing.
    // A stable order for matchers makes it easier to assert equality between actual
    // and expected pipelines.
    let mut column_pairs: Vec<(&ColumnSelector, &ColumnSelector)> =
        column_mapping.0.iter().collect();
    column_pairs.sort();

    let matchers: Vec<Document> = column_pairs
        .into_iter()
        .map(|(local_field, remote_field)| {
            Ok(doc! { "$eq": [
                format!("$${}", variable(&local_field.as_var())?),
                format!("${}", safe_column_selector(remote_field)?)
            ] })
        })
        .collect::<Result<_, MongoAgentError>>()?;

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

/// Transform an Agent IR qualified table reference into a MongoDB collection reference.
fn collection_reference(table_ref: &[String]) -> Result<String, MongoAgentError> {
    if table_ref.len() == 1 {
        Ok(table_ref[0].clone())
    } else {
        Err(MongoAgentError::BadQuery(anyhow!(
            "expected \"from\" field of relationship to contain one element"
        )))
    }
}

fn get_relationship<'a>(
    relationships: &'a HashMap<String, Relationship>,
    relationship_name: &str,
) -> Result<&'a Relationship, MongoAgentError> {
    match relationships.get(relationship_name) {
        Some(relationship) => Ok(relationship),
        None => Err(MongoAgentError::UnspecifiedRelation(
            relationship_name.to_owned(),
        )),
    }
}

fn append_to_path<'a, 'b, 'c>(parent_columns: &'a [&'b str], column: &'c str) -> Vec<&'c str>
where
    'b: 'c,
{
    parent_columns.iter().copied().chain(Some(column)).collect()
}

#[cfg(test)]
mod tests {
    use dc_api_types::{QueryRequest, QueryResponse};
    use mongodb::bson::{bson, Bson};
    use pretty_assertions::assert_eq;
    use serde_json::{from_value, json};

    use super::super::execute_query_request;
    use crate::mongodb::test_helpers::mock_collection_aggregate_response_for_pipeline;

    #[tokio::test]
    async fn looks_up_an_array_relation() -> Result<(), anyhow::Error> {
        let query_request: QueryRequest = from_value(json!({
            "query": {
                "fields": {
                    "class_title": { "type": "column", "column": "title", "column_type": "string" },
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
                        "target": { "name": ["students"], "type": "table"},
                    },
                },
            }],
        }))?;

        let expected_response: QueryResponse = from_value(json!({
            "rows": [
                {
                    "class_title": "MongoDB 101",
                    "students": [
                        { "student_name": "Alice" },
                        { "student_name": "Bob" },
                    ],
                },
            ],
        }))?;

        let expected_pipeline = bson!([
            {
                "$lookup": {
                    "from": "students",
                    "let": {
                        "v__id": "$_id"
                    },
                    "pipeline": [
                        {
                            "$match": { "$expr": {
                                "$eq": ["$$v__id", "$classId"]
                            } }
                        },
                        {
                            "$replaceWith": {
                                "student_name": { "$ifNull": ["$name", null] },
                            },
                        }
                    ],
                    "as": "students",
                },
            },
            {
                "$replaceWith": {
                    "class_title": { "$ifNull": ["$title", null] },
                    "students": {
                        "rows": {
                            "$getField": { "$literal": "students" },
                        },
                    },
                },
            },
        ]);

        let db = mock_collection_aggregate_response_for_pipeline(
            "classes",
            expected_pipeline,
            bson!([{
                "class_title": "MongoDB 101",
                "students": [
                    { "student_name": "Alice" },
                    { "student_name": "Bob" },
                ],
            }]),
        );

        let result = execute_query_request(db, &Default::default(), query_request).await?;
        assert_eq!(expected_response, result);

        Ok(())
    }

    #[tokio::test]
    async fn looks_up_an_object_relation() -> Result<(), anyhow::Error> {
        let query_request: QueryRequest = from_value(json!({
            "query": {
                "fields": {
                    "student_name": { "type": "column", "column": "name", "column_type": "string" },
                    "class": {
                        "type": "relationship",
                        "query": {
                            "fields": {
                                "class_title": { "type": "column", "column": "title", "column_type": "string" },
                            },
                        },
                        "relationship": "student_class",
                    },
                },
            },
            "target": {"name": ["students"], "type": "table"},
            "relationships": [{
                "source_table": ["students"],
                "relationships": {
                    "student_class": {
                        "column_mapping": { "classId": "_id" },
                        "relationship_type": "object",
                        "target": {"name": ["classes"], "type": "table"},
                    },
                },
            }],
        }))?;

        let expected_response: QueryResponse = from_value(json!({
            "rows": [
                {
                    "student_name": "Alice",
                    "class": { "class_title": "MongoDB 101" },
                },
                {
                    "student_name": "Bob",
                    "class": { "class_title": "MongoDB 101" },
                },
            ],
        }))?;

        let expected_pipeline = bson!([
            {
                "$lookup": {
                    "from": "classes",
                    "let": {
                        "v_classId": "$classId"
                    },
                    "pipeline": [
                        {
                            "$match": { "$expr": {
                                "$eq": ["$$v_classId", "$_id"]
                            } }
                        },
                        {
                            "$replaceWith": {
                                "class_title": { "$ifNull": ["$title", null] },
                            },
                        }
                    ],
                    "as": "class",
                },
            },
            {
                "$replaceWith": {
                    "student_name": { "$ifNull": ["$name", null] },
                    "class": { "rows": {
                        "$getField": { "$literal": "class" } }
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
                    "class": { "class_title": "MongoDB 101" },
                },
                {
                    "student_name": "Bob",
                    "class": { "class_title": "MongoDB 101" },
                },
            ]),
        );

        let result = execute_query_request(db, &Default::default(), query_request).await?;
        assert_eq!(expected_response, result);

        Ok(())
    }

    #[tokio::test]
    async fn looks_up_a_relation_with_multiple_column_mappings() -> Result<(), anyhow::Error> {
        let query_request: QueryRequest = from_value(json!({
            "query": {
                "fields": {
                    "class_title": { "type": "column", "column": "title", "column_type": "string" },
                    "students": {
                        "type": "relationship",
                        "query": {
                            "fields": {
                                "student_name": { "type": "column", "column": "name", "column_type": "string" },
                            },
                        },
                        "relationship": "students",
                    },
                },
            },
            "target": {"name": ["classes"], "type": "table"},
            "relationships": [{
                "source_table": ["classes"],
                "relationships": {
                    "students": {
                        "column_mapping": { "title": "class_title", "year": "year" },
                        "relationship_type": "array",
                        "target": {"name": ["students"], "type": "table"},
                    },
                },
            }],
        }))?;

        let expected_response: QueryResponse = from_value(json!({
            "rows": [
                {
                    "class_title": "MongoDB 101",
                    "students": [
                        { "student_name": "Alice" },
                        { "student_name": "Bob" },
                    ],
                },
            ],
        }))?;

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
                        "rows": { "$getField": { "$literal": "students" } },
                    },
                },
            },
        ]);

        let db = mock_collection_aggregate_response_for_pipeline(
            "classes",
            expected_pipeline,
            bson!([{
                "class_title": "MongoDB 101",
                "students": [
                { "student_name": "Alice" },
                { "student_name": "Bob" },
            ],
            }]),
        );

        let result = execute_query_request(db, &Default::default(), query_request).await?;
        assert_eq!(expected_response, result);

        Ok(())
    }

    #[tokio::test]
    async fn makes_recursive_lookups_for_nested_relations() -> Result<(), anyhow::Error> {
        let query_request: QueryRequest = from_value(json!({
            "query": {
                "fields": {
                    "class_title": { "type": "column", "column": "title", "column_type": "string" },
                    "students": {
                        "type": "relationship",
                        "relationship": "students",
                        "query": {
                            "fields": {
                                "student_name": { "type": "column", "column": "name", "column_type": "string" },
                                "assignments": {
                                    "type": "relationship",
                                    "relationship": "assignments",
                                    "query": {
                                        "fields": {
                                            "assignment_title": { "type": "column", "column": "title", "column_type": "string" },
                                        },
                                    },
                                },
                            },
                        },
                        "relationship": "students",
                    },
                },
            },
            "target": {"name": ["classes"], "type": "table"},
            "relationships": [
                {
                    "source_table": ["classes"],
                    "relationships": {
                        "students": {
                            "column_mapping": { "_id": "class_id" },
                            "relationship_type": "array",
                            "target": {"name": ["students"], "type": "table"},
                        },
                    },
                },
                {
                    "source_table": ["students"],
                    "relationships": {
                        "assignments": {
                            "column_mapping": { "_id": "student_id" },
                            "relationship_type": "array",
                            "target": {"name": ["assignments"], "type": "table"},
                        },
                    },
                }
            ],
        }))?;

        let expected_response: QueryResponse = from_value(json!({
            "rows": [
                {
                    "class_title": "MongoDB 101",
                    "students": { "rows": [
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
                     ]},
                },
            ],
        }))?;

        let expected_pipeline = bson!([
            {
                "$lookup": {
                    "from": "students",
                    "let": {
                        "v__id": "$_id"
                    },
                    "pipeline": [
                        {
                            "$match": {
                                "$expr": {
                                    "$eq": ["$$v__id", "$class_id"]
                                }
                            }
                        },
                        {
                            "$lookup": {
                                "from": "assignments",
                                "let": {
                                    "v__id": "$_id"
                                },
                                "pipeline": [
                                    {
                                        "$match": {
                                            "$expr": {
                                                "$eq": ["$$v__id", "$student_id"]
                                            }
                                        }
                                    },
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
                                    "rows": { "$getField": { "$literal": "assignments" } },
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
                        "rows": { "$getField": { "$literal": "students" } },
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

        let result = execute_query_request(db, &Default::default(), query_request).await?;
        assert_eq!(expected_response, result);

        Ok(())
    }

    #[tokio::test]
    async fn executes_aggregation_in_relation() -> Result<(), anyhow::Error> {
        let query_request: QueryRequest = from_value(json!({
            "query": {
                "fields": {
                    "students_aggregate": {
                        "type": "relationship",
                        "query": {
                            "aggregates": {
                                "aggregate_count": { "type": "star_count" },
                            },
                        },
                        "relationship": "students",
                    },
                },
            },
            "table": ["classes"],
            "table_relationships": [{
                "source_table": ["classes"],
                "relationships": {
                    "students": {
                        "column_mapping": { "_id": "classId" },
                        "relationship_type": "array",
                        "target_table": ["students"],
                    },
                },
            }],
        }))?;

        let expected_response: QueryResponse = from_value(json!({
            "rows": [
                {
                    "students_aggregate": {
                        "aggregates": {
                            "aggregate_count": 2,
                        },
                     },
                },
            ],
        }))?;

        let expected_pipeline = bson!([
            {
                "$lookup": {
                    "from": "students",
                    "let": {
                        "v__id": "$_id"
                    },
                    "pipeline": [
                        {
                            "$match": { "$expr": {
                                "$eq": ["$$v__id", "$classId"]
                            } }
                        },
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
                    "as": "students_aggregate",
                },
            },
            {
                "$replaceWith": {
                    "students_aggregate": { "$first": {
                        "$getField": { "$literal": "students_aggregate" }
                    } }
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

        let result = execute_query_request(db, &Default::default(), query_request).await?;
        assert_eq!(expected_response, result);

        Ok(())
    }

    #[tokio::test]
    async fn filters_by_field_of_related_collection() -> Result<(), anyhow::Error> {
        let query_request: QueryRequest = from_value(json!({
          "query": {
            "fields": {
              "movie": {
                "type": "relationship",
                "query": {
                  "fields": {
                    "title": { "type": "column", "column": "title", "column_type": "string" },
                    "year": { "type": "column", "column": "year", "column_type": "int" }
                  }
                },
                "relationship": "movie"
              },
              "name": {
                "type": "column",
                "column": "name",
                "column_type": "string"
              }
            },
            "limit": 50,
            "where": {
              "type": "exists",
              "in_table": { "type": "related", "relationship": "movie" },
              "where": {
                "type": "binary_op",
                "column": { "column_type": "string", "name": "title" },
                "operator": "equal",
                "value": { "type": "scalar", "value": "The Land Beyond the Sunset", "value_type": "string" }
              }
            }
          },
          "target": {
            "type": "table",
            "name": [
              "comments"
            ]
          },
          "relationships": [
            {
              "relationships": {
                "movie": {
                  "column_mapping": {
                    "movie_id": "_id"
                  },
                  "relationship_type": "object",
                  "target": { "type": "table", "name": [ "movies" ] }
                }
              },
              "source_table": [
                "comments"
              ]
            }
          ]
        }))?;

        let expected_response: QueryResponse = from_value(json!({
          "rows": [{
            "name": "Mercedes Tyler",
            "movie": { "rows": [{
              "title": "The Land Beyond the Sunset",
              "year": 1912
            }] },
          }]
        }))?;

        let expected_pipeline = bson!([
          {
            "$lookup": {
              "from": "movies",
              "let": {
                "v_movie_id": "$movie_id"
              },
              "pipeline": [
                {
                  "$match": { "$expr": {
                    "$eq": ["$$v_movie_id", "$_id"]
                  } }
                },
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
              "movie.title": {
                "$eq": "The Land Beyond the Sunset"
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
                  "$getField": {
                    "$literal": "movie"
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

        let result = execute_query_request(db, &Default::default(), query_request).await?;
        assert_eq!(expected_response, result);

        Ok(())
    }

    #[tokio::test]
    async fn filters_by_field_nested_in_object_in_related_collection() -> Result<(), anyhow::Error>
    {
        let query_request: QueryRequest = from_value(json!({
          "query": {
            "fields": {
              "movie": {
                "type": "relationship",
                "query": {
                  "fields": {
                    "credits": { "type": "object", "column": "credits", "query": {
                        "fields": {
                            "director": { "type": "column", "column": "director", "column_type": "string" },
                        }
                    } },
                  }
                },
                "relationship": "movie"
              },
              "name": {
                "type": "column",
                "column": "name",
                "column_type": "string"
              }
            },
            "limit": 50,
            "where": {
              "type": "exists",
              "in_table": { "type": "related", "relationship": "movie" },
              "where": {
                "type": "binary_op",
                "column": { "column_type": "string", "name": ["credits", "director"] },
                "operator": "equal",
                "value": { "type": "scalar", "value": "Martin Scorsese", "value_type": "string" }
              }
            }
          },
          "target": {
            "type": "table",
            "name": [
              "comments"
            ]
          },
          "relationships": [
            {
              "relationships": {
                "movie": {
                  "column_mapping": {
                    "movie_id": "_id"
                  },
                  "relationship_type": "object",
                  "target": { "type": "table", "name": [ "movies" ] }
                }
              },
              "source_table": [
                "comments"
              ]
            }
          ]
        }))?;

        let expected_response: QueryResponse = from_value(json!({
            "rows": [{
                "name": "Beric Dondarrion",
                "movie": { "rows": [{
                    "credits": {
                        "director": "Martin Scorsese",
                    }
                }] },
            }]
        }))?;

        let expected_pipeline = bson!([
            {
                "$lookup": {
                    "from": "movies",
                    "let": {
                        "v_movie_id": "$movie_id",
                    },
                    "pipeline": [
                        {
                            "$match": { "$expr": {
                                "$eq": ["$$v_movie_id", "$_id"]
                            } }
                        },
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
                    "movie.credits.director": {
                        "$eq": "Martin Scorsese"
                    }
                }
            },
            {
                "$limit": Bson::Int64(50),
            },
            {
                "$replaceWith": {
                    "name": { "$ifNull": ["$name", null] },
                    "movie": {
                        "rows": {
                            "$getField": {
                                "$literal": "movie"
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

        let result = execute_query_request(db, &Default::default(), query_request).await?;
        assert_eq!(expected_response, result);

        Ok(())
    }
}
