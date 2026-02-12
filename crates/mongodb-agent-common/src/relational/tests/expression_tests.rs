//! Tests for expression translation.

use mongodb::bson::{bson, Bson};
use ndc_models::{Float64, RelationalExpression, RelationalLiteral};

use crate::relational::{
    expression::{translate_aggregate_expression, translate_expression, ExpressionContext},
    ColumnMapping,
};

fn make_context(columns: &[&str]) -> (ColumnMapping, ExpressionContext<'static>) {
    let mapping = ColumnMapping::new(columns.iter().copied());
    // SAFETY: We're leaking the mapping for test convenience
    let mapping: &'static ColumnMapping = Box::leak(Box::new(mapping));
    let ctx = ExpressionContext {
        column_mapping: mapping,
    };
    (mapping.clone(), ctx)
}

#[test]
fn translates_column_reference() {
    let (_, ctx) = make_context(&["name", "age", "email"]);
    let expr = RelationalExpression::Column { index: 1 };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, Bson::String("$age".to_string()));
}

#[test]
fn translates_literal_null() {
    let (_, ctx) = make_context(&[]);
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::Null,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, Bson::Null);
}

#[test]
fn translates_literal_boolean() {
    let (_, ctx) = make_context(&[]);
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::Boolean { value: true },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, Bson::Boolean(true));
}

#[test]
fn translates_literal_string() {
    let (_, ctx) = make_context(&[]);
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::String {
            value: "hello".to_string(),
        },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, Bson::String("hello".to_string()));
}

#[test]
fn translates_literal_int64() {
    let (_, ctx) = make_context(&[]);
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::Int64 { value: 42 },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, Bson::Int64(42));
}

#[test]
fn translates_literal_float64() {
    let (_, ctx) = make_context(&[]);
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::Float64 {
            value: Float64(3.14),
        },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, Bson::Double(3.14));
}

