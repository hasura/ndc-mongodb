//! Tests for pipeline builder.

use std::collections::BTreeMap;

use mongodb::bson::{doc, Bson};
use mongodb_support::aggregate::{Accumulator, SortDocument, Stage};
use ndc_models::{
    Float64, JoinOn, JoinType, NullsSort, OrderDirection, Relation, RelationalExpression,
    RelationalLiteral, Sort,
};

use crate::relational::build_relational_pipeline;

#[test]
fn builds_pipeline_for_from_relation() {
    let relation = Relation::From {
        collection: "users".into(),
        columns: vec!["name".into(), "age".into()],
        arguments: Default::default(),
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "users");
    assert!(result.pipeline.is_empty()); // From alone has no stages
    assert_eq!(result.output_columns.len(), 2);
    assert_eq!(result.output_columns.field_for_index(0), Some("name"));
    assert_eq!(result.output_columns.field_for_index(1), Some("age"));
}

#[test]
fn builds_pipeline_for_filter_relation() {
    let relation = Relation::Filter {
        input: Box::new(Relation::From {
            collection: "users".into(),
            columns: vec!["name".into(), "age".into()],
            arguments: Default::default(),
        }),
        predicate: RelationalExpression::Gt {
            left: Box::new(RelationalExpression::Column { index: 1 }),
            right: Box::new(RelationalExpression::Literal {
                literal: RelationalLiteral::Int64 { value: 18 },
            }),
        },
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "users");
    // With query document optimization, we get 2 stages:
    // 1. Early $match with query document syntax (index-friendly, from early match optimization)
    // 2. Regular $match with query document syntax (from build_filter optimization)
    assert_eq!(result.pipeline.stages.len(), 2);
    // First stage is the early match optimization (index-friendly)
    assert_eq!(
        result.pipeline.stages[0],
        Stage::Match(doc! { "age": { "$gt": 18_i64 } })
    );
    // Second stage is also query document syntax (not $expr) thanks to Stage 4 optimization
    assert_eq!(
        result.pipeline.stages[1],
        Stage::Match(doc! { "age": { "$gt": 18_i64 } })
    );
}

#[test]
fn builds_pipeline_for_sort_relation() {
    let relation = Relation::Sort {
        input: Box::new(Relation::From {
            collection: "products".into(),
            columns: vec!["name".into(), "price".into()],
            arguments: Default::default(),
        }),
        exprs: vec![Sort {
            expr: RelationalExpression::Column { index: 1 },
            direction: OrderDirection::Desc,
            nulls_sort: NullsSort::NullsLast,
        }],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "products");
    assert_eq!(result.pipeline.stages.len(), 1);
    assert_eq!(
        result.pipeline.stages[0],
        Stage::Sort(SortDocument(doc! { "price": -1 }))
    );
}

#[test]
fn builds_pipeline_for_sort_with_case_expression() {
    // Test sorting with a Case expression (like the user's failing query)
    let relation = Relation::Sort {
        input: Box::new(Relation::From {
            collection: "data".into(),
            columns: vec!["category".into(), "value".into()],
            arguments: Default::default(),
        }),
        exprs: vec![Sort {
            expr: RelationalExpression::Case {
                scrutinee: Some(Box::new(RelationalExpression::Column { index: 0 })),
                when: vec![
                    ndc_models::CaseWhen {
                        when: RelationalExpression::Literal {
                            literal: ndc_models::RelationalLiteral::String { value: "A".into() },
                        },
                        then: RelationalExpression::Literal {
                            literal: ndc_models::RelationalLiteral::Int64 { value: 1 },
                        },
                    },
                    ndc_models::CaseWhen {
                        when: RelationalExpression::Literal {
                            literal: ndc_models::RelationalLiteral::String { value: "B".into() },
                        },
                        then: RelationalExpression::Literal {
                            literal: ndc_models::RelationalLiteral::Int64 { value: 2 },
                        },
                    },
                ],
                default: Some(Box::new(RelationalExpression::Literal {
                    literal: ndc_models::RelationalLiteral::Int64 { value: 99 },
                })),
            },
            direction: OrderDirection::Asc,
            nulls_sort: NullsSort::NullsLast,
        }],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "data");
    // Should have: $addFields, $sort, $unset (via Stage::Other)
    assert_eq!(result.pipeline.stages.len(), 3);

    // First stage: $addFields with the Case expression computed and null-order key
    // (NullsLast + Asc is non-default, so a __null_order_0 key is added)
    match &result.pipeline.stages[0] {
        Stage::AddFields(doc) => {
            assert!(doc.contains_key("__sort_key_0"));
            assert!(doc.contains_key("__null_order_0"));
        }
        other => panic!("Expected AddFields stage, got {:?}", other),
    }

    // Second stage: $sort on null-order key first, then computed field
    assert_eq!(
        result.pipeline.stages[1],
        Stage::Sort(SortDocument(doc! { "__null_order_0": 1, "__sort_key_0": 1 }))
    );

    // Third stage: $unset to remove the temporary fields
    match &result.pipeline.stages[2] {
        Stage::Other(doc) => {
            assert!(doc.contains_key("$unset"));
        }
        other => panic!("Expected Other (unset) stage, got {:?}", other),
    }
}

#[test]
fn builds_pipeline_for_paginate_relation() {
    let relation = Relation::Paginate {
        input: Box::new(Relation::From {
            collection: "items".into(),
            columns: vec!["id".into()],
            arguments: Default::default(),
        }),
        skip: 10,
        fetch: Some(20),
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "items");
    assert_eq!(result.pipeline.stages.len(), 2);
    assert_eq!(result.pipeline.stages[0], Stage::Skip(Bson::Int64(10)));
    assert_eq!(result.pipeline.stages[1], Stage::Limit(Bson::Int64(20)));
}

#[test]
fn builds_pipeline_for_nested_relations() {
    let relation = Relation::Paginate {
        input: Box::new(Relation::Sort {
            input: Box::new(Relation::Filter {
                input: Box::new(Relation::From {
                    collection: "orders".into(),
                    columns: vec!["id".into(), "total".into(), "status".into()],
                    arguments: Default::default(),
                }),
                predicate: RelationalExpression::Eq {
                    left: Box::new(RelationalExpression::Column { index: 2 }),
                    right: Box::new(RelationalExpression::Literal {
                        literal: RelationalLiteral::String {
                            value: "pending".to_string(),
                        },
                    }),
                },
            }),
            exprs: vec![Sort {
                expr: RelationalExpression::Column { index: 1 },
                direction: OrderDirection::Desc,
                nulls_sort: NullsSort::NullsLast,
            }],
        }),
        skip: 0,
        fetch: Some(10),
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "orders");
    // With query document optimization, we get 4 stages:
    // 1. Early $match with query document syntax (index-friendly)
    // 2. Regular $match with query document syntax (Stage 4 optimization)
    // 3. $sort
    // 4. $limit
    assert_eq!(result.pipeline.stages.len(), 4);
    // First stage is the early match optimization (index-friendly)
    assert_eq!(
        result.pipeline.stages[0],
        Stage::Match(doc! { "status": { "$eq": "pending" } })
    );
    // Second stage is also query document syntax (not $expr) thanks to Stage 4 optimization
    assert_eq!(
        result.pipeline.stages[1],
        Stage::Match(doc! { "status": { "$eq": "pending" } })
    );
    assert_eq!(
        result.pipeline.stages[2],
        Stage::Sort(SortDocument(doc! { "total": -1 }))
    );
    assert_eq!(result.pipeline.stages[3], Stage::Limit(Bson::Int64(10)));
}

#[test]
fn skips_skip_stage_when_zero() {
    let relation = Relation::Paginate {
        input: Box::new(Relation::From {
            collection: "items".into(),
            columns: vec!["id".into()],
            arguments: Default::default(),
        }),
        skip: 0,
        fetch: Some(5),
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.pipeline.stages.len(), 1);
    assert_eq!(result.pipeline.stages[0], Stage::Limit(Bson::Int64(5)));
}

#[test]
fn skips_limit_stage_when_none() {
    let relation = Relation::Paginate {
        input: Box::new(Relation::From {
            collection: "items".into(),
            columns: vec!["id".into()],
            arguments: Default::default(),
        }),
        skip: 5,
        fetch: None,
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.pipeline.stages.len(), 1);
    assert_eq!(result.pipeline.stages[0], Stage::Skip(Bson::Int64(5)));
}

// Phase 2 tests - Project relation

#[test]
fn builds_pipeline_for_project_relation() {
    let relation = Relation::Project {
        input: Box::new(Relation::From {
            collection: "users".into(),
            columns: vec!["first_name".into(), "last_name".into(), "age".into()],
            arguments: Default::default(),
        }),
        exprs: vec![
            // Select first_name as col_0
            RelationalExpression::Column { index: 0 },
            // Select age as col_1
            RelationalExpression::Column { index: 2 },
        ],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "users");
    assert_eq!(result.pipeline.stages.len(), 1);
    assert_eq!(
        result.pipeline.stages[0],
        Stage::Project(doc! {
            "_id": 0,
            "col_0": "$first_name",
            "col_1": "$age"
        })
    );
    // Column mapping should be updated
    assert_eq!(result.output_columns.len(), 2);
    assert_eq!(result.output_columns.field_for_index(0), Some("col_0"));
    assert_eq!(result.output_columns.field_for_index(1), Some("col_1"));
}

