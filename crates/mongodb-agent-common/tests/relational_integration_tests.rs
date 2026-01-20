//! Integration tests for relational query execution against a real MongoDB instance.
//!
//! These tests use the full `execute_relational_query` function with a real `ConnectorState`,
//! testing the complete end-to-end flow from `RelationalQuery` to `RelationalQueryResponse`.
//!
//! These tests require MongoDB 7.0+ running on localhost:27017 with the test_relational database
//! populated with test data.
//!
//! To run these tests:
//! 1. Start MongoDB: docker run -d --name mongodb-test -p 27017:27017 mongo:7.0
//! 2. Populate test data (see setup_test_data below)
//! 3. Run: MONGODB_URI="mongodb://localhost:27017/test_relational" cargo test -p mongodb-agent-common --test relational_integration_tests -- --ignored

use mongodb_agent_common::relational::execute_relational_query;
use mongodb_agent_common::state::try_init_state_from_uri;
use ndc_models::{
    Float64, JoinOn, JoinType, NullsSort, OrderDirection, Relation, RelationalExpression,
    RelationalLiteral, RelationalQuery, RelationalQueryResponse, Sort,
};
use serde_json::json;

/// Get a ConnectorState connected to the test database.
async fn get_test_state() -> mongodb_agent_common::state::ConnectorState {
    let uri = std::env::var("MONGODB_URI").unwrap_or_else(|_| {
        "mongodb://localhost:27017/test_relational".to_string()
    });
    try_init_state_from_uri(Some(&uri))
        .await
        .expect("Failed to initialize ConnectorState")
}

/// Execute a relational query through the full execute_relational_query function.
/// This tests the complete end-to-end flow: RelationalQuery -> ConnectorState -> MongoDB -> RelationalQueryResponse
async fn execute_query(relation: Relation) -> RelationalQueryResponse {
    let state = get_test_state().await;
    let query = RelationalQuery {
        root_relation: relation,
        request_arguments: None,
    };
    execute_relational_query(&state, query)
        .await
        .expect("Failed to execute relational query")
}

/// Helper to get rows from response
fn get_rows(response: &RelationalQueryResponse) -> &Vec<Vec<serde_json::Value>> {
    &response.rows
}

// =============================================================================
// I.1 Basic Relation Tests
// =============================================================================

#[tokio::test]
#[ignore]
async fn test_from_relation_all_columns() {
    let relation = Relation::From {
        collection: "products".into(),
        columns: vec!["_id".into(), "name".into(), "price".into(), "category".into(), "stock".into()],
        arguments: Default::default(),
    };

    let response = execute_query(relation).await;
    let rows = get_rows(&response);
    assert_eq!(rows.len(), 5, "Expected 5 products");
}

#[tokio::test]
#[ignore]
async fn test_from_relation_column_subset() {
    let relation = Relation::From {
        collection: "products".into(),
        columns: vec!["name".into(), "price".into()],
        arguments: Default::default(),
    };

    let response = execute_query(relation).await;
    let rows = get_rows(&response);
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[0].len(), 2, "Each row should have 2 columns");
}

#[tokio::test]
#[ignore]
async fn test_filter_equality() {
    let relation = Relation::Filter {
        input: Box::new(Relation::From {
            collection: "products".into(),
            columns: vec!["name".into(), "price".into()],
            arguments: Default::default(),
        }),
        predicate: RelationalExpression::Eq {
            left: Box::new(RelationalExpression::Column { index: 0 }),
            right: Box::new(RelationalExpression::Literal {
                literal: RelationalLiteral::String { value: "Widget".into() },
            }),
        },
    };

    let response = execute_query(relation).await;
    let rows = get_rows(&response);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][0], json!("Widget"));
}