#[test]
fn translates_eq_comparison() {
    let (_, ctx) = make_context(&["age"]);
    let expr = RelationalExpression::Eq {
        left: Box::new(RelationalExpression::Column { index: 0 }),
        right: Box::new(RelationalExpression::Literal {
            literal: RelationalLiteral::Int64 { value: 25 },
        }),
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$eq": ["$age", 25_i64] }));
}

#[test]
fn translates_neq_comparison() {
    let (_, ctx) = make_context(&["status"]);
    let expr = RelationalExpression::NotEq {
        left: Box::new(RelationalExpression::Column { index: 0 }),
        right: Box::new(RelationalExpression::Literal {
            literal: RelationalLiteral::String {
                value: "inactive".to_string(),
            },
        }),
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$ne": ["$status", "inactive"] }));
}

#[test]
fn translates_lt_comparison() {
    let (_, ctx) = make_context(&["price"]);
    let expr = RelationalExpression::Lt {
        left: Box::new(RelationalExpression::Column { index: 0 }),
        right: Box::new(RelationalExpression::Literal {
            literal: RelationalLiteral::Int64 { value: 100 },
        }),
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$lt": ["$price", 100_i64] }));
}

#[test]
fn translates_and_logical_operator() {
    let (_, ctx) = make_context(&["age", "active"]);
    let expr = RelationalExpression::And {
        left: Box::new(RelationalExpression::Gt {
            left: Box::new(RelationalExpression::Column { index: 0 }),
            right: Box::new(RelationalExpression::Literal {
                literal: RelationalLiteral::Int64 { value: 18 },
            }),
        }),
        right: Box::new(RelationalExpression::Eq {
            left: Box::new(RelationalExpression::Column { index: 1 }),
            right: Box::new(RelationalExpression::Literal {
                literal: RelationalLiteral::Boolean { value: true },
            }),
        }),
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        bson!({
            "$and": [
                { "$gt": ["$age", 18_i64] },
                { "$eq": ["$active", true] }
            ]
        })
    );
}

#[test]
fn translates_is_null() {
    let (_, ctx) = make_context(&["email"]);
    let expr = RelationalExpression::IsNull {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$eq": ["$email", null] }));
}

// Phase 2 tests - Arithmetic expressions

#[test]
fn translates_plus() {
    let (_, ctx) = make_context(&["a", "b"]);
    let expr = RelationalExpression::Plus {
        left: Box::new(RelationalExpression::Column { index: 0 }),
        right: Box::new(RelationalExpression::Column { index: 1 }),
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$add": ["$a", "$b"] }));
}

#[test]
fn translates_negate() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::Negate {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$multiply": [-1, "$value"] }));
}

// Phase 2 tests - Math functions

#[test]
fn translates_abs() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::Abs {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$abs": "$value" }));
}

#[test]
fn translates_round_with_precision() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::Round {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        prec: Some(Box::new(RelationalExpression::Literal {
            literal: RelationalLiteral::Int32 { value: 2 },
        })),
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$round": ["$value", 2] }));
}

// Phase 2 tests - String functions

#[test]
fn translates_concat() {
    let (_, ctx) = make_context(&["first", "last"]);
    let expr = RelationalExpression::Concat {
        exprs: vec![
            RelationalExpression::Column { index: 0 },
            RelationalExpression::Literal {
                literal: RelationalLiteral::String {
                    value: " ".to_string(),
                },
            },
            RelationalExpression::Column { index: 1 },
        ],
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$concat": ["$first", " ", "$last"] }));
}

#[test]
fn translates_to_lower() {
    let (_, ctx) = make_context(&["name"]);
    let expr = RelationalExpression::ToLower {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$toLower": "$name" }));
}

// Phase 2 tests - Conditional functions

#[test]
fn translates_coalesce() {
    let (_, ctx) = make_context(&["a", "b"]);
    let expr = RelationalExpression::Coalesce {
        exprs: vec![
            RelationalExpression::Column { index: 0 },
            RelationalExpression::Column { index: 1 },
        ],
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        bson!({ "$ifNull": ["$a", { "$ifNull": ["$b", null] }] })
    );
}

#[test]
fn translates_case_without_scrutinee() {
    use ndc_models::CaseWhen;
    let (_, ctx) = make_context(&["status"]);
    let expr = RelationalExpression::Case {
        scrutinee: None,
        when: vec![CaseWhen {
            when: RelationalExpression::Eq {
                left: Box::new(RelationalExpression::Column { index: 0 }),
                right: Box::new(RelationalExpression::Literal {
                    literal: RelationalLiteral::String {
                        value: "active".to_string(),
                    },
                }),
            },
            then: RelationalExpression::Literal {
                literal: RelationalLiteral::Int32 { value: 1 },
            },
        }],
        default: Some(Box::new(RelationalExpression::Literal {
            literal: RelationalLiteral::Int32 { value: 0 },
        })),
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        bson!({
            "$switch": {
                "branches": [
                    {
                        "case": { "$eq": ["$status", "active"] },
                        "then": 1
                    }
                ],
                "default": 0
            }
        })
    );
}

// Phase 2 tests - Date/time functions

#[test]
fn translates_current_timestamp() {
    let (_, ctx) = make_context(&[]);
    let expr = RelationalExpression::CurrentTimestamp;
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, Bson::String("$$NOW".to_string()));
}

#[test]
fn translates_current_time() {
    let (_, ctx) = make_context(&[]);
    let expr = RelationalExpression::CurrentTime;
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        bson!({
            "$dateToString": {
                "format": "%H:%M:%S.%L",
                "date": "$$NOW"
            }
        })
    );
}

#[test]
fn translates_date_part_epoch() {
    let (_, ctx) = make_context(&["created_at"]);
    let expr = RelationalExpression::DatePart {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        part: ndc_models::DatePartUnit::Epoch,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    // Now handles both dates and durations (millis) with $cond
    assert_eq!(
        result,
        bson!({
            "$cond": {
                "if": { "$isNumber": "$created_at" },
                "then": { "$divide": ["$created_at", 1000_i64] },
                "else": { "$divide": [{ "$toLong": "$created_at" }, 1000_i64] }
            }
        })
    );
}

#[test]
fn translates_date_part_day_handles_duration_from_date_subtraction() {
    // When you subtract two dates, MongoDB returns milliseconds (Long).
    // DatePart should handle both Date inputs and duration (millis) inputs.
    let (_, ctx) = make_context(&["date1", "date2"]);
    let expr = RelationalExpression::DatePart {
        expr: Box::new(RelationalExpression::Minus {
            left: Box::new(RelationalExpression::Column { index: 0 }),
            right: Box::new(RelationalExpression::Column { index: 1 }),
        }),
        part: ndc_models::DatePartUnit::Day,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    // Should use $cond to detect if input is a number (duration in millis) or a date
    assert_eq!(
        result,
        bson!({
            "$cond": {
                "if": { "$isNumber": { "$subtract": ["$date1", "$date2"] } },
                "then": { "$trunc": { "$divide": [{ "$subtract": ["$date1", "$date2"] }, 86400000_i64] } },
                "else": { "$dayOfMonth": { "$subtract": ["$date1", "$date2"] } }
            }
        })
    );
}

// Phase 2 tests - JSON functions

#[test]
fn translates_get_field() {
    let (_, ctx) = make_context(&["doc"]);
    let expr = RelationalExpression::GetField {
        column: Box::new(RelationalExpression::Column { index: 0 }),
        field: "name".to_string(),
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        bson!({
            "$getField": {
                "field": "name",
                "input": "$doc"
            }
        })
    );
}

// Phase 2 tests - Additional expressions

#[test]
fn translates_is_distinct_from() {
    let (_, ctx) = make_context(&["a", "b"]);
    let expr = RelationalExpression::IsDistinctFrom {
        left: Box::new(RelationalExpression::Column { index: 0 }),
        right: Box::new(RelationalExpression::Column { index: 1 }),
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    // a IS DISTINCT FROM b = NOT ((a IS NULL AND b IS NULL) OR (a = b))
    assert_eq!(
        result,
        bson!({
            "$not": [{
                "$or": [
                    { "$and": [{ "$eq": ["$a", null] }, { "$eq": ["$b", null] }] },
                    { "$eq": ["$a", "$b"] }
                ]
            }]
        })
    );
}

#[test]
fn translates_is_not_distinct_from() {
    let (_, ctx) = make_context(&["a", "b"]);
    let expr = RelationalExpression::IsNotDistinctFrom {
        left: Box::new(RelationalExpression::Column { index: 0 }),
        right: Box::new(RelationalExpression::Column { index: 1 }),
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    // a IS NOT DISTINCT FROM b = (a IS NULL AND b IS NULL) OR (a = b)
    assert_eq!(
        result,
        bson!({
            "$or": [
                { "$and": [{ "$eq": ["$a", null] }, { "$eq": ["$b", null] }] },
                { "$eq": ["$a", "$b"] }
            ]
        })
    );
}

#[test]
fn translates_reverse() {
    let (_, ctx) = make_context(&["text"]);
    let expr = RelationalExpression::Reverse {
        str: Box::new(RelationalExpression::Column { index: 0 }),
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    // Verify it uses $reduce with $reverseArray and $map to split/join chars
    assert!(result.as_document().unwrap().contains_key("$reduce"));
}

#[test]
fn translates_lpad() {
    let (_, ctx) = make_context(&["text"]);
    let expr = RelationalExpression::LPad {
        str: Box::new(RelationalExpression::Column { index: 0 }),
        n: Box::new(RelationalExpression::Literal {
            literal: RelationalLiteral::Int32 { value: 10 },
        }),
        padding_str: None, // Use default space
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    // Verify it uses $let with padding logic
    assert!(result.as_document().unwrap().contains_key("$let"));
}

#[test]
fn translates_rpad() {
    let (_, ctx) = make_context(&["text"]);
    let expr = RelationalExpression::RPad {
        str: Box::new(RelationalExpression::Column { index: 0 }),
        n: Box::new(RelationalExpression::Literal {
            literal: RelationalLiteral::Int32 { value: 10 },
        }),
        padding_str: Some(Box::new(RelationalExpression::Literal {
            literal: RelationalLiteral::String {
                value: "-".to_string(),
            },
        })),
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    // Verify it uses $let with padding logic
    assert!(result.as_document().unwrap().contains_key("$let"));
}

#[test]
fn translates_substr_index() {
    let (_, ctx) = make_context(&["text"]);
    let expr = RelationalExpression::SubstrIndex {
        str: Box::new(RelationalExpression::Column { index: 0 }),
        delim: Box::new(RelationalExpression::Literal {
            literal: RelationalLiteral::String {
                value: ",".to_string(),
            },
        }),
        count: Box::new(RelationalExpression::Literal {
            literal: RelationalLiteral::Int32 { value: 2 },
        }),
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    // Verify it uses $let with $split logic
    assert!(result.as_document().unwrap().contains_key("$let"));
}

// Phase 3 tests - Aggregate expression translation

#[test]
fn translates_count_aggregate() {
    let (_, ctx) = make_context(&["id"]);
    let expr = RelationalExpression::Count {
        expr: Box::new(RelationalExpression::Literal {
            literal: RelationalLiteral::Null,
        }),
        distinct: false,
    };
    let result = translate_aggregate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$sum": 1 }));
}

#[test]
fn translates_count_distinct_aggregate() {
    let (_, ctx) = make_context(&["name"]);
    let expr = RelationalExpression::Count {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        distinct: true,
    };
    let result = translate_aggregate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$addToSet": "$name" }));
}

#[test]
fn translates_sum_aggregate() {
    let (_, ctx) = make_context(&["amount"]);
    let expr = RelationalExpression::Sum {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
    };
    let result = translate_aggregate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$sum": "$amount" }));
}

#[test]
fn translates_average_aggregate() {
    let (_, ctx) = make_context(&["price"]);
    let expr = RelationalExpression::Average {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
    };
    let result = translate_aggregate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$avg": "$price" }));
}

#[test]
fn translates_min_aggregate() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::Min {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
    };
    let result = translate_aggregate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$min": "$value" }));
}

#[test]
fn translates_max_aggregate() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::Max {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
    };
    let result = translate_aggregate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$max": "$value" }));
}

#[test]
fn translates_first_value_aggregate() {
    let (_, ctx) = make_context(&["name"]);
    let expr = RelationalExpression::FirstValue {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
    };
    let result = translate_aggregate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$first": "$name" }));
}

#[test]
fn translates_last_value_aggregate() {
    let (_, ctx) = make_context(&["name"]);
    let expr = RelationalExpression::LastValue {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
    };
    let result = translate_aggregate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$last": "$name" }));
}

#[test]
fn translates_stddev_aggregate() {
    let (_, ctx) = make_context(&["score"]);
    let expr = RelationalExpression::Stddev {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
    };
    let result = translate_aggregate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$stdDevSamp": "$score" }));
}

#[test]
fn translates_stddev_pop_aggregate() {
    let (_, ctx) = make_context(&["score"]);
    let expr = RelationalExpression::StddevPop {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
    };
    let result = translate_aggregate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$stdDevPop": "$score" }));
}

#[test]
fn translates_median_aggregate() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::Median {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
    };
    let result = translate_aggregate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        bson!({ "$median": { "input": "$value", "method": "approximate" } })
    );
}

#[test]
fn translates_approx_percentile_cont_aggregate() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::ApproxPercentileCont {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        percentile: Float64(0.95),
    };
    let result = translate_aggregate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        bson!({ "$percentile": { "input": "$value", "p": [0.95], "method": "approximate" } })
    );
}

#[test]
fn translates_array_agg_aggregate() {
    let (_, ctx) = make_context(&["name"]);
    let expr = RelationalExpression::ArrayAgg {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        distinct: false,
        order_by: None,
    };
    let result = translate_aggregate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$push": "$name" }));
}

#[test]
fn translates_array_agg_distinct_aggregate() {
    let (_, ctx) = make_context(&["name"]);
    let expr = RelationalExpression::ArrayAgg {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        distinct: true,
        order_by: None,
    };
    let result = translate_aggregate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$addToSet": "$name" }));
}

#[test]
fn translates_bool_and_aggregate() {
    let (_, ctx) = make_context(&["active"]);
    let expr = RelationalExpression::BoolAnd {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
    };
    let result = translate_aggregate_expression(&expr, &ctx).unwrap();
    // BoolAnd uses $push, post-processing with $allElementsTrue is done in pipeline_builder
    assert_eq!(result, bson!({ "$push": "$active" }));
}

#[test]
fn translates_bool_or_aggregate() {
    let (_, ctx) = make_context(&["active"]);
    let expr = RelationalExpression::BoolOr {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
    };
    let result = translate_aggregate_expression(&expr, &ctx).unwrap();
    // BoolOr uses $push, post-processing with $anyElementTrue is done in pipeline_builder
    assert_eq!(result, bson!({ "$push": "$active" }));
}

#[test]
fn translates_string_agg_aggregate() {
    let (_, ctx) = make_context(&["name"]);
    let expr = RelationalExpression::StringAgg {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        separator: ", ".to_string(),
        distinct: false,
        order_by: None,
    };
    let result = translate_aggregate_expression(&expr, &ctx).unwrap();
    // StringAgg uses $push, post-processing with $reduce + $concat is done in pipeline_builder
    assert_eq!(result, bson!({ "$push": "$name" }));
}

#[test]
fn rejects_var_aggregate() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::Var {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
    };
    let result = translate_aggregate_expression(&expr, &ctx);
    assert!(result.is_err());
}

#[test]
fn rejects_approx_distinct_aggregate() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::ApproxDistinct {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
    };
    let result = translate_aggregate_expression(&expr, &ctx);
    assert!(result.is_err());
}

#[test]
fn rejects_non_aggregate_expression() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::Column { index: 0 };
    let result = translate_aggregate_expression(&expr, &ctx);
    assert!(result.is_err());
}

// ============================================================================
// Date/Time/Decimal Literal Tests
// ============================================================================

#[test]
fn translates_literal_date32() {
    let (_, ctx) = make_context(&[]);
    // Date32: days since epoch. 20473 days = 2026-01-20
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::Date32 { value: 20473 },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    // 20473 days * 86400000 ms/day = 1768867200000 ms
    let expected_millis = 20473_i64 * 86_400_000;
    assert_eq!(
        result,
        Bson::DateTime(mongodb::bson::DateTime::from_millis(expected_millis))
    );
}

#[test]
fn translates_literal_date64() {
    let (_, ctx) = make_context(&[]);
    // Date64: milliseconds since epoch
    let millis = 1768867200000_i64; // 2026-01-20 00:00:00 UTC
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::Date64 { value: millis },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        Bson::DateTime(mongodb::bson::DateTime::from_millis(millis))
    );
}

#[test]
fn translates_literal_time32_second() {
    let (_, ctx) = make_context(&[]);
    // Time32Second: seconds since midnight. 3661 = 01:01:01
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::Time32Second { value: 3661 },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, Bson::Int64(3661 * 1000)); // milliseconds
}

#[test]
fn translates_literal_time32_millisecond() {
    let (_, ctx) = make_context(&[]);
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::Time32Millisecond { value: 3661000 },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, Bson::Int64(3661000));
}

#[test]
fn translates_literal_time64_microsecond() {
    let (_, ctx) = make_context(&[]);
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::Time64Microsecond { value: 3661000000 },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, Bson::Int64(3661000)); // microseconds -> milliseconds
}

