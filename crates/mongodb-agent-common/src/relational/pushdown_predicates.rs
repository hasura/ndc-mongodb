//! Predicate pushdown optimization for relational queries.
//!
//! This module provides a transformation pass that pushes Filter nodes below
//! Project nodes when possible. This allows filters to execute earlier in the
//! pipeline, potentially enabling better index usage.
//!
//! Example transformation:
//! ```text
//! Filter(Project(From, [col_0=name, col_1=age]), col_1 > 18)
//! →
//! Project(Filter(From, age > 18), [col_0=name, col_1=age])
//! ```
//!
//! The transformation is only applied when:
//! 1. All column references in the predicate refer to simple column projections
//!    (not computed expressions like `col_0 + col_1`)
//! 2. The column indices can be remapped to the input relation

use ndc_models::{Relation, RelationalExpression, Sort};

/// Push predicates down through projections when possible.
///
/// This optimization rewrites the relation tree to move Filter nodes below
/// Project nodes, which allows filters to execute on the original data before
/// projection. This can enable better index usage.
pub fn pushdown_predicates(relation: &Relation) -> Relation {
    match relation {
        // The key pattern: Filter on top of Project
        Relation::Filter { input, predicate } => {
            let normalized_input = pushdown_predicates(input);

            if let Relation::Project {
                input: proj_input,
                exprs,
            } = &normalized_input
            {
                // Try to push the predicate down through the project
                if let Some(remapped) = try_remap_predicate(predicate, exprs) {
                    // Success! Push the filter below the project
                    tracing::debug!("pushing predicate down through project");
                    // Create the new filter node
                    let new_filter = Relation::Filter {
                        input: proj_input.clone(),
                        predicate: remapped,
                    };
                    // Recursively try to push the filter down further through more projects
                    let pushed_filter = pushdown_predicates(&new_filter);
                    return Relation::Project {
                        input: Box::new(pushed_filter),
                        exprs: exprs.clone(),
                    };
                }
            }

            // Cannot push down, keep original structure
            Relation::Filter {
                input: Box::new(normalized_input),
                predicate: predicate.clone(),
            }
        }

        // Sort on top of Project - try to push down
        Relation::Sort { input, exprs } => {
            let normalized_input = pushdown_predicates(input);

            if let Relation::Project {
                input: proj_input,
                exprs: proj_exprs,
            } = &normalized_input
            {
                // Try to push the sort down through the project
                if let Some(remapped) = try_remap_sort(exprs, proj_exprs) {
                    // Success! Push the sort below the project
                    tracing::debug!("pushing sort down through project");
                    // Create the new sort node
                    let new_sort = Relation::Sort {
                        input: proj_input.clone(),
                        exprs: remapped,
                    };
                    // Recursively try to push the sort down further through more projects
                    let pushed_sort = pushdown_predicates(&new_sort);
                    return Relation::Project {
                        input: Box::new(pushed_sort),
                        exprs: proj_exprs.clone(),
                    };
                }
            }

            // Cannot push down, keep original structure
            Relation::Sort {
                input: Box::new(normalized_input),
                exprs: exprs.clone(),
            }
        }

        // Recursively normalize other relation types
        Relation::From { .. } => relation.clone(),

        Relation::Paginate { input, fetch, skip } => Relation::Paginate {
            input: Box::new(pushdown_predicates(input)),
            fetch: *fetch,
            skip: *skip,
        },

        Relation::Project { input, exprs } => Relation::Project {
            input: Box::new(pushdown_predicates(input)),
            exprs: exprs.clone(),
        },

        Relation::Join {
            left,
            right,
            on,
            join_type,
        } => Relation::Join {
            left: Box::new(pushdown_predicates(left)),
            right: Box::new(pushdown_predicates(right)),
            on: on.clone(),
            join_type: *join_type,
        },

        Relation::Aggregate {
            input,
            group_by,
            aggregates,
        } => Relation::Aggregate {
            input: Box::new(pushdown_predicates(input)),
            group_by: group_by.clone(),
            aggregates: aggregates.clone(),
        },

        Relation::Window { input, exprs } => Relation::Window {
            input: Box::new(pushdown_predicates(input)),
            exprs: exprs.clone(),
        },

        Relation::Union { relations } => Relation::Union {
            relations: relations.iter().map(pushdown_predicates).collect(),
        },
    }
}