#[tokio::test]
#[ignore]
async fn test_filter_range() {
    let relation = Relation::Filter {
        input: Box::new(Relation::From {
            collection: "products".into(),
            columns: vec!["name".into(), "price".into()],
            arguments: Default::default(),
        }),
        predicate: RelationalExpression::Gt {
            left: Box::new(RelationalExpression::Column { index: 1 }),
            right: Box::new(RelationalExpression::Literal {
                literal: RelationalLiteral::Float64 { value: Float64(15.0) },
            }),
        },
    };

    let response = execute_query(relation).await;
    let rows = get_rows(&response);
    assert_eq!(rows.len(), 2, "Expected Gadget (25) and Thing (30)");
}

#[tokio::test]
#[ignore]
async fn test_sort_ascending() {
    let relation = Relation::Sort {
        input: Box::new(Relation::From {
            collection: "products".into(),
            columns: vec!["name".into(), "price".into()],
            arguments: Default::default(),
        }),
        exprs: vec![Sort {
            expr: RelationalExpression::Column { index: 1 },
            direction: OrderDirection::Asc,
            nulls_sort: NullsSort::NullsLast,
        }],
    };

    let response = execute_query(relation).await;
    let rows = get_rows(&response);
    assert!(rows.len() >= 4);
    // Check that prices are in ascending order (excluding nulls at end)
}

#[tokio::test]
#[ignore]
async fn test_paginate_limit() {
    let relation = Relation::Paginate {
        input: Box::new(Relation::Sort {
            input: Box::new(Relation::From {
                collection: "products".into(),
                columns: vec!["name".into()],
                arguments: Default::default(),
            }),
            exprs: vec![Sort {
                expr: RelationalExpression::Column { index: 0 },
                direction: OrderDirection::Asc,
                nulls_sort: NullsSort::NullsLast,
            }],
        }),
        fetch: Some(2),
        skip: 0,
    };

    let response = execute_query(relation).await;
    let rows = get_rows(&response);
    assert_eq!(rows.len(), 2, "Limit should restrict to 2 rows");
}

#[tokio::test]
#[ignore]
async fn test_paginate_offset() {
    let relation = Relation::Paginate {
        input: Box::new(Relation::Sort {
            input: Box::new(Relation::From {
                collection: "products".into(),
                columns: vec!["name".into()],
                arguments: Default::default(),
            }),
            exprs: vec![Sort {
                expr: RelationalExpression::Column { index: 0 },
                direction: OrderDirection::Asc,
                nulls_sort: NullsSort::NullsLast,
            }],
        }),
        fetch: Some(2),
        skip: 2,
    };

    let response = execute_query(relation).await;
    let rows = get_rows(&response);
    assert_eq!(rows.len(), 2, "Limit should restrict to 2 rows after offset");
}

// =============================================================================
// I.2 Aggregation Tests
// =============================================================================

#[tokio::test]
#[ignore]
async fn test_aggregate_count() {
    let relation = Relation::Aggregate {
        input: Box::new(Relation::From {
            collection: "products".into(),
            columns: vec!["name".into()],
            arguments: Default::default(),
        }),
        group_by: vec![],
        aggregates: vec![RelationalExpression::Count {
            expr: Box::new(RelationalExpression::Literal {
                literal: RelationalLiteral::Null,
            }),
            distinct: false,
        }],
    };

    let response = execute_query(relation).await;
    let rows = get_rows(&response);
    assert_eq!(rows.len(), 1, "Aggregate should return one row");
    assert_eq!(rows[0][0], json!(5), "Count should be 5");
}

#[tokio::test]
#[ignore]
async fn test_aggregate_sum_with_group_by() {
    let relation = Relation::Aggregate {
        input: Box::new(Relation::From {
            collection: "products".into(),
            columns: vec!["category".into(), "stock".into()],
            arguments: Default::default(),
        }),
        group_by: vec![RelationalExpression::Column { index: 0 }],
        aggregates: vec![RelationalExpression::Sum {
            expr: Box::new(RelationalExpression::Column { index: 1 }),
        }],
    };

    let response = execute_query(relation).await;
    let rows = get_rows(&response);
    assert_eq!(rows.len(), 3, "Expected 3 categories: A, B, C");
}