#[test]
fn translates_literal_time64_nanosecond() {
    let (_, ctx) = make_context(&[]);
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::Time64Nanosecond {
            value: 3661000000000,
        },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, Bson::Int64(3661000)); // nanoseconds -> milliseconds
}

#[test]
fn translates_literal_timestamp_second() {
    let (_, ctx) = make_context(&[]);
    let seconds = 1768867200_i64; // 2026-01-20 00:00:00 UTC
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::TimestampSecond { value: seconds },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        Bson::DateTime(mongodb::bson::DateTime::from_millis(seconds * 1000))
    );
}

#[test]
fn translates_literal_timestamp_millisecond() {
    let (_, ctx) = make_context(&[]);
    let millis = 1768867200000_i64;
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::TimestampMillisecond { value: millis },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        Bson::DateTime(mongodb::bson::DateTime::from_millis(millis))
    );
}

#[test]
fn translates_literal_timestamp_microsecond() {
    let (_, ctx) = make_context(&[]);
    let micros = 1768867200000000_i64;
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::TimestampMicrosecond { value: micros },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        Bson::DateTime(mongodb::bson::DateTime::from_millis(micros / 1000))
    );
}

#[test]
fn translates_literal_timestamp_nanosecond() {
    let (_, ctx) = make_context(&[]);
    let nanos = 1768867200000000000_i64;
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::TimestampNanosecond { value: nanos },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        Bson::DateTime(mongodb::bson::DateTime::from_millis(nanos / 1_000_000))
    );
}