/// Try to remap a predicate's column references through a projection.
///
/// Returns `Some(remapped_predicate)` if all column references in the predicate
/// can be traced back to simple column references in the projection expressions.
/// Returns `None` if any column reference in the predicate maps to a computed expression.
fn try_remap_predicate(
    predicate: &RelationalExpression,
    proj_exprs: &[RelationalExpression],
) -> Option<RelationalExpression> {
    remap_expression(predicate, proj_exprs)
}

/// Try to remap sort expressions' column references through a projection.
///
/// Returns `Some(remapped_sorts)` if all column references in all sort expressions
/// can be traced back to simple column references in the projection expressions.
/// Returns `None` if any column reference maps to a computed expression.
fn try_remap_sort(sort_exprs: &[Sort], proj_exprs: &[RelationalExpression]) -> Option<Vec<Sort>> {
    sort_exprs
        .iter()
        .map(|sort| {
            let remapped_expr = remap_expression(&sort.expr, proj_exprs)?;
            Some(Sort {
                expr: remapped_expr,
                direction: sort.direction,
                nulls_sort: sort.nulls_sort,
            })
        })
        .collect()
}

/// Check if an expression is a simple field reference (Column or GetField chain on a Column).
///
/// Simple field references can be pushed down through projections because they
/// directly reference fields from the input relation.
fn is_simple_field_reference(expr: &RelationalExpression) -> bool {
    match expr {
        RelationalExpression::Column { .. } => true,
        RelationalExpression::GetField { column, .. } => is_simple_field_reference(column),
        _ => false,
    }
}

/// Recursively remap column references in an expression through projection expressions.
fn remap_expression(
    expr: &RelationalExpression,
    proj_exprs: &[RelationalExpression],
) -> Option<RelationalExpression> {
    match expr {
        RelationalExpression::Column { index } => {
            // Look up what this column maps to in the projection
            let proj_expr = proj_exprs.get(*index as usize)?;

            // Remap if it's a simple column reference or a GetField chain
            match proj_expr {
                RelationalExpression::Column { index: orig_index } => {
                    Some(RelationalExpression::Column { index: *orig_index })
                }
                RelationalExpression::GetField {
                    column: _,
                    field: _,
                } => {
                    // The GetField in the projection references the input columns directly.
                    // We need to check if the base column is a simple Column reference
                    // that we can pass through.
                    if is_simple_field_reference(proj_expr) {
                        // Return the GetField as-is - it already references input columns
                        Some(proj_expr.clone())
                    } else {
                        None
                    }
                }
                // Projection produces a computed value - can't push down
                _ => None,
            }
        }

        // GetField expressions - remap the base column
        RelationalExpression::GetField { column, field } => {
            let remapped_column = remap_expression(column, proj_exprs)?;
            Some(RelationalExpression::GetField {
                column: Box::new(remapped_column),
                field: field.clone(),
            })
        }

        // Literals don't need remapping
        RelationalExpression::Literal { literal } => Some(RelationalExpression::Literal {
            literal: literal.clone(),
        }),

        // Binary comparisons - remap both sides
        RelationalExpression::Eq { left, right } => Some(RelationalExpression::Eq {
            left: Box::new(remap_expression(left, proj_exprs)?),
            right: Box::new(remap_expression(right, proj_exprs)?),
        }),
        RelationalExpression::NotEq { left, right } => Some(RelationalExpression::NotEq {
            left: Box::new(remap_expression(left, proj_exprs)?),
            right: Box::new(remap_expression(right, proj_exprs)?),
        }),
        RelationalExpression::Lt { left, right } => Some(RelationalExpression::Lt {
            left: Box::new(remap_expression(left, proj_exprs)?),
            right: Box::new(remap_expression(right, proj_exprs)?),
        }),
        RelationalExpression::LtEq { left, right } => Some(RelationalExpression::LtEq {
            left: Box::new(remap_expression(left, proj_exprs)?),
            right: Box::new(remap_expression(right, proj_exprs)?),
        }),
        RelationalExpression::Gt { left, right } => Some(RelationalExpression::Gt {
            left: Box::new(remap_expression(left, proj_exprs)?),
            right: Box::new(remap_expression(right, proj_exprs)?),
        }),
        RelationalExpression::GtEq { left, right } => Some(RelationalExpression::GtEq {
            left: Box::new(remap_expression(left, proj_exprs)?),
            right: Box::new(remap_expression(right, proj_exprs)?),
        }),

        // Logical operators
        RelationalExpression::And { left, right } => Some(RelationalExpression::And {
            left: Box::new(remap_expression(left, proj_exprs)?),
            right: Box::new(remap_expression(right, proj_exprs)?),
        }),
        RelationalExpression::Or { left, right } => Some(RelationalExpression::Or {
            left: Box::new(remap_expression(left, proj_exprs)?),
            right: Box::new(remap_expression(right, proj_exprs)?),
        }),
        RelationalExpression::Not { expr } => Some(RelationalExpression::Not {
            expr: Box::new(remap_expression(expr, proj_exprs)?),
        }),

        // IsNull and similar
        RelationalExpression::IsNull { expr } => Some(RelationalExpression::IsNull {
            expr: Box::new(remap_expression(expr, proj_exprs)?),
        }),
        RelationalExpression::IsNotNull { expr } => Some(RelationalExpression::IsNotNull {
            expr: Box::new(remap_expression(expr, proj_exprs)?),
        }),

        // For any other expression types, don't push down to be safe
        // This includes complex expressions like arithmetic, function calls, etc.
        _ => None,
    }
}