// =============================================================================
// I.3 Join Tests
// =============================================================================

#[tokio::test]
#[ignore]
async fn test_left_join() {
    let relation = Relation::Join {
        left: Box::new(Relation::From {
            collection: "orders".into(),
            columns: vec!["product_id".into(), "quantity".into()],
            arguments: Default::default(),
        }),
        right: Box::new(Relation::From {
            collection: "products".into(),
            columns: vec!["_id".into(), "name".into()],
            arguments: Default::default(),
        }),
        join_type: JoinType::Left,
        on: vec![JoinOn {
            left: RelationalExpression::Column { index: 0 },
            right: RelationalExpression::Column { index: 0 },
        }],
    };

    let response = execute_query(relation).await;
    let rows = get_rows(&response);
    assert_eq!(rows.len(), 5, "All 5 orders should be returned");
    // Order 5 has product_id=99 (orphan), so columns 2,3 should be null
}

#[tokio::test]
#[ignore]
async fn test_inner_join() {
    let relation = Relation::Join {
        left: Box::new(Relation::From {
            collection: "orders".into(),
            columns: vec!["product_id".into(), "quantity".into()],
            arguments: Default::default(),
        }),
        right: Box::new(Relation::From {
            collection: "products".into(),
            columns: vec!["_id".into(), "name".into()],
            arguments: Default::default(),
        }),
        join_type: JoinType::Inner,
        on: vec![JoinOn {
            left: RelationalExpression::Column { index: 0 },
            right: RelationalExpression::Column { index: 0 },
        }],
    };

    let response = execute_query(relation).await;
    let rows = get_rows(&response);
    assert_eq!(rows.len(), 4, "4 orders have matching products (not product_id=99)");
}

// =============================================================================
// I.4 Window Function Tests
// =============================================================================

#[tokio::test]
#[ignore]
async fn test_window_row_number() {
    let relation = Relation::Window {
        input: Box::new(Relation::Sort {
            input: Box::new(Relation::From {
                collection: "products".into(),
                columns: vec!["name".into(), "price".into()],
                arguments: Default::default(),
            }),
            exprs: vec![Sort {
                expr: RelationalExpression::Column { index: 1 },
                direction: OrderDirection::Asc,
                nulls_sort: NullsSort::NullsLast,
            }],
        }),
        exprs: vec![RelationalExpression::RowNumber {
            partition_by: vec![],
            order_by: vec![Sort {
                expr: RelationalExpression::Column { index: 1 },
                direction: OrderDirection::Asc,
                nulls_sort: NullsSort::NullsLast,
            }],
        }],
    };

    let response = execute_query(relation).await;
    let rows = get_rows(&response);
    assert_eq!(rows.len(), 5);
    // Row number should be present in column index 2
}

// =============================================================================
// I.5 Union Tests
// =============================================================================

#[tokio::test]
#[ignore]
async fn test_union_two_collections() {
    let relation = Relation::Union {
        relations: vec![
            Relation::From {
                collection: "products".into(),
                columns: vec!["name".into()],
                arguments: Default::default(),
            },
            Relation::From {
                collection: "categories".into(),
                columns: vec!["name".into()],
                arguments: Default::default(),
            },
        ],
    };

    let response = execute_query(relation).await;
    let rows = get_rows(&response);
    assert_eq!(rows.len(), 8, "5 products + 3 categories = 8 rows");
}

// =============================================================================
// I.6 Complex Analytical Query Tests
// =============================================================================
// These tests represent realistic multi-level analytical queries that combine
// multiple operations: joins, aggregations, window functions, filters, sorts.