#[test]
fn translates_literal_duration_second() {
    let (_, ctx) = make_context(&[]);
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::DurationSecond { value: 3600 },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, Bson::Int64(3600 * 1000)); // milliseconds
}

#[test]
fn translates_literal_duration_millisecond() {
    let (_, ctx) = make_context(&[]);
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::DurationMillisecond { value: 3600000 },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, Bson::Int64(3600000));
}

#[test]
fn translates_literal_duration_microsecond() {
    let (_, ctx) = make_context(&[]);
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::DurationMicrosecond { value: 3600000000 },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, Bson::Int64(3600000)); // microseconds -> milliseconds
}

#[test]
fn translates_literal_duration_nanosecond() {
    let (_, ctx) = make_context(&[]);
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::DurationNanosecond {
            value: 3600000000000,
        },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, Bson::Int64(3600000)); // nanoseconds -> milliseconds
}

#[test]
fn translates_literal_interval() {
    let (_, ctx) = make_context(&[]);
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::Interval {
            months: 1,
            days: 15,
            nanoseconds: 3600000000000, // 1 hour in nanoseconds
        },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        bson!({
            "months": 1,
            "days": 15,
            "millis": 3600000_i64 // 1 hour in milliseconds
        })
    );
}

#[test]
fn translates_literal_decimal128() {
    let (_, ctx) = make_context(&[]);
    // 12345 with scale 2 = 123.45
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::Decimal128 {
            value: 12345,
            scale: 2,
            prec: 5,
        },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    match result {
        Bson::Decimal128(d) => {
            assert_eq!(d.to_string(), "123.45");
        }
        _ => panic!("Expected Decimal128"),
    }
}