#[allow(unused_imports)]
use ndc_models::RelationalLiteral;

#[cfg(test)]
mod tests {
    use super::*;
    use ndc_models::{NullsSort, OrderDirection};

    #[test]
    fn pushes_filter_below_simple_project() {
        // Filter(Project(From, [col_0=name, col_1=age]), col_1 > 18)
        // should become
        // Project(Filter(From, age > 18), [col_0=name, col_1=age])
        let relation = Relation::Filter {
            input: Box::new(Relation::Project {
                input: Box::new(Relation::From {
                    collection: "users".into(),
                    columns: vec!["name".into(), "age".into()],
                    arguments: Default::default(),
                }),
                exprs: vec![
                    RelationalExpression::Column { index: 0 }, // name
                    RelationalExpression::Column { index: 1 }, // age
                ],
            }),
            predicate: RelationalExpression::Gt {
                left: Box::new(RelationalExpression::Column { index: 1 }), // col_1 in project
                right: Box::new(RelationalExpression::Literal {
                    literal: RelationalLiteral::Int64 { value: 18 },
                }),
            },
        };

        let result = pushdown_predicates(&relation);

        // Should be Project(Filter(From, ...))
        match &result {
            Relation::Project { input, exprs } => {
                assert_eq!(exprs.len(), 2);

                match input.as_ref() {
                    Relation::Filter {
                        input: inner,
                        predicate,
                    } => {
                        // Inner should be From
                        assert!(matches!(inner.as_ref(), Relation::From { .. }));

                        // Predicate should reference original column index 1 (age)
                        match predicate {
                            RelationalExpression::Gt { left, .. } => {
                                assert!(matches!(
                                    left.as_ref(),
                                    RelationalExpression::Column { index: 1 }
                                ));
                            }
                            _ => panic!("Expected Gt predicate"),
                        }
                    }
                    _ => panic!("Expected Filter inside Project"),
                }
            }
            _ => panic!("Expected Project at top level"),
        }
    }