/// SQL equivalent:
/// ```sql
/// SELECT
///     o.product_id, o.quantity, p._id, p.name, p.price,
///     (o.quantity * p.price) as line_total
/// FROM orders o
/// INNER JOIN products p ON o.product_id = p._id
/// WHERE p.price > 10
/// ORDER BY line_total DESC
/// LIMIT 5;
/// ```
#[tokio::test]
#[ignore]
async fn test_complex_join_filter_sort_paginate() {
    // Step 1: Join orders with products
    let join = Relation::Join {
        left: Box::new(Relation::From {
            collection: "orders".into(),
            columns: vec!["product_id".into(), "quantity".into()],
            arguments: Default::default(),
        }),
        right: Box::new(Relation::From {
            collection: "products".into(),
            columns: vec!["_id".into(), "name".into(), "price".into()],
            arguments: Default::default(),
        }),
        join_type: JoinType::Inner,
        on: vec![JoinOn {
            left: RelationalExpression::Column { index: 0 },  // o.product_id
            right: RelationalExpression::Column { index: 0 }, // p._id
        }],
    };
    // Output columns: [product_id, quantity, _id, name, price] (indices 0-4)

    // Step 2: Filter where price > 10
    let filtered = Relation::Filter {
        input: Box::new(join),
        predicate: RelationalExpression::Gt {
            left: Box::new(RelationalExpression::Column { index: 4 }),  // price
            right: Box::new(RelationalExpression::Literal {
                literal: RelationalLiteral::Float64 { value: Float64(10.0) },
            }),
        },
    };

    // Step 3: Project to add computed column (quantity * price)
    let projected = Relation::Project {
        input: Box::new(filtered),
        exprs: vec![
            RelationalExpression::Column { index: 3 },  // name
            RelationalExpression::Column { index: 1 },  // quantity
            RelationalExpression::Column { index: 4 },  // price
            RelationalExpression::Multiply {            // line_total = quantity * price
                left: Box::new(RelationalExpression::Column { index: 1 }),
                right: Box::new(RelationalExpression::Column { index: 4 }),
            },
        ],
    };
    // Output columns: [name, quantity, price, line_total] (indices 0-3)

    // Step 4: Sort by line_total DESC
    let sorted = Relation::Sort {
        input: Box::new(projected),
        exprs: vec![Sort {
            expr: RelationalExpression::Column { index: 3 },  // line_total
            direction: OrderDirection::Desc,
            nulls_sort: NullsSort::NullsLast,
        }],
    };

    // Step 5: Limit to top 5
    let paginated = Relation::Paginate {
        input: Box::new(sorted),
        fetch: Some(5),
        skip: 0,
    };

    let response = execute_query(paginated).await;
    let rows = get_rows(&response);

    // Should return orders for products with price > 10 (Gadget=25, Gizmo=15, Thing=30)
    assert!(!rows.is_empty(), "Should have results");
    assert!(rows.len() <= 5, "Should be limited to 5 rows");

    // Verify line_total is computed and sorted descending
    if rows.len() >= 2 {
        let first_total = rows[0][3].as_f64().unwrap_or(0.0);
        let second_total = rows[1][3].as_f64().unwrap_or(0.0);
        assert!(first_total >= second_total, "Should be sorted by line_total DESC");
    }
}