#[test]
fn builds_pipeline_for_project_with_expressions() {
    let relation = Relation::Project {
        input: Box::new(Relation::From {
            collection: "products".into(),
            columns: vec!["name".into(), "price".into(), "quantity".into()],
            arguments: Default::default(),
        }),
        exprs: vec![
            // Select name as col_0
            RelationalExpression::Column { index: 0 },
            // Compute price * quantity as col_1
            RelationalExpression::Multiply {
                left: Box::new(RelationalExpression::Column { index: 1 }),
                right: Box::new(RelationalExpression::Column { index: 2 }),
            },
        ],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "products");
    assert_eq!(result.pipeline.stages.len(), 1);
    assert_eq!(
        result.pipeline.stages[0],
        Stage::Project(doc! {
            "_id": 0,
            "col_0": "$name",
            "col_1": { "$multiply": ["$price", "$quantity"] }
        })
    );
}

#[test]
fn builds_pipeline_for_project_then_filter() {
    let relation = Relation::Filter {
        input: Box::new(Relation::Project {
            input: Box::new(Relation::From {
                collection: "users".into(),
                columns: vec!["name".into(), "age".into()],
                arguments: Default::default(),
            }),
            exprs: vec![
                RelationalExpression::Column { index: 0 },
                RelationalExpression::Column { index: 1 },
            ],
        }),
        // Filter on col_1 (which is age after projection)
        predicate: RelationalExpression::Gt {
            left: Box::new(RelationalExpression::Column { index: 1 }),
            right: Box::new(RelationalExpression::Literal {
                literal: RelationalLiteral::Int64 { value: 21 },
            }),
        },
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "users");
    // With predicate pushdown AND early match optimization, we get 3 stages:
    // 1. Early $match on original field (index-friendly)
    // 2. Regular $match with query document syntax (filter was pushed below project)
    // 3. $project
    assert_eq!(result.pipeline.stages.len(), 3);
    // First stage is the early match optimization (index-friendly)
    assert_eq!(
        result.pipeline.stages[0],
        Stage::Match(doc! { "age": { "$gt": 21_i64 } })
    );
    // Second stage is also query document syntax thanks to Stage 4 optimization
    assert_eq!(
        result.pipeline.stages[1],
        Stage::Match(doc! { "age": { "$gt": 21_i64 } })
    );
    // Third stage is project
    assert_eq!(
        result.pipeline.stages[2],
        Stage::Project(doc! {
            "_id": 0,
            "col_0": "$name",
            "col_1": "$age"
        })
    );
}

// Phase 3 tests - Aggregate relation

#[test]
fn builds_pipeline_for_aggregate_with_no_group_by() {
    // SELECT COUNT(*), SUM(amount) FROM orders
    let relation = Relation::Aggregate {
        input: Box::new(Relation::From {
            collection: "orders".into(),
            columns: vec!["id".into(), "amount".into()],
            arguments: Default::default(),
        }),
        group_by: vec![],
        aggregates: vec![
            RelationalExpression::Count {
                expr: Box::new(RelationalExpression::Literal {
                    literal: RelationalLiteral::Null,
                }),
                distinct: false,
            },
            RelationalExpression::Sum {
                expr: Box::new(RelationalExpression::Column { index: 1 }),
            },
        ],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "orders");
    assert_eq!(result.pipeline.stages.len(), 2);

    // First stage is $group with _id: null
    let mut expected_accumulators = BTreeMap::new();
    expected_accumulators.insert("_a0".to_string(), Accumulator::Sum(Bson::Int32(1)));
    expected_accumulators.insert(
        "_a1".to_string(),
        Accumulator::Sum(Bson::String("$amount".to_string())),
    );
    assert_eq!(
        result.pipeline.stages[0],
        Stage::Group {
            key_expression: Bson::Null,
            accumulators: expected_accumulators,
        }
    );

    // Second stage is $project to remap columns
    assert_eq!(
        result.pipeline.stages[1],
        Stage::Project(doc! {
            "_id": 0,
            "col_0": "$_a0",
            "col_1": "$_a1"
        })
    );

    // Column mapping should reflect the output
    assert_eq!(result.output_columns.len(), 2);
    assert_eq!(result.output_columns.field_for_index(0), Some("col_0"));
    assert_eq!(result.output_columns.field_for_index(1), Some("col_1"));
}

#[test]
fn builds_pipeline_for_aggregate_with_single_group_by() {
    // SELECT city, COUNT(*), AVG(amount) FROM orders GROUP BY city
    let relation = Relation::Aggregate {
        input: Box::new(Relation::From {
            collection: "orders".into(),
            columns: vec!["city".into(), "amount".into()],
            arguments: Default::default(),
        }),
        group_by: vec![RelationalExpression::Column { index: 0 }],
        aggregates: vec![
            RelationalExpression::Count {
                expr: Box::new(RelationalExpression::Literal {
                    literal: RelationalLiteral::Null,
                }),
                distinct: false,
            },
            RelationalExpression::Average {
                expr: Box::new(RelationalExpression::Column { index: 1 }),
            },
        ],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "orders");
    assert_eq!(result.pipeline.stages.len(), 2);

    // First stage is $group with _id: "$city"
    let mut expected_accumulators = BTreeMap::new();
    expected_accumulators.insert("_a0".to_string(), Accumulator::Sum(Bson::Int32(1)));
    expected_accumulators.insert(
        "_a1".to_string(),
        Accumulator::Avg(Bson::String("$amount".to_string())),
    );
    assert_eq!(
        result.pipeline.stages[0],
        Stage::Group {
            key_expression: Bson::String("$city".to_string()),
            accumulators: expected_accumulators,
        }
    );

    // Second stage is $project - group by column comes first
    assert_eq!(
        result.pipeline.stages[1],
        Stage::Project(doc! {
            "_id": 0,
            "col_0": "$_id",
            "col_1": "$_a0",
            "col_2": "$_a1"
        })
    );

    // Column mapping: col_0 = city, col_1 = count, col_2 = avg
    assert_eq!(result.output_columns.len(), 3);
}

#[test]
fn builds_pipeline_for_aggregate_with_multiple_group_by() {
    // SELECT city, year, SUM(amount) FROM orders GROUP BY city, year
    let relation = Relation::Aggregate {
        input: Box::new(Relation::From {
            collection: "orders".into(),
            columns: vec!["city".into(), "year".into(), "amount".into()],
            arguments: Default::default(),
        }),
        group_by: vec![
            RelationalExpression::Column { index: 0 },
            RelationalExpression::Column { index: 1 },
        ],
        aggregates: vec![RelationalExpression::Sum {
            expr: Box::new(RelationalExpression::Column { index: 2 }),
        }],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "orders");
    assert_eq!(result.pipeline.stages.len(), 2);

    // First stage is $group with _id: { _g0: "$city", _g1: "$year" }
    let mut expected_accumulators = BTreeMap::new();
    expected_accumulators.insert(
        "_a0".to_string(),
        Accumulator::Sum(Bson::String("$amount".to_string())),
    );
    assert_eq!(
        result.pipeline.stages[0],
        Stage::Group {
            key_expression: Bson::Document(doc! { "_g0": "$city", "_g1": "$year" }),
            accumulators: expected_accumulators,
        }
    );

    // Second stage is $project - group by columns come first
    assert_eq!(
        result.pipeline.stages[1],
        Stage::Project(doc! {
            "_id": 0,
            "col_0": "$_id._g0",
            "col_1": "$_id._g1",
            "col_2": "$_a0"
        })
    );

    // Column mapping: col_0 = city, col_1 = year, col_2 = sum
    assert_eq!(result.output_columns.len(), 3);
}