    #[test]
    fn does_not_push_filter_with_computed_column() {
        // Filter on a computed column cannot be pushed down
        let relation = Relation::Filter {
            input: Box::new(Relation::Project {
                input: Box::new(Relation::From {
                    collection: "users".into(),
                    columns: vec!["name".into(), "age".into()],
                    arguments: Default::default(),
                }),
                exprs: vec![
                    RelationalExpression::Column { index: 0 }, // name
                    // age + 10 - a computed expression
                    RelationalExpression::Plus {
                        left: Box::new(RelationalExpression::Column { index: 1 }),
                        right: Box::new(RelationalExpression::Literal {
                            literal: RelationalLiteral::Int64 { value: 10 },
                        }),
                    },
                ],
            }),
            predicate: RelationalExpression::Gt {
                left: Box::new(RelationalExpression::Column { index: 1 }), // computed col
                right: Box::new(RelationalExpression::Literal {
                    literal: RelationalLiteral::Int64 { value: 28 },
                }),
            },
        };

        let result = pushdown_predicates(&relation);

        // Should remain Filter(Project(...))
        assert!(matches!(result, Relation::Filter { .. }));
    }

    #[test]
    fn pushes_and_predicate() {
        // Filter(Project(From, [name, age]), col_1 > 18 AND col_1 < 65)
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
            predicate: RelationalExpression::And {
                left: Box::new(RelationalExpression::Gt {
                    left: Box::new(RelationalExpression::Column { index: 1 }),
                    right: Box::new(RelationalExpression::Literal {
                        literal: RelationalLiteral::Int64 { value: 18 },
                    }),
                }),
                right: Box::new(RelationalExpression::Lt {
                    left: Box::new(RelationalExpression::Column { index: 1 }),
                    right: Box::new(RelationalExpression::Literal {
                        literal: RelationalLiteral::Int64 { value: 65 },
                    }),
                }),
            },
        };

        let result = pushdown_predicates(&relation);