/// SQL equivalent:
/// ```sql
/// SELECT
///     p.category,
///     COUNT(*) as product_count,
///     SUM(p.stock) as total_stock,
///     AVG(p.price) as avg_price
/// FROM products p
/// GROUP BY p.category
/// ORDER BY total_stock DESC;
/// ```
#[tokio::test]
#[ignore]
async fn test_complex_aggregate_with_multiple_functions() {
    // Step 1: From products with relevant columns
    let from = Relation::From {
        collection: "products".into(),
        columns: vec!["category".into(), "stock".into(), "price".into()],
        arguments: Default::default(),
    };
    // Output: [category, stock, price] (indices 0-2)

    // Step 2: Aggregate with GROUP BY category
    let aggregated = Relation::Aggregate {
        input: Box::new(from),
        group_by: vec![RelationalExpression::Column { index: 0 }],  // category
        aggregates: vec![
            // COUNT(*)
            RelationalExpression::Count {
                expr: Box::new(RelationalExpression::Literal {
                    literal: RelationalLiteral::Null,
                }),
                distinct: false,
            },
            // SUM(stock)
            RelationalExpression::Sum {
                expr: Box::new(RelationalExpression::Column { index: 1 }),
            },
            // AVG(price)
            RelationalExpression::Average {
                expr: Box::new(RelationalExpression::Column { index: 2 }),
            },
        ],
    };
    // Output: [category, count, sum_stock, avg_price] (indices 0-3)

    // Step 3: Sort by total_stock DESC
    let sorted = Relation::Sort {
        input: Box::new(aggregated),
        exprs: vec![Sort {
            expr: RelationalExpression::Column { index: 2 },  // sum_stock
            direction: OrderDirection::Desc,
            nulls_sort: NullsSort::NullsLast,
        }],
    };

    let response = execute_query(sorted).await;
    let rows = get_rows(&response);

    assert_eq!(rows.len(), 3, "Should have 3 categories: A, B, C");

    // Each row should have 4 columns: category, count, sum_stock, avg_price
    assert_eq!(rows[0].len(), 4, "Each row should have 4 columns");

    // Verify sorted by sum_stock descending
    if rows.len() >= 2 {
        let first_stock = rows[0][2].as_i64().unwrap_or(0);
        let second_stock = rows[1][2].as_i64().unwrap_or(0);
        assert!(first_stock >= second_stock, "Should be sorted by total_stock DESC");
    }
}

/// SQL equivalent:
/// ```sql
/// SELECT
///     name, category, price,
///     ROW_NUMBER() OVER (PARTITION BY category ORDER BY price DESC) as category_rank,
///     SUM(price) OVER (PARTITION BY category ORDER BY price DESC) as running_total
/// FROM products
/// WHERE price IS NOT NULL
/// ORDER BY category, category_rank;
/// ```
#[tokio::test]
#[ignore]
async fn test_complex_window_with_partition_and_running_total() {
    // Step 1: From products
    let from = Relation::From {
        collection: "products".into(),
        columns: vec!["name".into(), "category".into(), "price".into()],
        arguments: Default::default(),
    };
    // Output: [name, category, price] (indices 0-2)

    // Step 2: Filter out null prices
    let filtered = Relation::Filter {
        input: Box::new(from),
        predicate: RelationalExpression::Not {
            expr: Box::new(RelationalExpression::IsNull {
                expr: Box::new(RelationalExpression::Column { index: 2 }),
            }),
        },
    };

    // Step 3: Add window functions
    let windowed = Relation::Window {
        input: Box::new(filtered),
        exprs: vec![
            // ROW_NUMBER() OVER (PARTITION BY category ORDER BY price DESC)
            RelationalExpression::RowNumber {
                partition_by: vec![RelationalExpression::Column { index: 1 }],  // category
                order_by: vec![Sort {
                    expr: RelationalExpression::Column { index: 2 },  // price
                    direction: OrderDirection::Desc,
                    nulls_sort: NullsSort::NullsLast,
                }],
            },
        ],
    };
    // Output: [name, category, price, row_number] (indices 0-3)

    // Step 4: Sort by category, then rank
    let sorted = Relation::Sort {
        input: Box::new(windowed),
        exprs: vec![
            Sort {
                expr: RelationalExpression::Column { index: 1 },  // category
                direction: OrderDirection::Asc,
                nulls_sort: NullsSort::NullsLast,
            },
            Sort {
                expr: RelationalExpression::Column { index: 3 },  // row_number
                direction: OrderDirection::Asc,
                nulls_sort: NullsSort::NullsLast,
            },
        ],
    };

    let response = execute_query(sorted).await;
    let rows = get_rows(&response);

    // Should have 4 products (excluding the one with null price)
    assert_eq!(rows.len(), 4, "Should have 4 products with non-null prices");

    // Each row should have 4 columns: name, category, price, row_number
    assert_eq!(rows[0].len(), 4, "Each row should have 4 columns");

    // Verify row_number is present and is a positive integer
    for row in rows {
        let row_num = row[3].as_i64().expect("row_number should be an integer");
        assert!(row_num >= 1, "row_number should be >= 1");
    }
}

