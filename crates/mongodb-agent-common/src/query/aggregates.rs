use std::collections::BTreeMap;

use configuration::MongoScalarType;
use mongodb::bson::{self, doc, Bson};
use mongodb_support::{
    aggregate::{Accumulator, Pipeline, Selection, Stage},
    BsonScalarType,
};
use ndc_models::FieldName;

use crate::{
    aggregation_function::AggregationFunction,
    comparison_function::ComparisonFunction,
    interface_types::MongoAgentError,
    mongo_query_plan::{
        Aggregate, ComparisonTarget, ComparisonValue, Expression, Query, QueryPlan, Type,
    },
    mongodb::sanitize::get_field,
};

use super::{
    column_ref::ColumnRef,
    constants::{RESULT_FIELD, ROWS_FIELD},
    make_selector,
    pipeline::pipeline_for_fields_facet,
    query_level::QueryLevel,
};

type Result<T> = std::result::Result<T, MongoAgentError>;

/// Returns a map of pipelines for evaluating each aggregate independently, paired with
/// a `Selection` that converts results of each pipeline to a format compatible with
/// `QueryResponse`.
pub fn facet_pipelines_for_query(
    query_plan: &QueryPlan,
    query_level: QueryLevel,
) -> Result<(BTreeMap<String, Pipeline>, Selection)> {
    let query = &query_plan.query;
    let Query {
        aggregates,
        aggregates_limit,
        fields,
        ..
    } = query;
    let mut facet_pipelines = aggregates
        .iter()
        .flatten()
        .map(|(key, aggregate)| {
            Ok((
                key.to_string(),
                pipeline_for_aggregate(aggregate.clone(), *aggregates_limit)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;

    if fields.is_some() {
        let fields_pipeline = pipeline_for_fields_facet(query_plan, query_level)?;
        facet_pipelines.insert(ROWS_FIELD.to_owned(), fields_pipeline);
    }

    // This builds a map that feeds into a `$replaceWith` pipeline stage to build a map of
    // aggregation results.
    let aggregate_selections: bson::Document = aggregates
        .iter()
        .flatten()
        .map(|(key, aggregate)| {
            // The facet result for each aggregate is an array containing a single document which
            // has a field called `result`. This code selects each facet result by name, and pulls
            // out the `result` value.
            let value_expr = doc! {
                "$getField": {
                    "field": RESULT_FIELD, // evaluates to the value of this field
                    "input": { "$first": get_field(key.as_str()) }, // field is accessed from this document
                },
            };

            // Matching SQL semantics, if a **count** aggregation does not match any rows we want
            // to return zero. Other aggregations should return null.
            let value_expr = if is_count(aggregate) {
                doc! {
                    "$ifNull": [value_expr, 0],
                }
            // Otherwise if the aggregate value is missing because the aggregation applied to an
            // empty document set then provide an explicit `null` value.
            } else {
                doc! {
                    "$ifNull": [value_expr, null]
                }
            };

            (key.to_string(), value_expr.into())
        })
        .collect();

    let select_aggregates = if !aggregate_selections.is_empty() {
        Some(("aggregates".to_owned(), aggregate_selections.into()))
    } else {
        None
    };

    let select_rows = match fields {
        Some(_) => Some(("rows".to_owned(), Bson::String(format!("${ROWS_FIELD}")))),
        _ => None,
    };

    let selection = Selection::new(
        [select_aggregates, select_rows]
            .into_iter()
            .flatten()
            .collect(),
    );

    Ok((facet_pipelines, selection))
}

fn is_count(aggregate: &Aggregate) -> bool {
    match aggregate {
        Aggregate::ColumnCount { .. } => true,
        Aggregate::StarCount { .. } => true,
        Aggregate::SingleColumn { .. } => false,
    }
}

// TODO: We can probably combine some aggregates in the same group stage:
// - single column
// - star count
// - column count, non-distinct
//
// We might still need separate facets for
// - column count, distinct
//
// The issue with non-distinct column count is we want to exclude null and non-existent values.
// That could probably be done with an accumulator like,
//
//     count: if $exists: ["$column", true] then 1 else 0
//
// Distinct counts need a group by the target column AFAIK so they need a facet.
fn pipeline_for_aggregate(aggregate: Aggregate, limit: Option<u32>) -> Result<Pipeline> {
    let pipeline = match aggregate {
        Aggregate::ColumnCount {
            column,
            field_path,
            distinct,
            ..
        } if distinct => {
            let target_field = mk_target_field(column, field_path);
            Pipeline::from_iter(
                [
                    Some(filter_to_documents_with_value(target_field.clone())?),
                    limit.map(Into::into).map(Stage::Limit),
                    Some(Stage::Group {
                        key_expression: ColumnRef::from_comparison_target(&target_field)
                            .into_aggregate_expression()
                            .into_bson(),
                        accumulators: [].into(),
                    }),
                    Some(Stage::Count(RESULT_FIELD.to_string())),
                ]
                .into_iter()
                .flatten(),
            )
        }

        // TODO: ENG-1465 count by distinct
        Aggregate::ColumnCount {
            column,
            field_path,
            distinct: _,
            ..
        } => Pipeline::from_iter(
            [
                Some(filter_to_documents_with_value(mk_target_field(
                    column, field_path,
                ))?),
                limit.map(Into::into).map(Stage::Limit),
                Some(Stage::Count(RESULT_FIELD.to_string())),
            ]
            .into_iter()
            .flatten(),
        ),

        Aggregate::SingleColumn {
            column,
            field_path,
            function,
            ..
        } => {
            use AggregationFunction as A;

            let target_field = ComparisonTarget::Column {
                name: column.clone(),
                arguments: Default::default(),
                field_path: field_path.clone(),
                field_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::Null)), // type does not matter here
            };
            let field_ref = ColumnRef::from_column_and_field_path(&column, field_path.as_ref())
                .into_aggregate_expression()
                .into_bson();

            let accumulator = match function {
                A::Avg => Accumulator::Avg(field_ref),
                A::Min => Accumulator::Min(field_ref),
                A::Max => Accumulator::Max(field_ref),
                A::Sum => Accumulator::Sum(field_ref),
            };
            Pipeline::from_iter(
                [
                    Some(filter_to_documents_with_value(target_field)?),
                    limit.map(Into::into).map(Stage::Limit),
                    Some(Stage::Group {
                        key_expression: Bson::Null,
                        accumulators: [(RESULT_FIELD.to_string(), accumulator)].into(),
                    }),
                ]
                .into_iter()
                .flatten(),
            )
        }

        Aggregate::StarCount {} => Pipeline::from_iter(
            [
                limit.map(Into::into).map(Stage::Limit),
                Some(Stage::Count(RESULT_FIELD.to_string())),
            ]
            .into_iter()
            .flatten(),
        ),
    };
    Ok(pipeline)
}

fn mk_target_field(name: FieldName, field_path: Option<Vec<FieldName>>) -> ComparisonTarget {
    ComparisonTarget::Column {
        name,
        arguments: Default::default(),
        field_path,
        field_type: Type::Scalar(MongoScalarType::ExtendedJSON), // type does not matter here
    }
}

fn filter_to_documents_with_value(target_field: ComparisonTarget) -> Result<Stage> {
    Ok(Stage::Match(make_selector(
        &Expression::BinaryComparisonOperator {
            column: target_field,
            operator: ComparisonFunction::NotEqual,
            value: ComparisonValue::Scalar {
                value: serde_json::Value::Null,
                value_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::Null)),
            },
        },
    )?))
}