        // Should be Project(Filter(From, ...))
        assert!(matches!(result, Relation::Project { .. }));
    }

    #[test]
    fn pushes_through_reordered_columns() {
        // Project reorders columns: [age, name] instead of [name, age]
        // Filter on col_0 (which is age in the project) should become filter on index 1
        let relation = Relation::Filter {
            input: Box::new(Relation::Project {
                input: Box::new(Relation::From {
                    collection: "users".into(),
                    columns: vec!["name".into(), "age".into()], // 0=name, 1=age
                    arguments: Default::default(),
                }),
                exprs: vec![
                    RelationalExpression::Column { index: 1 }, // age -> col_0
                    RelationalExpression::Column { index: 0 }, // name -> col_1
                ],
            }),
            predicate: RelationalExpression::Gt {
                left: Box::new(RelationalExpression::Column { index: 0 }), // col_0 = age
                right: Box::new(RelationalExpression::Literal {
                    literal: RelationalLiteral::Int64 { value: 21 },
                }),
            },
        };

        let result = pushdown_predicates(&relation);

        // Should be Project(Filter(From, age > 21))
        match &result {
            Relation::Project { input, .. } => {
                match input.as_ref() {
                    Relation::Filter { predicate, .. } => {
                        // Predicate should now reference original index 1 (age)
                        match predicate {
                            RelationalExpression::Gt { left, .. } => match left.as_ref() {
                                RelationalExpression::Column { index } => {
                                    assert_eq!(
                                        *index, 1,
                                        "Should reference original age column at index 1"
                                    );
                                }
                                _ => panic!("Expected Column"),
                            },
                            _ => panic!("Expected Gt"),
                        }
                    }
                    _ => panic!("Expected Filter"),
                }
            }
            _ => panic!("Expected Project"),
        }
    }

    #[test]
    fn pushes_sort_down_through_project() {
        use ndc_models::{NullsSort, OrderDirection, Sort};

        // Sort(Project(From, [...]), [col_1 DESC]) should become
        // Project(Sort(From, [age DESC]), [...])
        let relation = Relation::Sort {
            input: Box::new(Relation::Project {
                input: Box::new(Relation::From {
                    collection: "users".into(),
                    columns: vec!["name".into(), "age".into()], // 0=name, 1=age
                    arguments: Default::default(),
                }),
                exprs: vec![
                    RelationalExpression::Column { index: 0 }, // name -> col_0
                    RelationalExpression::Column { index: 1 }, // age -> col_1
                ],
            }),
            exprs: vec![Sort {
                expr: RelationalExpression::Column { index: 1 }, // col_1 = age
                direction: OrderDirection::Desc,
                nulls_sort: NullsSort::NullsLast,
            }],
        };

        let result = pushdown_predicates(&relation);

        // Should be Project(Sort(From, age DESC), [...])
        match &result {
            Relation::Project { input, .. } => {
                match input.as_ref() {
                    Relation::Sort { exprs, .. } => {
                        // Sort should now reference original index 1 (age)
                        assert_eq!(exprs.len(), 1);
                        match &exprs[0].expr {
                            RelationalExpression::Column { index } => {
                                assert_eq!(
                                    *index, 1,
                                    "Sort should reference original age column at index 1"
                                );
                            }
                            _ => panic!("Expected Column in sort expression"),
                        }
                    }
                    _ => panic!("Expected Sort inside Project"),
                }
            }
            _ => panic!("Expected Project at top level"),
        }
    }

    #[test]
    fn does_not_push_sort_with_computed_expression() {
        use ndc_models::{NullsSort, OrderDirection, Sort};

        // Sort on a computed expression cannot be pushed down
        let relation = Relation::Sort {
            input: Box::new(Relation::Project {
                input: Box::new(Relation::From {
                    collection: "users".into(),
                    columns: vec!["name".into(), "age".into()],
                    arguments: Default::default(),
                }),
                exprs: vec![
                    RelationalExpression::Column { index: 0 },
                    // col_1 is a computed expression (literal), not a column reference
                    RelationalExpression::Literal {
                        literal: RelationalLiteral::Int64 { value: 100 },
                    },
                ],
            }),
            exprs: vec![Sort {
                expr: RelationalExpression::Column { index: 1 }, // col_1 is computed
                direction: OrderDirection::Asc,
                nulls_sort: NullsSort::NullsLast,
            }],
        };

        let result = pushdown_predicates(&relation);

        // Should NOT be pushed down - Sort should stay on top
        assert!(
            matches!(result, Relation::Sort { .. }),
            "Sort should NOT be pushed down when referencing computed columns"
        );
    }

    #[test]
    fn pushes_sort_with_get_field_expression() {
        use ndc_models::{NullsSort, OrderDirection, Sort};

        // This test mirrors the actual failing query pattern:
        // Sort(Project(Project(Filter(From, ...), [col_0=aggregate_id, col_1=current_state, col_2=last_update_time]),
        //             [col_0, GetField(col_1, "fail_id"), ..., GetField(col_1, "trade_date"), ...]),
        //      [col_4 DESC])  // col_4 = GetField(col_1, "trade_date")
        //
        // The sort on col_4 should be pushed down and remapped to GetField(col_1, "trade_date")
        // on the inner projection where col_1 is still current_state
        let relation = Relation::Sort {
            input: Box::new(Relation::Project {
                input: Box::new(Relation::From {
                    collection: "failsAggregate".into(),
                    columns: vec![
                        "aggregate_id".into(),
                        "current_state".into(), // index 1
                        "last_update_time".into(),
                    ],
                    arguments: Default::default(),
                }),
                exprs: vec![
                    RelationalExpression::Column { index: 0 }, // aggregate_id -> col_0
                    RelationalExpression::GetField {
                        // current_state.trade_date -> col_1
                        column: Box::new(RelationalExpression::Column { index: 1 }),
                        field: "trade_date".to_string(),
                    },
                ],
            }),
            exprs: vec![Sort {
                expr: RelationalExpression::Column { index: 1 }, // col_1 = trade_date
                direction: OrderDirection::Desc,
                nulls_sort: NullsSort::NullsFirst,
            }],
        };

        let result = pushdown_predicates(&relation);

        // Should be Project(Sort(From, GetField(col_1, "trade_date") DESC), [...])
        match &result {
            Relation::Project { input, .. } => {
                match input.as_ref() {
                    Relation::Sort { exprs, .. } => {
                        assert_eq!(exprs.len(), 1);
                        // Sort expression should now be GetField(Column(1), "trade_date")
                        match &exprs[0].expr {
                            RelationalExpression::GetField { column, field } => {
                                assert_eq!(field, "trade_date");
                                match column.as_ref() {
                                    RelationalExpression::Column { index } => {
                                        assert_eq!(
                                            *index, 1,
                                            "GetField should reference original current_state column at index 1"
                                        );
                                    }
                                    _ => panic!("Expected Column in GetField"),
                                }
                            }
                            _ => panic!(
                                "Expected GetField in sort expression, got {:?}",
                                &exprs[0].expr
                            ),
                        }
                    }
                    _ => panic!("Expected Sort inside Project, got {:?}", input.as_ref()),
                }
            }
            _ => panic!("Expected Project at top level, got {:?}", result),
        }
    }

    #[test]
    fn pushes_sort_through_multiple_projects() {
        // Sort(Project2(Project1(From))) should become Project2(Project1(Sort(From)))
        // This tests the recursive pushdown through multiple project layers
        let relation = Relation::Sort {
            input: Box::new(Relation::Project {
                input: Box::new(Relation::Project {
                    input: Box::new(Relation::From {
                        collection: "test".into(),
                        columns: vec!["aggregate_id".into(), "current_state".into()],
                        arguments: Default::default(),
                    }),
                    exprs: vec![
                        RelationalExpression::Column { index: 0 },
                        RelationalExpression::Column { index: 1 },
                        RelationalExpression::GetField {
                            column: Box::new(RelationalExpression::Column { index: 1 }),
                            field: "trade_date".to_string(),
                        },
                    ],
                }),
                exprs: vec![
                    RelationalExpression::Column { index: 0 },
                    RelationalExpression::Column { index: 2 }, // References trade_date GetField
                ],
            }),
            exprs: vec![Sort {
                expr: RelationalExpression::Column { index: 1 }, // References col_1 which is trade_date
                direction: OrderDirection::Desc,
                nulls_sort: NullsSort::NullsFirst,
            }],
        };

        let result = pushdown_predicates(&relation);

        // Should be Project2(Project1(Sort(From, GetField(col_1, "trade_date") DESC)))
        // The sort should have been pushed through BOTH projects
        fn find_sort(relation: &Relation) -> Option<&Relation> {
            match relation {
                Relation::Sort { .. } => Some(relation),
                Relation::Project { input, .. } => find_sort(input),
                Relation::Filter { input, .. } => find_sort(input),
                _ => None,
            }
        }

        // Verify Sort exists and is above From but below Projects
        let sort = find_sort(&result).expect("Sort should exist in result");
        match sort {
            Relation::Sort { input, exprs } => {
                // Sort's input should be From (pushed through both projects)
                assert!(
                    matches!(input.as_ref(), Relation::From { .. }),
                    "Sort should be directly above From after pushdown through multiple projects, got {:?}",
                    input.as_ref()
                );
                // Sort expression should be GetField referencing original column
                match &exprs[0].expr {
                    RelationalExpression::GetField { column, field } => {
                        assert_eq!(field, "trade_date");
                        match column.as_ref() {
                            RelationalExpression::Column { index } => {
                                assert_eq!(*index, 1, "Should reference original current_state");
                            }
                            _ => panic!("Expected Column in GetField"),
                        }
                    }
                    _ => panic!("Expected GetField, got {:?}", &exprs[0].expr),
                }
            }
            _ => panic!("Expected Sort"),
        }
    }
}