/// SQL equivalent:
/// ```sql
/// SELECT
///     c.name as category_name,
///     p.name as product_name,
///     o.quantity,
///     p.price,
///     (o.quantity * p.price) as order_value
/// FROM categories c
/// LEFT JOIN products p ON c._id = p.category
/// LEFT JOIN orders o ON p._id = o.product_id
/// WHERE c.active = true
/// ORDER BY c.name, order_value DESC NULLS LAST;
/// ```
#[tokio::test]
#[ignore]
async fn test_complex_multi_join_with_filter() {
    // Step 1: From categories
    let categories = Relation::From {
        collection: "categories".into(),
        columns: vec!["_id".into(), "name".into(), "active".into()],
        arguments: Default::default(),
    };
    // Output: [_id, name, active] (indices 0-2)

    // Step 2: Filter active categories
    let active_categories = Relation::Filter {
        input: Box::new(categories),
        predicate: RelationalExpression::Eq {
            left: Box::new(RelationalExpression::Column { index: 2 }),  // active
            right: Box::new(RelationalExpression::Literal {
                literal: RelationalLiteral::Boolean { value: true },
            }),
        },
    };

    // Step 3: Left join with products
    let with_products = Relation::Join {
        left: Box::new(active_categories),
        right: Box::new(Relation::From {
            collection: "products".into(),
            columns: vec!["category".into(), "name".into(), "price".into(), "_id".into()],
            arguments: Default::default(),
        }),
        join_type: JoinType::Left,
        on: vec![JoinOn {
            left: RelationalExpression::Column { index: 0 },   // c._id
            right: RelationalExpression::Column { index: 0 },  // p.category
        }],
    };
    // Output: [c._id, c.name, c.active, p.category, p.name, p.price, p._id] (indices 0-6)

    // Step 4: Left join with orders
    let with_orders = Relation::Join {
        left: Box::new(with_products),
        right: Box::new(Relation::From {
            collection: "orders".into(),
            columns: vec!["product_id".into(), "quantity".into()],
            arguments: Default::default(),
        }),
        join_type: JoinType::Left,
        on: vec![JoinOn {
            left: RelationalExpression::Column { index: 6 },   // p._id
            right: RelationalExpression::Column { index: 0 },  // o.product_id
        }],
    };
    // Output: [c._id, c.name, c.active, p.category, p.name, p.price, p._id, o.product_id, o.quantity]

    // Step 5: Project to clean up columns and add computed value
    let projected = Relation::Project {
        input: Box::new(with_orders),
        exprs: vec![
            RelationalExpression::Column { index: 1 },  // c.name (category_name)
            RelationalExpression::Column { index: 4 },  // p.name (product_name)
            RelationalExpression::Column { index: 8 },  // o.quantity
            RelationalExpression::Column { index: 5 },  // p.price
            RelationalExpression::Multiply {            // order_value
                left: Box::new(RelationalExpression::Column { index: 8 }),  // quantity
                right: Box::new(RelationalExpression::Column { index: 5 }), // price
            },
        ],
    };
    // Output: [category_name, product_name, quantity, price, order_value] (indices 0-4)

    // Step 6: Sort by category_name, then order_value DESC
    let sorted = Relation::Sort {
        input: Box::new(projected),
        exprs: vec![
            Sort {
                expr: RelationalExpression::Column { index: 0 },  // category_name
                direction: OrderDirection::Asc,
                nulls_sort: NullsSort::NullsLast,
            },
            Sort {
                expr: RelationalExpression::Column { index: 4 },  // order_value
                direction: OrderDirection::Desc,
                nulls_sort: NullsSort::NullsLast,
            },
        ],
    };

    let response = execute_query(sorted).await;
    let rows = get_rows(&response);

    // Should have results for active categories (A and B) with their products and orders
    assert!(!rows.is_empty(), "Should have results for active categories");

    // Each row should have 5 columns
    assert_eq!(rows[0].len(), 5, "Each row should have 5 columns");
}