#[test]
fn builds_pipeline_for_aggregate_with_min_max() {
    // SELECT MIN(price), MAX(price) FROM products
    let relation = Relation::Aggregate {
        input: Box::new(Relation::From {
            collection: "products".into(),
            columns: vec!["price".into()],
            arguments: Default::default(),
        }),
        group_by: vec![],
        aggregates: vec![
            RelationalExpression::Min {
                expr: Box::new(RelationalExpression::Column { index: 0 }),
            },
            RelationalExpression::Max {
                expr: Box::new(RelationalExpression::Column { index: 0 }),
            },
        ],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    let mut expected_accumulators = BTreeMap::new();
    expected_accumulators.insert(
        "_a0".to_string(),
        Accumulator::Min(Bson::String("$price".to_string())),
    );
    expected_accumulators.insert(
        "_a1".to_string(),
        Accumulator::Max(Bson::String("$price".to_string())),
    );
    assert_eq!(
        result.pipeline.stages[0],
        Stage::Group {
            key_expression: Bson::Null,
            accumulators: expected_accumulators,
        }
    );
}

#[test]
fn builds_pipeline_for_aggregate_with_first_last() {
    // SELECT FIRST_VALUE(name), LAST_VALUE(name) FROM users
    let relation = Relation::Aggregate {
        input: Box::new(Relation::From {
            collection: "users".into(),
            columns: vec!["name".into()],
            arguments: Default::default(),
        }),
        group_by: vec![],
        aggregates: vec![
            RelationalExpression::FirstValue {
                expr: Box::new(RelationalExpression::Column { index: 0 }),
            },
            RelationalExpression::LastValue {
                expr: Box::new(RelationalExpression::Column { index: 0 }),
            },
        ],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    let mut expected_accumulators = BTreeMap::new();
    expected_accumulators.insert(
        "_a0".to_string(),
        Accumulator::First(Bson::String("$name".to_string())),
    );
    expected_accumulators.insert(
        "_a1".to_string(),
        Accumulator::Last(Bson::String("$name".to_string())),
    );
    assert_eq!(
        result.pipeline.stages[0],
        Stage::Group {
            key_expression: Bson::Null,
            accumulators: expected_accumulators,
        }
    );
}

#[test]
fn builds_pipeline_for_aggregate_with_stddev() {
    // SELECT STDDEV(score), STDDEV_POP(score) FROM results
    let relation = Relation::Aggregate {
        input: Box::new(Relation::From {
            collection: "results".into(),
            columns: vec!["score".into()],
            arguments: Default::default(),
        }),
        group_by: vec![],
        aggregates: vec![
            RelationalExpression::Stddev {
                expr: Box::new(RelationalExpression::Column { index: 0 }),
            },
            RelationalExpression::StddevPop {
                expr: Box::new(RelationalExpression::Column { index: 0 }),
            },
        ],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    let mut expected_accumulators = BTreeMap::new();
    expected_accumulators.insert(
        "_a0".to_string(),
        Accumulator::StdDevSamp(Bson::String("$score".to_string())),
    );
    expected_accumulators.insert(
        "_a1".to_string(),
        Accumulator::StdDevPop(Bson::String("$score".to_string())),
    );
    assert_eq!(
        result.pipeline.stages[0],
        Stage::Group {
            key_expression: Bson::Null,
            accumulators: expected_accumulators,
        }
    );
}

#[test]
fn builds_pipeline_for_aggregate_with_array_agg() {
    // SELECT ARRAY_AGG(name) FROM users GROUP BY city
    let relation = Relation::Aggregate {
        input: Box::new(Relation::From {
            collection: "users".into(),
            columns: vec!["city".into(), "name".into()],
            arguments: Default::default(),
        }),
        group_by: vec![RelationalExpression::Column { index: 0 }],
        aggregates: vec![RelationalExpression::ArrayAgg {
            expr: Box::new(RelationalExpression::Column { index: 1 }),
            distinct: false,
            order_by: None,
        }],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    let mut expected_accumulators = BTreeMap::new();
    expected_accumulators.insert(
        "_a0".to_string(),
        Accumulator::Push(Bson::String("$name".to_string())),
    );
    assert_eq!(
        result.pipeline.stages[0],
        Stage::Group {
            key_expression: Bson::String("$city".to_string()),
            accumulators: expected_accumulators,
        }
    );
}

#[test]
fn builds_pipeline_for_aggregate_with_distinct_array_agg() {
    // SELECT ARRAY_AGG(DISTINCT name) FROM users
    let relation = Relation::Aggregate {
        input: Box::new(Relation::From {
            collection: "users".into(),
            columns: vec!["name".into()],
            arguments: Default::default(),
        }),
        group_by: vec![],
        aggregates: vec![RelationalExpression::ArrayAgg {
            expr: Box::new(RelationalExpression::Column { index: 0 }),
            distinct: true,
            order_by: None,
        }],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    let mut expected_accumulators = BTreeMap::new();
    expected_accumulators.insert(
        "_a0".to_string(),
        Accumulator::AddToSet(Bson::String("$name".to_string())),
    );
    assert_eq!(
        result.pipeline.stages[0],
        Stage::Group {
            key_expression: Bson::Null,
            accumulators: expected_accumulators,
        }
    );
}

#[test]
fn builds_pipeline_for_aggregate_with_count_distinct() {
    // SELECT city, COUNT(DISTINCT product_id) FROM orders GROUP BY city
    // Count distinct should use $addToSet in $group and $size in $project
    let relation = Relation::Aggregate {
        input: Box::new(Relation::From {
            collection: "orders".into(),
            columns: vec!["city".into(), "product_id".into()],
            arguments: Default::default(),
        }),
        group_by: vec![RelationalExpression::Column { index: 0 }],
        aggregates: vec![RelationalExpression::Count {
            expr: Box::new(RelationalExpression::Column { index: 1 }),
            distinct: true,
        }],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    // Verify $group stage uses $addToSet for distinct count
    let mut expected_accumulators = BTreeMap::new();
    expected_accumulators.insert(
        "_a0".to_string(),
        Accumulator::AddToSet(Bson::String("$product_id".to_string())),
    );
    assert_eq!(
        result.pipeline.stages[0],
        Stage::Group {
            key_expression: Bson::String("$city".to_string()),
            accumulators: expected_accumulators,
        }
    );

    // Verify $project stage applies $size to convert array to count
    if let Stage::Project(project_doc) = &result.pipeline.stages[1] {
        // col_1 should be { "$size": "$_a0" }, not just "$_a0"
        let col_1_value = project_doc.get("col_1").expect("col_1 should exist");
        assert_eq!(col_1_value, &Bson::Document(doc! { "$size": "$_a0" }));
    } else {
        panic!("Expected $project stage at index 1");
    }
}

// ============================================================================
// Phase 4: Join Tests
// ============================================================================

#[test]
fn builds_pipeline_for_left_join() {
    let relation = Relation::Join {
        left: Box::new(Relation::From {
            collection: "orders".into(),
            columns: vec!["order_id".into(), "customer_id".into()],
            arguments: Default::default(),
        }),
        right: Box::new(Relation::From {
            collection: "customers".into(),
            columns: vec!["id".into(), "name".into()],
            arguments: Default::default(),
        }),
        on: vec![JoinOn {
            left: RelationalExpression::Column { index: 1 },
            right: RelationalExpression::Column { index: 0 },
        }],
        join_type: JoinType::Left,
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "orders");
    assert_eq!(result.pipeline.stages.len(), 3); // $lookup, $unwind, $project

    // Check $lookup stage
    match &result.pipeline.stages[0] {
        Stage::Lookup {
            from,
            r#as,
            r#let,
            pipeline,
            ..
        } => {
            assert_eq!(from.as_deref(), Some("customers"));
            assert_eq!(r#as, "_joined");
            assert!(r#let.is_some());
            assert!(pipeline.is_some());
        }
        other => panic!("Expected Lookup stage, got {:?}", other),
    }

    // Check $unwind stage with preserveNullAndEmptyArrays
    match &result.pipeline.stages[1] {
        Stage::Unwind {
            path,
            preserve_null_and_empty_arrays,
            ..
        } => {
            assert_eq!(path, "$_joined");
            assert_eq!(*preserve_null_and_empty_arrays, Some(true));
        }
        other => panic!("Expected Unwind stage, got {:?}", other),
    }

    // Check $project stage
    match &result.pipeline.stages[2] {
        Stage::Project(doc) => {
            assert!(doc.contains_key("col_0"));
            assert!(doc.contains_key("col_1"));
            assert!(doc.contains_key("col_2"));
            assert!(doc.contains_key("col_3"));
        }
        other => panic!("Expected Project stage, got {:?}", other),
    }

    // Check output columns
    assert_eq!(result.output_columns.len(), 4);
}

#[test]
fn builds_pipeline_for_inner_join() {
    let relation = Relation::Join {
        left: Box::new(Relation::From {
            collection: "orders".into(),
            columns: vec!["order_id".into(), "customer_id".into()],
            arguments: Default::default(),
        }),
        right: Box::new(Relation::From {
            collection: "customers".into(),
            columns: vec!["id".into(), "name".into()],
            arguments: Default::default(),
        }),
        on: vec![JoinOn {
            left: RelationalExpression::Column { index: 1 },
            right: RelationalExpression::Column { index: 0 },
        }],
        join_type: JoinType::Inner,
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "orders");
    assert_eq!(result.pipeline.stages.len(), 3); // $lookup, $unwind, $project

    // Check $unwind stage WITHOUT preserveNullAndEmptyArrays (filters non-matches)
    match &result.pipeline.stages[1] {
        Stage::Unwind {
            path,
            preserve_null_and_empty_arrays,
            ..
        } => {
            assert_eq!(path, "$_joined");
            assert_eq!(*preserve_null_and_empty_arrays, None);
        }
        other => panic!("Expected Unwind stage, got {:?}", other),
    }
}

#[test]
fn builds_pipeline_for_left_semi_join() {
    let relation = Relation::Join {
        left: Box::new(Relation::From {
            collection: "orders".into(),
            columns: vec!["order_id".into(), "customer_id".into()],
            arguments: Default::default(),
        }),
        right: Box::new(Relation::From {
            collection: "customers".into(),
            columns: vec!["id".into(), "name".into()],
            arguments: Default::default(),
        }),
        on: vec![JoinOn {
            left: RelationalExpression::Column { index: 1 },
            right: RelationalExpression::Column { index: 0 },
        }],
        join_type: JoinType::LeftSemi,
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "orders");
    assert_eq!(result.pipeline.stages.len(), 3); // $lookup, $match, $project

    // Check $match stage for non-empty _joined
    match &result.pipeline.stages[1] {
        Stage::Match(doc) => {
            assert_eq!(*doc, doc! { "_joined": { "$ne": [] } });
        }
        other => panic!("Expected Match stage, got {:?}", other),
    }

    // Check output columns - only left columns
    assert_eq!(result.output_columns.len(), 2);
}

#[test]
fn builds_pipeline_for_left_anti_join() {
    let relation = Relation::Join {
        left: Box::new(Relation::From {
            collection: "orders".into(),
            columns: vec!["order_id".into(), "customer_id".into()],
            arguments: Default::default(),
        }),
        right: Box::new(Relation::From {
            collection: "customers".into(),
            columns: vec!["id".into(), "name".into()],
            arguments: Default::default(),
        }),
        on: vec![JoinOn {
            left: RelationalExpression::Column { index: 1 },
            right: RelationalExpression::Column { index: 0 },
        }],
        join_type: JoinType::LeftAnti,
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "orders");
    assert_eq!(result.pipeline.stages.len(), 3); // $lookup, $match, $project

    // Check $match stage for empty _joined
    match &result.pipeline.stages[1] {
        Stage::Match(doc) => {
            assert_eq!(*doc, doc! { "_joined": { "$eq": [] } });
        }
        other => panic!("Expected Match stage, got {:?}", other),
    }

    // Check output columns - only left columns
    assert_eq!(result.output_columns.len(), 2);
}

#[test]
fn join_with_multiple_conditions() {
    let relation = Relation::Join {
        left: Box::new(Relation::From {
            collection: "orders".into(),
            columns: vec!["order_id".into(), "customer_id".into(), "region".into()],
            arguments: Default::default(),
        }),
        right: Box::new(Relation::From {
            collection: "customers".into(),
            columns: vec!["id".into(), "region".into(), "name".into()],
            arguments: Default::default(),
        }),
        on: vec![
            JoinOn {
                left: RelationalExpression::Column { index: 1 },
                right: RelationalExpression::Column { index: 0 },
            },
            JoinOn {
                left: RelationalExpression::Column { index: 2 },
                right: RelationalExpression::Column { index: 1 },
            },
        ],
        join_type: JoinType::Inner,
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "orders");

    // Check $lookup stage has multiple conditions
    match &result.pipeline.stages[0] {
        Stage::Lookup {
            r#let, pipeline, ..
        } => {
            let let_doc = r#let.as_ref().unwrap();
            assert!(let_doc.contains_key("left_0"));
            assert!(let_doc.contains_key("left_1"));

            // Check pipeline has $and condition
            let pipeline = pipeline.as_ref().unwrap();
            match &pipeline.stages[0] {
                Stage::Match(doc) => {
                    let expr = doc.get("$expr").unwrap();
                    if let Bson::Document(expr_doc) = expr {
                        assert!(expr_doc.contains_key("$and"));
                    } else {
                        panic!("Expected $expr to be a document");
                    }
                }
                other => panic!("Expected Match stage in pipeline, got {:?}", other),
            }
        }
        other => panic!("Expected Lookup stage, got {:?}", other),
    }

    // Output should have 6 columns (3 left + 3 right)
    assert_eq!(result.output_columns.len(), 6);
}

#[test]
fn join_rejects_unsupported_join_types() {
    // Full outer join is not supported
    let relation = Relation::Join {
        left: Box::new(Relation::From {
            collection: "orders".into(),
            columns: vec!["order_id".into()],
            arguments: Default::default(),
        }),
        right: Box::new(Relation::From {
            collection: "customers".into(),
            columns: vec!["id".into()],
            arguments: Default::default(),
        }),
        on: vec![JoinOn {
            left: RelationalExpression::Column { index: 0 },
            right: RelationalExpression::Column { index: 0 },
        }],
        join_type: JoinType::Full,
    };

    let result = build_relational_pipeline(&relation);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Unsupported join type"));
}

#[test]
fn join_with_filter_on_left() {
    let relation = Relation::Join {
        left: Box::new(Relation::Filter {
            input: Box::new(Relation::From {
                collection: "orders".into(),
                columns: vec!["order_id".into(), "customer_id".into(), "amount".into()],
                arguments: Default::default(),
            }),
            predicate: RelationalExpression::Gt {
                left: Box::new(RelationalExpression::Column { index: 2 }),
                right: Box::new(RelationalExpression::Literal {
                    literal: RelationalLiteral::Int64 { value: 100 },
                }),
            },
        }),
        right: Box::new(Relation::From {
            collection: "customers".into(),
            columns: vec!["id".into(), "name".into()],
            arguments: Default::default(),
        }),
        on: vec![JoinOn {
            left: RelationalExpression::Column { index: 1 },
            right: RelationalExpression::Column { index: 0 },
        }],
        join_type: JoinType::Left,
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "orders");
    // With early match optimization, we get 5 stages:
    // 1. Early $match (index-friendly)
    // 2. Regular $match (filter) with $expr
    // 3. $lookup
    // 4. $unwind
    // 5. $project
    assert_eq!(result.pipeline.stages.len(), 5);

    // First stage is the early match optimization (index-friendly)
    match &result.pipeline.stages[0] {
        Stage::Match(doc) => {
            assert!(doc.contains_key("amount"));
        }
        other => panic!("Expected early Match stage, got {:?}", other),
    }

    // Second stage should be the filter with query document syntax (not $expr)
    match &result.pipeline.stages[1] {
        Stage::Match(doc) => {
            // With Stage 4 optimization, we get query document syntax, not $expr
            assert!(
                doc.contains_key("amount"),
                "Expected query document with 'amount' field, got {:?}",
                doc
            );
        }
        other => panic!("Expected Match stage with query document, got {:?}", other),
    }
}

// ============================================================================
// Phase 5: Window Function Tests
// ============================================================================

#[test]
fn builds_pipeline_for_row_number() {
    let relation = Relation::Window {
        input: Box::new(Relation::From {
            collection: "products".into(),
            columns: vec!["name".into(), "category".into(), "price".into()],
            arguments: Default::default(),
        }),
        exprs: vec![RelationalExpression::RowNumber {
            partition_by: vec![RelationalExpression::Column { index: 1 }], // category
            order_by: vec![Sort {
                expr: RelationalExpression::Column { index: 2 }, // price
                direction: OrderDirection::Desc,
                nulls_sort: NullsSort::NullsLast,
            }],
        }],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "products");
    assert_eq!(result.pipeline.stages.len(), 2); // $setWindowFields, $project

    // Check $setWindowFields stage
    match &result.pipeline.stages[0] {
        Stage::Other(doc) => {
            assert!(doc.contains_key("$setWindowFields"));
            let swf = doc.get_document("$setWindowFields").unwrap();
            assert!(swf.contains_key("partitionBy"));
            assert!(swf.contains_key("sortBy"));
            assert!(swf.contains_key("output"));
        }
        other => panic!(
            "Expected Other stage with $setWindowFields, got {:?}",
            other
        ),
    }

    // Check $project stage
    match &result.pipeline.stages[1] {
        Stage::Project(doc) => {
            // Should have original columns + window output
            assert!(doc.contains_key("col_0"));
            assert!(doc.contains_key("col_1"));
            assert!(doc.contains_key("col_2"));
            assert!(doc.contains_key("col_3")); // window result
        }
        other => panic!("Expected Project stage, got {:?}", other),
    }

    // Output should have 4 columns (3 original + 1 window)
    assert_eq!(result.output_columns.len(), 4);
}

#[test]
fn builds_pipeline_for_rank() {
    let relation = Relation::Window {
        input: Box::new(Relation::From {
            collection: "scores".into(),
            columns: vec!["student".into(), "subject".into(), "score".into()],
            arguments: Default::default(),
        }),
        exprs: vec![RelationalExpression::Rank {
            partition_by: vec![RelationalExpression::Column { index: 1 }], // subject
            order_by: vec![Sort {
                expr: RelationalExpression::Column { index: 2 }, // score
                direction: OrderDirection::Desc,
                nulls_sort: NullsSort::NullsLast,
            }],
        }],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "scores");
    assert_eq!(result.pipeline.stages.len(), 2);

    // Check $setWindowFields contains $rank
    match &result.pipeline.stages[0] {
        Stage::Other(doc) => {
            let swf = doc.get_document("$setWindowFields").unwrap();
            let output = swf.get_document("output").unwrap();
            let w0 = output.get_document("_w0").unwrap();
            assert!(w0.contains_key("$rank"));
        }
        other => panic!(
            "Expected Other stage with $setWindowFields, got {:?}",
            other
        ),
    }
}

#[test]
fn builds_pipeline_for_dense_rank() {
    let relation = Relation::Window {
        input: Box::new(Relation::From {
            collection: "scores".into(),
            columns: vec!["student".into(), "score".into()],
            arguments: Default::default(),
        }),
        exprs: vec![RelationalExpression::DenseRank {
            partition_by: vec![], // No partition - rank across all rows
            order_by: vec![Sort {
                expr: RelationalExpression::Column { index: 1 }, // score
                direction: OrderDirection::Desc,
                nulls_sort: NullsSort::NullsLast,
            }],
        }],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    // Check $setWindowFields has null partitionBy
    match &result.pipeline.stages[0] {
        Stage::Other(doc) => {
            let swf = doc.get_document("$setWindowFields").unwrap();
            assert!(swf.get("partitionBy").unwrap().as_null().is_some());
            let output = swf.get_document("output").unwrap();
            let w0 = output.get_document("_w0").unwrap();
            assert!(w0.contains_key("$denseRank"));
        }
        other => panic!(
            "Expected Other stage with $setWindowFields, got {:?}",
            other
        ),
    }
}

#[test]
fn builds_pipeline_for_multiple_window_functions() {
    let relation = Relation::Window {
        input: Box::new(Relation::From {
            collection: "products".into(),
            columns: vec!["name".into(), "category".into(), "price".into()],
            arguments: Default::default(),
        }),
        exprs: vec![
            RelationalExpression::RowNumber {
                partition_by: vec![RelationalExpression::Column { index: 1 }],
                order_by: vec![Sort {
                    expr: RelationalExpression::Column { index: 2 },
                    direction: OrderDirection::Asc,
                    nulls_sort: NullsSort::NullsLast,
                }],
            },
            RelationalExpression::Rank {
                partition_by: vec![RelationalExpression::Column { index: 1 }],
                order_by: vec![Sort {
                    expr: RelationalExpression::Column { index: 2 },
                    direction: OrderDirection::Desc,
                    nulls_sort: NullsSort::NullsLast,
                }],
            },
        ],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    // Should have 2 $setWindowFields stages + 1 $project
    assert_eq!(result.pipeline.stages.len(), 3);

    // Output should have 5 columns (3 original + 2 window)
    assert_eq!(result.output_columns.len(), 5);
}

#[test]
fn builds_pipeline_for_window_with_multiple_partition_columns() {
    let relation = Relation::Window {
        input: Box::new(Relation::From {
            collection: "sales".into(),
            columns: vec![
                "region".into(),
                "category".into(),
                "product".into(),
                "amount".into(),
            ],
            arguments: Default::default(),
        }),
        exprs: vec![RelationalExpression::RowNumber {
            partition_by: vec![
                RelationalExpression::Column { index: 0 }, // region
                RelationalExpression::Column { index: 1 }, // category
            ],
            order_by: vec![Sort {
                expr: RelationalExpression::Column { index: 3 }, // amount
                direction: OrderDirection::Desc,
                nulls_sort: NullsSort::NullsLast,
            }],
        }],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    // Check partitionBy is a document with multiple fields
    match &result.pipeline.stages[0] {
        Stage::Other(doc) => {
            let swf = doc.get_document("$setWindowFields").unwrap();
            let partition_by = swf.get_document("partitionBy").unwrap();
            assert!(partition_by.contains_key("region"));
            assert!(partition_by.contains_key("category"));
        }
        other => panic!(
            "Expected Other stage with $setWindowFields, got {:?}",
            other
        ),
    }
}

#[test]
fn window_rejects_unsupported_functions() {
    let relation = Relation::Window {
        input: Box::new(Relation::From {
            collection: "data".into(),
            columns: vec!["value".into()],
            arguments: Default::default(),
        }),
        exprs: vec![RelationalExpression::NTile {
            partition_by: vec![],
            order_by: vec![Sort {
                expr: RelationalExpression::Column { index: 0 },
                direction: OrderDirection::Asc,
                nulls_sort: NullsSort::NullsLast,
            }],
            n: 4,
        }],
    };

    let result = build_relational_pipeline(&relation);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("NTile"));
}

#[test]
fn window_after_filter() {
    let relation = Relation::Window {
        input: Box::new(Relation::Filter {
            input: Box::new(Relation::From {
                collection: "products".into(),
                columns: vec!["name".into(), "category".into(), "price".into()],
                arguments: Default::default(),
            }),
            predicate: RelationalExpression::Gt {
                left: Box::new(RelationalExpression::Column { index: 2 }),
                right: Box::new(RelationalExpression::Literal {
                    literal: RelationalLiteral::Float64 {
                        value: Float64(100.0),
                    },
                }),
            },
        }),
        exprs: vec![RelationalExpression::RowNumber {
            partition_by: vec![RelationalExpression::Column { index: 1 }],
            order_by: vec![Sort {
                expr: RelationalExpression::Column { index: 2 },
                direction: OrderDirection::Desc,
                nulls_sort: NullsSort::NullsLast,
            }],
        }],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "products");
    // With early match optimization, we get 4 stages:
    // 1. Early $match (index-friendly)
    // 2. Regular $match (filter) with $expr
    // 3. $setWindowFields
    // 4. $project
    assert_eq!(result.pipeline.stages.len(), 4);

    // First stage is the early match optimization (index-friendly)
    match &result.pipeline.stages[0] {
        Stage::Match(doc) => {
            assert!(doc.contains_key("price"));
        }
        other => panic!("Expected early Match stage, got {:?}", other),
    }

    // Second stage should be the filter with query document syntax (not $expr)
    match &result.pipeline.stages[1] {
        Stage::Match(doc) => {
            // With Stage 4 optimization, we get query document syntax, not $expr
            assert!(
                doc.contains_key("price"),
                "Expected query document with 'price' field, got {:?}",
                doc
            );
        }
        other => panic!("Expected Match stage with query document, got {:?}", other),
    }
}

// =============================================================================
// Union Tests (Phase 6)
// =============================================================================

#[test]
fn builds_pipeline_for_union_two_relations() {
    let relation = Relation::Union {
        relations: vec![
            Relation::From {
                collection: "products".into(),
                columns: vec!["name".into(), "price".into()],
                arguments: Default::default(),
            },
            Relation::From {
                collection: "services".into(),
                columns: vec!["title".into(), "cost".into()],
                arguments: Default::default(),
            },
        ],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "products");
    assert_eq!(result.pipeline.stages.len(), 1);

    match &result.pipeline.stages[0] {
        Stage::UnionWith { coll, pipeline } => {
            assert_eq!(coll, "services");
            assert!(pipeline.is_some());
        }
        other => panic!("Expected UnionWith stage, got {:?}", other),
    }
}

#[test]
fn builds_pipeline_for_union_three_relations() {
    let relation = Relation::Union {
        relations: vec![
            Relation::From {
                collection: "collection_a".into(),
                columns: vec!["col1".into(), "col2".into()],
                arguments: Default::default(),
            },
            Relation::From {
                collection: "collection_b".into(),
                columns: vec!["col1".into(), "col2".into()],
                arguments: Default::default(),
            },
            Relation::From {
                collection: "collection_c".into(),
                columns: vec!["col1".into(), "col2".into()],
                arguments: Default::default(),
            },
        ],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "collection_a");
    assert_eq!(result.pipeline.stages.len(), 2); // Two $unionWith stages

    match &result.pipeline.stages[0] {
        Stage::UnionWith { coll, .. } => assert_eq!(coll, "collection_b"),
        other => panic!("Expected UnionWith stage for collection_b, got {:?}", other),
    }

    match &result.pipeline.stages[1] {
        Stage::UnionWith { coll, .. } => assert_eq!(coll, "collection_c"),
        other => panic!("Expected UnionWith stage for collection_c, got {:?}", other),
    }
}

#[test]
fn builds_pipeline_for_union_with_filter() {
    let relation = Relation::Union {
        relations: vec![
            Relation::Filter {
                input: Box::new(Relation::From {
                    collection: "active_users".into(),
                    columns: vec!["name".into(), "email".into()],
                    arguments: Default::default(),
                }),
                predicate: RelationalExpression::Eq {
                    left: Box::new(RelationalExpression::Column { index: 0 }),
                    right: Box::new(RelationalExpression::Literal {
                        literal: RelationalLiteral::String {
                            value: "Alice".into(),
                        },
                    }),
                },
            },
            Relation::Filter {
                input: Box::new(Relation::From {
                    collection: "inactive_users".into(),
                    columns: vec!["name".into(), "email".into()],
                    arguments: Default::default(),
                }),
                predicate: RelationalExpression::Eq {
                    left: Box::new(RelationalExpression::Column { index: 0 }),
                    right: Box::new(RelationalExpression::Literal {
                        literal: RelationalLiteral::String {
                            value: "Bob".into(),
                        },
                    }),
                },
            },
        ],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "active_users");
    assert_eq!(result.pipeline.stages.len(), 2); // $match + $unionWith

    // First stage should be the filter for first relation (query document syntax, not $expr)
    match &result.pipeline.stages[0] {
        Stage::Match(doc) => {
            // With Stage 4 optimization, we get query document syntax, not $expr
            // The filter is on column 0 which is "name"
            assert!(
                doc.contains_key("name"),
                "Expected query document with 'name' field, got {:?}",
                doc
            );
        }
        other => panic!("Expected Match stage with query document, got {:?}", other),
    }

    // Second stage should be unionWith
    match &result.pipeline.stages[1] {
        Stage::UnionWith { coll, pipeline } => {
            assert_eq!(coll, "inactive_users");
            assert!(pipeline.is_some());
            // The pipeline should contain a $match stage for the second relation's filter
            let inner_pipeline = pipeline.as_ref().unwrap();
            assert!(!inner_pipeline.is_empty());
        }
        other => panic!("Expected UnionWith stage, got {:?}", other),
    }
}

#[test]
fn union_fails_with_column_count_mismatch() {
    let relation = Relation::Union {
        relations: vec![
            Relation::From {
                collection: "products".into(),
                columns: vec!["name".into(), "price".into()],
                arguments: Default::default(),
            },
            Relation::From {
                collection: "services".into(),
                columns: vec!["title".into()], // Only 1 column vs 2
                arguments: Default::default(),
            },
        ],
    };

    let result = build_relational_pipeline(&relation);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("1 columns"),
        "Error should mention column count mismatch: {}",
        err
    );
}

#[test]
fn union_fails_with_empty_relations() {
    let relation = Relation::Union { relations: vec![] };

    let result = build_relational_pipeline(&relation);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("at least one relation"),
        "Error should mention empty relations: {}",
        err
    );
}

#[test]
fn builds_pipeline_for_union_single_relation() {
    // Edge case: union with only one relation
    let relation = Relation::Union {
        relations: vec![Relation::From {
            collection: "items".into(),
            columns: vec!["id".into(), "name".into()],
            arguments: Default::default(),
        }],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "items");
    assert!(result.pipeline.is_empty()); // No $unionWith stages needed
    assert_eq!(result.output_columns.len(), 2);
}

// =============================================================================
// Right Join Tests
// =============================================================================

#[test]
fn builds_pipeline_for_right_join() {
    // Right join: all rows from customers (right), with matching rows from orders (left)
    // RightJoin(orders, customers) -> LeftJoin(customers, orders) + reorder columns
    let relation = Relation::Join {
        left: Box::new(Relation::From {
            collection: "orders".into(),
            columns: vec!["order_id".into(), "customer_id".into()],
            arguments: Default::default(),
        }),
        right: Box::new(Relation::From {
            collection: "customers".into(),
            columns: vec!["id".into(), "name".into()],
            arguments: Default::default(),
        }),
        on: vec![JoinOn {
            left: RelationalExpression::Column { index: 1 }, // orders.customer_id
            right: RelationalExpression::Column { index: 0 }, // customers.id
        }],
        join_type: JoinType::Right,
    };

    let result = build_relational_pipeline(&relation).unwrap();

    // After transformation: starts from customers (the original right side)
    assert_eq!(result.collection, "customers");

    // Should have: $lookup, $unwind, $project (from left join), $project (reorder)
    assert_eq!(result.pipeline.stages.len(), 4);

    // Check $lookup stage - should look up into orders (original left)
    match &result.pipeline.stages[0] {
        Stage::Lookup { from, r#as, .. } => {
            assert_eq!(from.as_deref(), Some("orders"));
            assert_eq!(r#as, "_joined");
        }
        other => panic!("Expected Lookup stage, got {:?}", other),
    }

    // Check $unwind stage with preserveNullAndEmptyArrays (left join semantics)
    match &result.pipeline.stages[1] {
        Stage::Unwind {
            path,
            preserve_null_and_empty_arrays,
            ..
        } => {
            assert_eq!(path, "$_joined");
            assert_eq!(*preserve_null_and_empty_arrays, Some(true));
        }
        other => panic!("Expected Unwind stage, got {:?}", other),
    }

    // Check output columns - should be 4 (2 from orders + 2 from customers)
    assert_eq!(result.output_columns.len(), 4);
}

#[test]
fn builds_pipeline_for_right_semi_join() {
    // RightSemi: rows from customers that have a match in orders
    // RightSemi(orders, customers) -> LeftSemi(customers, orders)
    let relation = Relation::Join {
        left: Box::new(Relation::From {
            collection: "orders".into(),
            columns: vec!["order_id".into(), "customer_id".into()],
            arguments: Default::default(),
        }),
        right: Box::new(Relation::From {
            collection: "customers".into(),
            columns: vec!["id".into(), "name".into()],
            arguments: Default::default(),
        }),
        on: vec![JoinOn {
            left: RelationalExpression::Column { index: 1 }, // orders.customer_id
            right: RelationalExpression::Column { index: 0 }, // customers.id
        }],
        join_type: JoinType::RightSemi,
    };

    let result = build_relational_pipeline(&relation).unwrap();

    // After transformation: starts from customers (the original right side)
    assert_eq!(result.collection, "customers");

    // Should have: $lookup, $match (non-empty), $project
    assert_eq!(result.pipeline.stages.len(), 3);

    // Check $lookup stage - should look up into orders
    match &result.pipeline.stages[0] {
        Stage::Lookup { from, .. } => {
            assert_eq!(from.as_deref(), Some("orders"));
        }
        other => panic!("Expected Lookup stage, got {:?}", other),
    }

    // Check $match stage - should filter for non-empty _joined array
    match &result.pipeline.stages[1] {
        Stage::Match(doc) => {
            assert!(doc.contains_key("_joined"));
        }
        other => panic!("Expected Match stage, got {:?}", other),
    }

    // Output should only have 2 columns (customers columns only, since semi-join)
    assert_eq!(result.output_columns.len(), 2);
}

#[test]
fn builds_pipeline_for_right_anti_join() {
    // RightAnti: rows from customers that have NO match in orders
    // RightAnti(orders, customers) -> LeftAnti(customers, orders)
    let relation = Relation::Join {
        left: Box::new(Relation::From {
            collection: "orders".into(),
            columns: vec!["order_id".into(), "customer_id".into()],
            arguments: Default::default(),
        }),
        right: Box::new(Relation::From {
            collection: "customers".into(),
            columns: vec!["id".into(), "name".into()],
            arguments: Default::default(),
        }),
        on: vec![JoinOn {
            left: RelationalExpression::Column { index: 1 }, // orders.customer_id
            right: RelationalExpression::Column { index: 0 }, // customers.id
        }],
        join_type: JoinType::RightAnti,
    };

    let result = build_relational_pipeline(&relation).unwrap();

    // After transformation: starts from customers
    assert_eq!(result.collection, "customers");

    // Should have: $lookup, $match (empty), $project
    assert_eq!(result.pipeline.stages.len(), 3);

    // Check $lookup stage
    match &result.pipeline.stages[0] {
        Stage::Lookup { from, .. } => {
            assert_eq!(from.as_deref(), Some("orders"));
        }
        other => panic!("Expected Lookup stage, got {:?}", other),
    }

    // Check $match stage - should filter for empty _joined array
    match &result.pipeline.stages[1] {
        Stage::Match(doc) => {
            assert!(doc.contains_key("_joined"));
        }
        other => panic!("Expected Match stage, got {:?}", other),
    }

    // Output should only have 2 columns (customers columns only)
    assert_eq!(result.output_columns.len(), 2);
}

#[test]
fn right_join_with_complex_right_side() {
    // Right join where the right side has a filter applied
    // This is valid because after swap, the complex side becomes the left (outer) side
    let relation = Relation::Join {
        left: Box::new(Relation::From {
            collection: "orders".into(),
            columns: vec!["order_id".into(), "customer_id".into()],
            arguments: Default::default(),
        }),
        right: Box::new(Relation::Filter {
            input: Box::new(Relation::From {
                collection: "customers".into(),
                columns: vec!["id".into(), "name".into(), "status".into()],
                arguments: Default::default(),
            }),
            predicate: RelationalExpression::Eq {
                left: Box::new(RelationalExpression::Column { index: 2 }),
                right: Box::new(RelationalExpression::Literal {
                    literal: RelationalLiteral::String {
                        value: "active".into(),
                    },
                }),
            },
        }),
        on: vec![JoinOn {
            left: RelationalExpression::Column { index: 1 }, // orders.customer_id
            right: RelationalExpression::Column { index: 0 }, // customers.id
        }],
        join_type: JoinType::Right,
    };

    let result = build_relational_pipeline(&relation).unwrap();

    // After transformation: starts from customers (the original right side)
    assert_eq!(result.collection, "customers");

    // Output should have 5 columns (2 from orders + 3 from customers)
    assert_eq!(result.output_columns.len(), 5);
}

// Tests for string_agg and array_agg with distinct and order_by options

#[test]
fn builds_pipeline_for_string_agg_with_distinct() {
    // SELECT STRING_AGG(DISTINCT name, ',') FROM users
    let relation = Relation::Aggregate {
        input: Box::new(Relation::From {
            collection: "users".into(),
            columns: vec!["name".into()],
            arguments: Default::default(),
        }),
        group_by: vec![],
        aggregates: vec![RelationalExpression::StringAgg {
            expr: Box::new(RelationalExpression::Column { index: 0 }),
            separator: ",".to_string(),
            distinct: true,
            order_by: None,
        }],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    // Should use $addToSet instead of $push for distinct
    let mut expected_accumulators = BTreeMap::new();
    expected_accumulators.insert(
        "_a0".to_string(),
        Accumulator::AddToSet(Bson::String("$name".to_string())),
    );
    assert_eq!(
        result.pipeline.stages[0],
        Stage::Group {
            key_expression: Bson::Null,
            accumulators: expected_accumulators,
        }
    );

    // Post-processing should still use $reduce with the separator
    if let Stage::Project(project_doc) = &result.pipeline.stages[1] {
        let col_0 = project_doc.get("col_0").expect("col_0 should exist");
        // Check that it's a $reduce expression
        if let Bson::Document(reduce_doc) = col_0 {
            assert!(
                reduce_doc.contains_key("$reduce"),
                "Expected $reduce in projection"
            );
        }
    } else {
        panic!("Expected Project stage");
    }
}

#[test]
fn builds_pipeline_for_array_agg_with_order_by() {
    // SELECT ARRAY_AGG(name ORDER BY name ASC) FROM users
    let relation = Relation::Aggregate {
        input: Box::new(Relation::From {
            collection: "users".into(),
            columns: vec!["name".into()],
            arguments: Default::default(),
        }),
        group_by: vec![],
        aggregates: vec![RelationalExpression::ArrayAgg {
            expr: Box::new(RelationalExpression::Column { index: 0 }),
            distinct: false,
            order_by: Some(vec![Sort {
                expr: RelationalExpression::Column { index: 0 },
                direction: OrderDirection::Asc,
                nulls_sort: NullsSort::NullsLast,
            }]),
        }],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    // Should use $push to collect values
    let mut expected_accumulators = BTreeMap::new();
    expected_accumulators.insert(
        "_a0".to_string(),
        Accumulator::Push(Bson::String("$name".to_string())),
    );
    assert_eq!(
        result.pipeline.stages[0],
        Stage::Group {
            key_expression: Bson::Null,
            accumulators: expected_accumulators,
        }
    );

    // Post-processing should apply $sortArray
    if let Stage::Project(project_doc) = &result.pipeline.stages[1] {
        let col_0 = project_doc.get("col_0").expect("col_0 should exist");
        if let Bson::Document(sort_doc) = col_0 {
            assert!(
                sort_doc.contains_key("$sortArray"),
                "Expected $sortArray in projection"
            );
            // Verify ascending sort
            let sort_array = sort_doc.get_document("$sortArray").unwrap();
            assert_eq!(sort_array.get_i32("sortBy").unwrap(), 1);
        }
    } else {
        panic!("Expected Project stage");
    }
}

#[test]
fn builds_pipeline_for_array_agg_with_order_by_desc() {
    // SELECT ARRAY_AGG(name ORDER BY name DESC) FROM users
    let relation = Relation::Aggregate {
        input: Box::new(Relation::From {
            collection: "users".into(),
            columns: vec!["name".into()],
            arguments: Default::default(),
        }),
        group_by: vec![],
        aggregates: vec![RelationalExpression::ArrayAgg {
            expr: Box::new(RelationalExpression::Column { index: 0 }),
            distinct: false,
            order_by: Some(vec![Sort {
                expr: RelationalExpression::Column { index: 0 },
                direction: OrderDirection::Desc,
                nulls_sort: NullsSort::NullsLast,
            }]),
        }],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    // Post-processing should apply $sortArray with descending
    if let Stage::Project(project_doc) = &result.pipeline.stages[1] {
        let col_0 = project_doc.get("col_0").expect("col_0 should exist");
        if let Bson::Document(sort_doc) = col_0 {
            assert!(
                sort_doc.contains_key("$sortArray"),
                "Expected $sortArray in projection"
            );
            // Verify descending sort
            let sort_array = sort_doc.get_document("$sortArray").unwrap();
            assert_eq!(sort_array.get_i32("sortBy").unwrap(), -1);
        }
    } else {
        panic!("Expected Project stage");
    }
}

#[test]
fn builds_pipeline_for_string_agg_with_order_by() {
    // SELECT STRING_AGG(name, ',' ORDER BY name ASC) FROM users
    let relation = Relation::Aggregate {
        input: Box::new(Relation::From {
            collection: "users".into(),
            columns: vec!["name".into()],
            arguments: Default::default(),
        }),
        group_by: vec![],
        aggregates: vec![RelationalExpression::StringAgg {
            expr: Box::new(RelationalExpression::Column { index: 0 }),
            separator: ",".to_string(),
            distinct: false,
            order_by: Some(vec![Sort {
                expr: RelationalExpression::Column { index: 0 },
                direction: OrderDirection::Asc,
                nulls_sort: NullsSort::NullsLast,
            }]),
        }],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    // Should use $push to collect values
    let mut expected_accumulators = BTreeMap::new();
    expected_accumulators.insert(
        "_a0".to_string(),
        Accumulator::Push(Bson::String("$name".to_string())),
    );
    assert_eq!(
        result.pipeline.stages[0],
        Stage::Group {
            key_expression: Bson::Null,
            accumulators: expected_accumulators,
        }
    );

    // Post-processing should apply $sortArray inside $reduce
    if let Stage::Project(project_doc) = &result.pipeline.stages[1] {
        let col_0 = project_doc.get("col_0").expect("col_0 should exist");
        if let Bson::Document(reduce_doc) = col_0 {
            assert!(
                reduce_doc.contains_key("$reduce"),
                "Expected $reduce in projection"
            );
            // The input to $reduce should be a $sortArray
            let reduce = reduce_doc.get_document("$reduce").unwrap();
            let input = reduce.get("input").unwrap();
            if let Bson::Document(input_doc) = input {
                assert!(
                    input_doc.contains_key("$sortArray"),
                    "Expected $sortArray as input to $reduce"
                );
            }
        }
    } else {
        panic!("Expected Project stage");
    }
}

#[test]
fn builds_pipeline_for_string_agg_with_distinct_and_order_by() {
    // SELECT STRING_AGG(DISTINCT name, ',' ORDER BY name DESC) FROM users
    // Note: distinct uses $addToSet which doesn't preserve order,
    // but the $sortArray post-processing will sort the result
    let relation = Relation::Aggregate {
        input: Box::new(Relation::From {
            collection: "users".into(),
            columns: vec!["name".into()],
            arguments: Default::default(),
        }),
        group_by: vec![],
        aggregates: vec![RelationalExpression::StringAgg {
            expr: Box::new(RelationalExpression::Column { index: 0 }),
            separator: ",".to_string(),
            distinct: true,
            order_by: Some(vec![Sort {
                expr: RelationalExpression::Column { index: 0 },
                direction: OrderDirection::Desc,
                nulls_sort: NullsSort::NullsLast,
            }]),
        }],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    // Should use $addToSet for distinct
    let mut expected_accumulators = BTreeMap::new();
    expected_accumulators.insert(
        "_a0".to_string(),
        Accumulator::AddToSet(Bson::String("$name".to_string())),
    );
    assert_eq!(
        result.pipeline.stages[0],
        Stage::Group {
            key_expression: Bson::Null,
            accumulators: expected_accumulators,
        }
    );

    // Post-processing should have $sortArray inside $reduce
    if let Stage::Project(project_doc) = &result.pipeline.stages[1] {
        let col_0 = project_doc.get("col_0").expect("col_0 should exist");
        if let Bson::Document(reduce_doc) = col_0 {
            assert!(
                reduce_doc.contains_key("$reduce"),
                "Expected $reduce in projection"
            );
        }
    } else {
        panic!("Expected Project stage");
    }
}

// =============================================================================
// Integration Test: Original Failing Query Pattern
// =============================================================================
//
// This test verifies that the optimizations we implemented work together to
// produce index-friendly pipelines. The original issue was a query like:
//
//   Filter(Project(From(failsAggregate, [current_state.trade_date, ...]), [...]), trade_date >= "2026-01-07")
//   → Sort(trade_date DESC)
//
// Which produced:
//   { $match: { $expr: { $getField: { field: "current_state.trade_date", input: "$$ROOT" } ... } } }
//
// This prevented index usage because:
// 1. $expr blocks index usage
// 2. $getField forces expression context
// 3. Sort was on projected column names

#[test]
fn optimizations_work_together_for_filter_project_sort() {
    // Simulate the original failing query pattern:
    // From → Project → Filter → Sort
    //
    // With our optimizations:
    // 1. Predicate pushdown moves Filter below Project
    // 2. Sort pushdown moves Sort below Project
    // 3. Early match adds index-friendly $match
    // 4. Query documents generate index-friendly filter syntax

    let relation = Relation::Sort {
        input: Box::new(Relation::Filter {
            input: Box::new(Relation::Project {
                input: Box::new(Relation::From {
                    collection: "failsAggregate".into(),
                    columns: vec!["trade_date".into(), "trade_id".into(), "amount".into()],
                    arguments: Default::default(),
                }),
                exprs: vec![
                    RelationalExpression::Column { index: 0 }, // trade_date -> col_0
                    RelationalExpression::Column { index: 1 }, // trade_id -> col_1
                    RelationalExpression::Column { index: 2 }, // amount -> col_2
                ],
            }),
            predicate: RelationalExpression::GtEq {
                left: Box::new(RelationalExpression::Column { index: 0 }), // col_0 = trade_date
                right: Box::new(RelationalExpression::Literal {
                    literal: RelationalLiteral::String {
                        value: "2026-01-07".into(),
                    },
                }),
            },
        }),
        exprs: vec![Sort {
            expr: RelationalExpression::Column { index: 0 }, // col_0 = trade_date
            direction: OrderDirection::Desc,
            nulls_sort: NullsSort::NullsLast,
        }],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "failsAggregate");

    // With all optimizations applied, the pipeline should be:
    // 1. Early $match (index-friendly query document)
    // 2. Regular $match (query document, not $expr)
    // 3. $sort (on original field name because of sort pushdown)
    // 4. $project
    //
    // Note: After pushdown, the tree becomes:
    // Project(Sort(Filter(From, trade_date >= "2026-01-07"), trade_date DESC), [...])

    assert_eq!(
        result.pipeline.stages.len(),
        4,
        "Expected 4 stages: early match + match + sort + project"
    );

    // Stage 1: Early $match with index-friendly query document
    match &result.pipeline.stages[0] {
        Stage::Match(doc) => {
            assert!(
                doc.contains_key("trade_date"),
                "Early match should use original field name 'trade_date', got {:?}",
                doc
            );
            // Should NOT contain $expr
            assert!(
                !doc.contains_key("$expr"),
                "Early match should NOT use $expr, got {:?}",
                doc
            );
        }
        other => panic!("Expected Match stage, got {:?}", other),
    }

    // Stage 2: Regular $match (also query document, not $expr)
    match &result.pipeline.stages[1] {
        Stage::Match(doc) => {
            assert!(
                doc.contains_key("trade_date"),
                "Regular match should use original field name 'trade_date', got {:?}",
                doc
            );
            // Should NOT contain $expr thanks to Stage 4 optimization
            assert!(
                !doc.contains_key("$expr"),
                "Regular match should NOT use $expr, got {:?}",
                doc
            );
        }
        other => panic!("Expected Match stage, got {:?}", other),
    }

    // Stage 3: $sort on original field name (thanks to sort pushdown)
    match &result.pipeline.stages[2] {
        Stage::Sort(sort_doc) => {
            assert!(
                sort_doc.0.contains_key("trade_date"),
                "Sort should use original field name 'trade_date', got {:?}",
                sort_doc
            );
            // Should NOT contain col_0
            assert!(
                !sort_doc.0.contains_key("col_0"),
                "Sort should NOT use projected name 'col_0', got {:?}",
                sort_doc
            );
        }
        other => panic!("Expected Sort stage, got {:?}", other),
    }

    // Stage 4: $project
    match &result.pipeline.stages[3] {
        Stage::Project(doc) => {
            assert!(doc.contains_key("col_0"), "Project should produce col_0");
            assert!(doc.contains_key("col_1"), "Project should produce col_1");
            assert!(doc.contains_key("col_2"), "Project should produce col_2");
        }
        other => panic!("Expected Project stage, got {:?}", other),
    }
}

// =============================================================================
// Test for GetField expressions (nested field access)
// =============================================================================
//
// This test verifies that GetField expressions (accessing nested object fields)
// are properly optimized to use query documents instead of $expr.
//
// The original failing query had a predicate like:
//   GetField { column: Column { index: 1 }, field: "trade_date" } >= "2026-01-07"
// Where column 1 is "current_state" (an object).
//
// This should produce a query document like:
//   { "current_state.trade_date": { "$gte": "2026-01-07" } }
// Instead of:
//   { "$expr": { "$gte": [{ "$getField": { "field": "trade_date", "input": "$current_state" } }, ...] } }

#[test]
fn optimizes_get_field_filter_to_query_document() {
    // This mimics the original failing query pattern:
    // Filter(From(...), GetField(Column(1), "trade_date") >= "2026-01-07")
    let relation = Relation::Filter {
        input: Box::new(Relation::From {
            collection: "failsAggregate".into(),
            columns: vec![
                "aggregate_id".into(),
                "current_state".into(), // index 1 - an object with nested fields
                "last_update_time".into(),
            ],
            arguments: Default::default(),
        }),
        predicate: RelationalExpression::GtEq {
            left: Box::new(RelationalExpression::GetField {
                column: Box::new(RelationalExpression::Column { index: 1 }), // current_state
                field: "trade_date".to_string(),
            }),
            right: Box::new(RelationalExpression::Literal {
                literal: RelationalLiteral::String {
                    value: "2026-01-07".into(),
                },
            }),
        },
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "failsAggregate");

    // Should have 2 stages: early match + regular match
    // Both should use query documents, NOT $expr
    assert!(!result.pipeline.stages.is_empty(), "Expected at least 1 stage");

    // Check that the $match uses the nested field path, not $expr
    match &result.pipeline.stages[0] {
        Stage::Match(doc) => {
            // Should have "current_state.trade_date" as the key
            assert!(
                doc.contains_key("current_state.trade_date"),
                "Match should use nested field path 'current_state.trade_date', got {:?}",
                doc
            );
            // Should NOT use $expr
            assert!(
                !doc.contains_key("$expr"),
                "Match should NOT use $expr, got {:?}",
                doc
            );
        }
        other => panic!("Expected Match stage, got {:?}", other),
    }
}

#[test]
fn sorts_on_get_field_using_original_field_path() {
    // This is the critical optimization: when sorting on a GetField expression after projection,
    // the sort should use the original field path (e.g., "current_state.trade_date")
    // NOT a projected column name (e.g., "col_4")
    //
    // Original query pattern:
    //   Sort(Project(Project(Filter(From)))) where sort is on a projected column
    //   that was created from GetField(Column(1), "trade_date")
    //
    // The sort pushdown should move the Sort below the Project nodes so the sort
    // uses "current_state.trade_date" directly

    let relation = Relation::Sort {
        input: Box::new(Relation::Project {
            input: Box::new(Relation::From {
                collection: "failsAggregate".into(),
                columns: vec!["aggregate_id".into(), "current_state".into()],
                arguments: Default::default(),
            }),
            exprs: vec![
                // col_0 = aggregate_id
                RelationalExpression::Column { index: 0 },
                // col_1 = current_state.trade_date (GetField)
                RelationalExpression::GetField {
                    column: Box::new(RelationalExpression::Column { index: 1 }),
                    field: "trade_date".into(),
                },
            ],
        }),
        exprs: vec![Sort {
            expr: RelationalExpression::Column { index: 1 }, // References col_1 (the GetField result)
            direction: OrderDirection::Desc,
            nulls_sort: NullsSort::NullsLast,
        }],
    };

    let result = build_relational_pipeline(&relation).unwrap();

    assert_eq!(result.collection, "failsAggregate");

    // With sort pushdown optimization:
    // 1. Sort is pushed below Project
    // 2. Sort uses the original GetField expression, which becomes "current_state.trade_date"
    // Expected pipeline: $sort, $project
    assert_eq!(result.pipeline.stages.len(), 2);

    // First stage should be $sort on the original nested field path
    assert_eq!(
        result.pipeline.stages[0],
        Stage::Sort(SortDocument(doc! { "current_state.trade_date": -1 }))
    );

    // Second stage should be the project
    match &result.pipeline.stages[1] {
        Stage::Project(doc) => {
            assert!(doc.contains_key("col_0"));
            assert!(doc.contains_key("col_1"));
        }
        other => panic!("Expected Project stage, got {:?}", other),
    }
}