#[cfg(test)]
mod tests {
    use configuration::Configuration;
    use mongodb::bson::bson;
    use ndc_test_helpers::{
        binop, collection, column_aggregate, column_count_aggregate, dimension_column, field,
        group, grouping, named_type, object_type, query, query_request, row_set, target, value,
    };
    use serde_json::json;

    use crate::{
        mongo_query_plan::MongoConfiguration,
        mongodb::test_helpers::mock_collection_aggregate_response_for_pipeline,
        query::execute_query_request::execute_query_request, test_helpers::mflix_config,
    };

    #[tokio::test]
    async fn executes_aggregation() -> Result<(), anyhow::Error> {
        let query_request = query_request()
            .collection("students")
            .query(query().aggregates([
                column_count_aggregate!("count" => "gpa", distinct: true),
                column_aggregate!("avg" => "gpa", "avg"),
            ]))
            .into();

        let expected_response = row_set()
            .aggregates([("count", json!(11)), ("avg", json!(3))])
            .into_response();

        let expected_pipeline = bson!([
            {
                "$facet": {
                    "avg": [
                        { "$match": { "gpa": { "$ne": null } } },
                        { "$group": { "_id": null, "result": { "$avg": "$gpa" } } },
                    ],
                    "count": [
                        { "$match": { "gpa": { "$ne": null } } },
                        { "$group": { "_id": "$gpa" } },
                        { "$count": "result" },
                    ],
                },
            },
            {
                "$replaceWith": {
                    "aggregates": {
                        "avg": {
                            "$ifNull": [
                                {
                                    "$getField": {
                                        "field": "result",
                                        "input": { "$first": { "$getField": { "$literal": "avg" } } },
                                    }
                                },
                                null
                            ]
                        },
                        "count": {
                            "$ifNull": [
                                {
                                    "$getField": {
                                        "field": "result",
                                        "input": { "$first": { "$getField": { "$literal": "count" } } },
                                    }
                                },
                                0,
                            ]
                        },
                    },
                },
            },
        ]);

        let db = mock_collection_aggregate_response_for_pipeline(
            "students",
            expected_pipeline,
            bson!([{
                "aggregates": {
                    "count": 11,
                    "avg": 3,
                },
            }]),
        );

        let result = execute_query_request(db, &students_config(), query_request).await?;
        assert_eq!(result, expected_response);
        Ok(())
    }