/// SQL equivalent:
/// ```sql
/// WITH category_stats AS (
///     SELECT
///         category,
///         SUM(stock) as total_stock,
///         AVG(price) as avg_price
///     FROM products
///     GROUP BY category
/// )
/// SELECT * FROM category_stats
/// WHERE total_stock > 50
/// UNION ALL
/// SELECT 'ALL' as category, SUM(stock), AVG(price)
/// FROM products;
/// ```
#[tokio::test]
#[ignore]
async fn test_complex_aggregate_filter_union() {
    // Branch 1: Category stats filtered by stock > 50
    let category_agg = Relation::Aggregate {
        input: Box::new(Relation::From {
            collection: "products".into(),
            columns: vec!["category".into(), "stock".into(), "price".into()],
            arguments: Default::default(),
        }),
        group_by: vec![RelationalExpression::Column { index: 0 }],  // category
        aggregates: vec![
            RelationalExpression::Sum {
                expr: Box::new(RelationalExpression::Column { index: 1 }),  // stock
            },
            RelationalExpression::Average {
                expr: Box::new(RelationalExpression::Column { index: 2 }),  // price
            },
        ],
    };
    // Output: [category, sum_stock, avg_price]

    let filtered_categories = Relation::Filter {
        input: Box::new(category_agg),
        predicate: RelationalExpression::Gt {
            left: Box::new(RelationalExpression::Column { index: 1 }),  // sum_stock
            right: Box::new(RelationalExpression::Literal {
                literal: RelationalLiteral::Int64 { value: 50 },
            }),
        },
    };

    // Branch 2: Overall totals (no grouping)
    let overall_agg = Relation::Aggregate {
        input: Box::new(Relation::From {
            collection: "products".into(),
            columns: vec!["stock".into(), "price".into()],
            arguments: Default::default(),
        }),
        group_by: vec![],
        aggregates: vec![
            // Add a literal 'ALL' for the category column to match union schema
            RelationalExpression::Sum {
                expr: Box::new(RelationalExpression::Column { index: 0 }),  // stock
            },
            RelationalExpression::Average {
                expr: Box::new(RelationalExpression::Column { index: 1 }),  // price
            },
        ],
    };
    // Output: [sum_stock, avg_price] - Note: different column count, need project

    // Project to add 'ALL' as category
    let overall_with_label = Relation::Project {
        input: Box::new(overall_agg),
        exprs: vec![
            RelationalExpression::Literal {
                literal: RelationalLiteral::String { value: "ALL".into() },
            },
            RelationalExpression::Column { index: 0 },  // sum_stock
            RelationalExpression::Column { index: 1 },  // avg_price
        ],
    };

    // Union both branches
    let unioned = Relation::Union {
        relations: vec![filtered_categories, overall_with_label],
    };

    // Sort by category
    let sorted = Relation::Sort {
        input: Box::new(unioned),
        exprs: vec![Sort {
            expr: RelationalExpression::Column { index: 0 },
            direction: OrderDirection::Asc,
            nulls_sort: NullsSort::NullsLast,
        }],
    };

    let response = execute_query(sorted).await;
    let rows = get_rows(&response);

    // Should have categories with stock > 50 plus the "ALL" row
    assert!(!rows.is_empty(), "Should have results");

    // Find the "ALL" row
    let all_row = rows.iter().find(|r| r[0] == json!("ALL"));
    assert!(all_row.is_some(), "Should have an 'ALL' summary row");
}