#[test]
fn translates_literal_decimal128_with_leading_zeros() {
    let (_, ctx) = make_context(&[]);
    // 123 with scale 5 = 0.00123
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::Decimal128 {
            value: 123,
            scale: 5,
            prec: 5,
        },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    match result {
        Bson::Decimal128(d) => {
            assert_eq!(d.to_string(), "0.00123");
        }
        _ => panic!("Expected Decimal128"),
    }
}

#[test]
fn translates_literal_decimal128_negative() {
    let (_, ctx) = make_context(&[]);
    // -12345 with scale 2 = -123.45
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::Decimal128 {
            value: -12345,
            scale: 2,
            prec: 5,
        },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    match result {
        Bson::Decimal128(d) => {
            assert_eq!(d.to_string(), "-123.45");
        }
        _ => panic!("Expected Decimal128"),
    }
}

#[test]
fn translates_literal_decimal256() {
    let (_, ctx) = make_context(&[]);
    // "12345" with scale 2 = 123.45
    let expr = RelationalExpression::Literal {
        literal: RelationalLiteral::Decimal256 {
            value: "12345".to_string(),
            scale: 2,
            prec: 5,
        },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    match result {
        Bson::Decimal128(d) => {
            assert_eq!(d.to_string(), "123.45");
        }
        _ => panic!("Expected Decimal128"),
    }
}

// Cast expression tests

#[test]
fn translates_cast_to_boolean() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::Cast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Boolean,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$toBool": "$value" }));
}