    #[tokio::test]
    async fn executes_aggregation_with_fields() -> Result<(), anyhow::Error> {
        let query_request = query_request()
            .collection("students")
            .query(
                query()
                    .aggregates([column_aggregate!("avg" => "gpa", "avg")])
                    .fields([field!("student_gpa" => "gpa")])
                    .predicate(binop("_lt", target!("gpa"), value!(4.0))),
            )
            .into();

        let expected_response = row_set()
            .aggregates([("avg", json!(3.1))])
            .row([("student_gpa", 3.1)])
            .into_response();

        let expected_pipeline = bson!([
            { "$match": { "gpa": { "$lt": 4.0 } } },
            {
                "$facet": {
                    "__ROWS__": [{
                        "$replaceWith": {
                            "student_gpa": { "$ifNull": ["$gpa", null] },
                        },
                    }],
                    "avg": [
                        { "$match": { "gpa": { "$ne": null } } },
                        { "$group": { "_id": null, "result": { "$avg": "$gpa" } } },
                    ],
                },
            },
            {
                "$replaceWith": {
                    "aggregates": {
                        "avg": {
                            "$ifNull": [
                                {
                                    "$getField": {
                                        "field": "result",
                                        "input": { "$first": { "$getField": { "$literal": "avg" } } },
                                    }
                                },
                                null
                            ]
                        },
                    },
                    "rows": "$__ROWS__",
                },
            },
        ]);

        let db = mock_collection_aggregate_response_for_pipeline(
            "students",
            expected_pipeline,
            bson!([{
                "aggregates": {
                    "avg": 3.1,
                },
                "rows": [{
                    "student_gpa": 3.1,
                }],
            }]),
        );

        let result = execute_query_request(db, &students_config(), query_request).await?;
        assert_eq!(result, expected_response);
        Ok(())
    }

    #[tokio::test]
    async fn executes_query_with_groups_with_single_column_aggregates() -> Result<(), anyhow::Error>
    {
        let query_request = query_request()
            .collection("movies")
            .query(
                query().groups(
                    grouping()
                        .dimensions([dimension_column("year")])
                        .aggregates([
                            (
                                "average_viewer_rating",
                                column_aggregate("tomatoes.viewer.rating", "avg"),
                            ),
                            ("max_runtime", column_aggregate("runtime", "max")),
                        ]),
                ),
            )
            .into();

        let expected_response = row_set()
            .groups([
                group(
                    [2007],
                    [
                        ("average_viewer_rating", json!(7.5)),
                        ("max_runtime", json!(207)),
                    ],
                ),
                group(
                    [2015],
                    [
                        ("average_viewer_rating", json!(6.9)),
                        ("max_runtime", json!(412)),
                    ],
                ),
            ])
            .into_response();

        let expected_pipeline = bson!([
            {
                "$group": {
                    "_id": "$year",
                    "average_viewer_rating": { "$avg": "$tomatoes.viewer.rating" } ,
                    "max_runtime": { "$max": "$runtime" },
                }
            },
        ]);

        let db = mock_collection_aggregate_response_for_pipeline(
            "movies",
            expected_pipeline,
            bson!([
                {
                    "_id": 2007,
                    "average_viewer_rating": 7.5,
                    "max_runtime": 207,
                },
                {
                    "_id": 2015,
                    "average_viewer_rating": 6.9,
                    "max_runtime": 412,
                },
            ]),
        );

        let result = execute_query_request(db, &mflix_config(), query_request).await?;
        assert_eq!(result, expected_response);
        Ok(())
    }

    fn students_config() -> MongoConfiguration {
        MongoConfiguration(Configuration {
            collections: [collection("students")].into(),
            object_types: [(
                "students".into(),
                object_type([("gpa", named_type("Double"))]),
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
