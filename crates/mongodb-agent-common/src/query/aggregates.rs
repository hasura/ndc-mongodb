use std::collections::BTreeMap;

use indexmap::IndexMap;
use mongodb::bson::{bson, Bson};
use mongodb_support::aggregate::{Accumulator, Pipeline, Selection, Stage};
use ndc_models::FieldName;

use crate::{aggregation_function::AggregationFunction, mongo_query_plan::Aggregate};

use super::column_ref::ColumnRef;

pub fn pipeline_for_aggregates(aggregates: &IndexMap<FieldName, Aggregate>) -> Pipeline {
    let group_stage = Stage::Group {
        key_expression: Bson::Null,
        accumulators: accumulators_for_aggregates(aggregates),
    };
    let replace_with_stage = Stage::ReplaceWith(selection_for_aggregates(aggregates));
    Pipeline::new(vec![group_stage, replace_with_stage])
}

pub fn accumulators_for_aggregates(
    aggregates: &IndexMap<FieldName, Aggregate>,
) -> BTreeMap<String, Accumulator> {
    aggregates
        .into_iter()
        .map(|(name, aggregate)| (name.to_string(), aggregate_to_accumulator(aggregate)))
        .collect()
}

fn aggregate_to_accumulator(aggregate: &Aggregate) -> Accumulator {
    use Aggregate as A;
    match aggregate {
        A::ColumnCount {
            column,
            field_path,
            distinct,
            ..
        } => {
            let field_ref = ColumnRef::from_column_and_field_path(column, field_path.as_ref())
                .into_aggregate_expression()
                .into_bson();
            if *distinct {
                Accumulator::AddToSet(field_ref)
            } else {
                Accumulator::Sum(bson!({
                    "$cond": {
                        "if": { "$eq": [field_ref, null] }, // count non-null, non-missing values
                        "then": 0,
                        "else": 1,
                    }
                }))
            }
        }
        A::SingleColumn {
            column,
            field_path,
            function,
            ..
        } => {
            use AggregationFunction as A;

            let field_ref = ColumnRef::from_column_and_field_path(column, field_path.as_ref())
                .into_aggregate_expression()
                .into_bson();

            match function {
                A::Avg => Accumulator::Avg(field_ref),
                A::Min => Accumulator::Min(field_ref),
                A::Max => Accumulator::Max(field_ref),
                A::Sum => Accumulator::Sum(field_ref),
            }
        }
        A::StarCount => Accumulator::Sum(bson!(1)),
    }
}

fn selection_for_aggregates(aggregates: &IndexMap<FieldName, Aggregate>) -> Selection {
    let selected_aggregates = aggregates
        .iter()
        .map(|(key, aggregate)| selection_for_aggregate(key, aggregate))
        .collect();
    Selection::new(selected_aggregates)
}

pub fn selection_for_aggregate(key: &FieldName, aggregate: &Aggregate) -> (String, Bson) {
    let column_ref = ColumnRef::from_field(key.as_ref()).into_aggregate_expression();

    // Selecting distinct counts requires some post-processing since the $group stage produces
    // an array of unique values. We need to count the non-null values in that array.
    let value_expression = match aggregate {
        Aggregate::ColumnCount { distinct, .. } if *distinct => bson!({
            "$reduce": {
                "input": column_ref,
                "initialValue": 0,
                "in": {
                    "$cond": {
                        "if": { "$eq": ["$$this", null] },
                        "then": "$$value",
                        "else": { "$sum": ["$$value", 1] },
                    }
                },
            }
        }),
        _ => column_ref.into(),
    };

    // Fill in null or zero values for missing fields. If we skip this we get errors on missing
    // data down the line.
    let value_expression = replace_missing_aggregate_value(value_expression, aggregate.is_count());

    // Convert types to match what the engine expects for each aggregation result
    let value_expression = convert_aggregate_result_type(value_expression, aggregate);

    (key.to_string(), value_expression)
}

pub fn replace_missing_aggregate_value(expression: Bson, is_count: bool) -> Bson {
    bson!({
        "$ifNull": [
            expression,
            if is_count { bson!(0) } else { bson!(null) }
        ]
    })
}

/// The system expects specific return types for specific aggregates. That means we may need
/// to do a numeric type conversion here. The conversion applies to the aggregated result,
/// not to input values.
fn convert_aggregate_result_type(column_ref: impl Into<Bson>, aggregate: &Aggregate) -> Bson {
    let convert_to = match aggregate {
        Aggregate::ColumnCount { .. } => None,
        Aggregate::SingleColumn {
            column_type,
            function,
            ..
        } => function.expected_result_type(column_type),
        Aggregate::StarCount => None,
    };
    match convert_to {
        // $convert implicitly fills `null` if input value is missing
        Some(scalar_type) => bson!({
                "$convert": {
                "input": column_ref,
                "to": scalar_type.bson_name(),
            }
        }),
        None => column_ref.into(),
    }
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
                ("avg", column_aggregate("gpa", "avg").into()),
            ]))
            .into();

        let expected_response = row_set()
            .aggregates([("count", json!(11)), ("avg", json!(3))])
            .into_response();

        let expected_pipeline = bson!([
            {
                "$facet": {
                    "avg": [
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
                            "$convert": {
                                "input": {
                                    "$getField": {
                                        "field": "result",
                                        "input": { "$first": { "$getField": { "$literal": "avg" } } },
                                    }
                                },
                                "to": "double",
                            }
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
                    .aggregates([("avg", column_aggregate("gpa", "avg"))])
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
                        { "$group": { "_id": null, "result": { "$avg": "$gpa" } } },
                    ],
                },
            },
            {
                "$replaceWith": {
                    "aggregates": {
                        "avg": {
                            "$convert": {
                                "input": {
                                    "$getField": {
                                        "field": "result",
                                        "input": { "$first": { "$getField": { "$literal": "avg" } } },
                                    }
                                },
                                "to": "double",
                            }
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
                            ("max.runtime", column_aggregate("runtime", "max")),
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
                        ("max.runtime", json!(207)),
                    ],
                ),
                group(
                    [2015],
                    [
                        ("average_viewer_rating", json!(6.9)),
                        ("max.runtime", json!(412)),
                    ],
                ),
            ])
            .into_response();

        let expected_pipeline = bson!([
            {
                "$group": {
                    "_id": ["$year"],
                    "average_viewer_rating": { "$avg": "$tomatoes.viewer.rating" },
                    "max.runtime": { "$max": "$runtime" },
                }
            },
            {
                "$replaceWith": {
                    "dimensions": "$_id",
                    "average_viewer_rating": { "$convert": { "input": "$average_viewer_rating", "to": "double" } },
                    "max.runtime": { "$ifNull": [{ "$getField": { "$literal": "max.runtime" } }, null] },
                }
            },
        ]);

        let db = mock_collection_aggregate_response_for_pipeline(
            "movies",
            expected_pipeline,
            bson!([
                {
                    "dimensions": [2007],
                    "average_viewer_rating": 7.5,
                    "max.runtime": 207,
                },
                {
                    "dimensions": [2015],
                    "average_viewer_rating": 6.9,
                    "max.runtime": 412,
                },
            ]),
        );

        let result = execute_query_request(db, &mflix_config(), query_request).await?;
        assert_eq!(result, expected_response);
        Ok(())
    }

    // TODO: Test:
    // - fields & group by
    // - group by & aggregates
    // - various counts on groups
    // - groups and variables
    // - groups and relationships

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