#[test]
fn translates_cast_to_string() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::Cast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Utf8,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        bson!({
            "$cond": {
                "if": {
                    "$or": [
                        { "$isArray": "$value" },
                        { "$eq": [{ "$type": "$value" }, "object"] }
                    ]
                },
                "then": "$value",
                "else": { "$toString": "$value" }
            }
        })
    );
}

#[test]
fn translates_cast_to_int32() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::Cast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Int32,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$toInt": "$value" }));
}

#[test]
fn translates_cast_to_int64() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::Cast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Int64,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$toLong": "$value" }));
}

#[test]
fn translates_cast_to_double() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::Cast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Float64,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$toDouble": "$value" }));
}

#[test]
fn translates_cast_to_decimal() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::Cast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Decimal128 { scale: 2, prec: 10 },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$toDecimal": "$value" }));
}

#[test]
fn translates_cast_to_date() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::Cast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Date,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$toDate": "$value" }));
}

#[test]
fn translates_cast_to_timestamp() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::Cast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Timestamp,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(result, bson!({ "$toDate": "$value" }));
}

#[test]
fn translates_cast_to_time() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::Cast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Time,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    // Time is extracted from Date as milliseconds since midnight
    assert_eq!(
        result,
        bson!({
            "$add": [
                { "$multiply": [{ "$hour": "$value" }, 3_600_000] },
                { "$multiply": [{ "$minute": "$value" }, 60_000] },
                { "$multiply": [{ "$second": "$value" }, 1_000] },
                { "$millisecond": "$value" }
            ]
        })
    );
}