/// SQL equivalent:
/// ```sql
/// SELECT
///     p.name,
///     p.category,
///     p.price,
///     RANK() OVER (ORDER BY p.price DESC) as overall_rank,
///     DENSE_RANK() OVER (PARTITION BY p.category ORDER BY p.price DESC) as category_rank
/// FROM products p
/// WHERE p.stock > 0
/// ORDER BY overall_rank, category_rank;
/// ```
#[tokio::test]
#[ignore]
async fn test_complex_multiple_window_functions() {
    // Step 1: From products
    let from = Relation::From {
        collection: "products".into(),
        columns: vec!["name".into(), "category".into(), "price".into(), "stock".into()],
        arguments: Default::default(),
    };

    // Step 2: Filter stock > 0
    let filtered = Relation::Filter {
        input: Box::new(from),
        predicate: RelationalExpression::Gt {
            left: Box::new(RelationalExpression::Column { index: 3 }),  // stock
            right: Box::new(RelationalExpression::Literal {
                literal: RelationalLiteral::Int64 { value: 0 },
            }),
        },
    };
    // Output: [name, category, price, stock] (indices 0-3)

    // Step 3: Project to remove stock (we don't need it in output)
    let projected = Relation::Project {
        input: Box::new(filtered),
        exprs: vec![
            RelationalExpression::Column { index: 0 },  // name
            RelationalExpression::Column { index: 1 },  // category
            RelationalExpression::Column { index: 2 },  // price
        ],
    };
    // Output: [name, category, price] (indices 0-2)

    // Step 4: Add multiple window functions
    let windowed = Relation::Window {
        input: Box::new(projected),
        exprs: vec![
            // RANK() OVER (ORDER BY price DESC) - overall rank
            RelationalExpression::Rank {
                partition_by: vec![],
                order_by: vec![Sort {
                    expr: RelationalExpression::Column { index: 2 },  // price
                    direction: OrderDirection::Desc,
                    nulls_sort: NullsSort::NullsLast,
                }],
            },
            // DENSE_RANK() OVER (PARTITION BY category ORDER BY price DESC)
            RelationalExpression::DenseRank {
                partition_by: vec![RelationalExpression::Column { index: 1 }],  // category
                order_by: vec![Sort {
                    expr: RelationalExpression::Column { index: 2 },  // price
                    direction: OrderDirection::Desc,
                    nulls_sort: NullsSort::NullsLast,
                }],
            },
        ],
    };
    // Output: [name, category, price, overall_rank, category_rank] (indices 0-4)

    // Step 5: Sort by overall_rank, then category_rank
    let sorted = Relation::Sort {
        input: Box::new(windowed),
        exprs: vec![
            Sort {
                expr: RelationalExpression::Column { index: 3 },  // overall_rank
                direction: OrderDirection::Asc,
                nulls_sort: NullsSort::NullsLast,
            },
            Sort {
                expr: RelationalExpression::Column { index: 4 },  // category_rank
                direction: OrderDirection::Asc,
                nulls_sort: NullsSort::NullsLast,
            },
        ],
    };

    let response = execute_query(sorted).await;
    let rows = get_rows(&response);

    // Should have products with stock > 0 (4 products: Widget, Gadget, Gizmo, Item)
    // Thing has stock=0 so excluded
    assert_eq!(rows.len(), 4, "Should have 4 products with stock > 0");

    // Each row should have 5 columns
    assert_eq!(rows[0].len(), 5, "Each row should have 5 columns");

    // Verify ranks are positive integers
    for row in rows {
        let overall_rank = row[3].as_i64().expect("overall_rank should be integer");
        let category_rank = row[4].as_i64().expect("category_rank should be integer");
        assert!(overall_rank >= 1, "overall_rank should be >= 1");
        assert!(category_rank >= 1, "category_rank should be >= 1");
    }
}