#[test]
fn translates_cast_to_duration() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::Cast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Duration,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    // Duration is converted to long (milliseconds)
    assert_eq!(result, bson!({ "$toLong": "$value" }));
}

#[test]
fn translates_cast_to_interval() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::Cast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Interval,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    // Interval is a document with months, days, and millis
    assert_eq!(
        result,
        bson!({
            "months": 0,
            "days": 0,
            "millis": { "$toLong": "$value" }
        })
    );
}

// TryCast expression tests

#[test]
fn translates_try_cast_to_boolean() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::TryCast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Boolean,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        bson!({
            "$convert": {
                "input": "$value",
                "to": "bool",
                "onError": null,
                "onNull": null
            }
        })
    );
}

#[test]
fn translates_try_cast_to_string() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::TryCast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Utf8,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        bson!({
            "$cond": {
                "if": {
                    "$or": [
                        { "$isArray": "$value" },
                        { "$eq": [{ "$type": "$value" }, "object"] }
                    ]
                },
                "then": "$value",
                "else": {
                    "$convert": {
                        "input": "$value",
                        "to": "string",
                        "onError": null,
                        "onNull": null
                    }
                }
            }
        })
    );
}

#[test]
fn translates_try_cast_to_int() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::TryCast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Int32,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        bson!({
            "$convert": {
                "input": "$value",
                "to": "int",
                "onError": null,
                "onNull": null
            }
        })
    );
}

#[test]
fn translates_try_cast_to_long() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::TryCast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Int64,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        bson!({
            "$convert": {
                "input": "$value",
                "to": "long",
                "onError": null,
                "onNull": null
            }
        })
    );
}

#[test]
fn translates_try_cast_to_double() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::TryCast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Float64,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        bson!({
            "$convert": {
                "input": "$value",
                "to": "double",
                "onError": null,
                "onNull": null
            }
        })
    );
}

#[test]
fn translates_try_cast_to_decimal() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::TryCast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Decimal128 { scale: 2, prec: 10 },
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        bson!({
            "$convert": {
                "input": "$value",
                "to": "decimal",
                "onError": null,
                "onNull": null
            }
        })
    );
}

#[test]
fn translates_try_cast_to_date() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::TryCast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Date,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        bson!({
            "$convert": {
                "input": "$value",
                "to": "date",
                "onError": null,
                "onNull": null
            }
        })
    );
}

#[test]
fn translates_try_cast_to_time() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::TryCast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Time,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    // TryCast to Time returns null on error, extracts time-of-day on success
    let date_expr = bson!({
        "$convert": {
            "input": "$value",
            "to": "date",
            "onError": null,
            "onNull": null
        }
    });
    assert_eq!(
        result,
        bson!({
            "$cond": {
                "if": { "$eq": [date_expr.clone(), null] },
                "then": null,
                "else": {
                    "$add": [
                        { "$multiply": [{ "$hour": date_expr.clone() }, 3_600_000] },
                        { "$multiply": [{ "$minute": date_expr.clone() }, 60_000] },
                        { "$multiply": [{ "$second": date_expr.clone() }, 1_000] },
                        { "$millisecond": date_expr }
                    ]
                }
            }
        })
    );
}

#[test]
fn translates_try_cast_to_duration() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::TryCast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Duration,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    assert_eq!(
        result,
        bson!({
            "$convert": {
                "input": "$value",
                "to": "long",
                "onError": null,
                "onNull": null
            }
        })
    );
}

#[test]
fn translates_try_cast_to_interval() {
    let (_, ctx) = make_context(&["value"]);
    let expr = RelationalExpression::TryCast {
        expr: Box::new(RelationalExpression::Column { index: 0 }),
        from_type: None,
        as_type: ndc_models::CastType::Interval,
    };
    let result = translate_expression(&expr, &ctx).unwrap();
    let millis_expr = bson!({
        "$convert": {
            "input": "$value",
            "to": "long",
            "onError": null,
            "onNull": null
        }
    });
    assert_eq!(
        result,
        bson!({
            "$cond": {
                "if": { "$eq": [millis_expr.clone(), null] },
                "then": null,
                "else": {
                    "months": 0,
                    "days": 0,
                    "millis": millis_expr
                }
            }
        })
    );
}
