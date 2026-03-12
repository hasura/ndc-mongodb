//! Column origin tracing for relational query optimization.
//!
//! This module provides functionality to trace column indices back through
//! the relation tree to find their original field names from the base collection.
//! This is essential for generating index-friendly MongoDB queries.

use ndc_models::{Relation, RelationalExpression};

use super::normalize_joins::column_count;

/// Represents the origin of a column in the relation tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnOrigin {
    /// The original field path from the base collection (e.g., "current_state.trade_date").
    /// None if the column is computed from an expression rather than a simple field reference.
    pub original_path: Option<String>,
    /// The collection this field comes from.
    pub collection: Option<String>,
}

impl ColumnOrigin {
    /// Create a new ColumnOrigin with a known original path.
    pub fn from_field(collection: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            original_path: Some(path.into()),
            collection: Some(collection.into()),
        }
    }

    /// Create a ColumnOrigin for a computed column (no original path).
    pub fn computed() -> Self {
        Self {
            original_path: None,
            collection: None,
        }
    }

    /// Returns true if this column traces back to an original field.
    pub fn is_original_field(&self) -> bool {
        self.original_path.is_some()
    }
}

/// Trace a column index back through the relation tree to find its origin.
///
/// Given a column index and a relation, this function walks down through the
/// relation tree to determine what original field (if any) the column refers to.
///
/// # Arguments
/// * `relation` - The relation tree to trace through
/// * `column_index` - The column index to trace
///
/// # Returns
/// A `ColumnOrigin` describing where this column comes from.
pub fn trace_column_origin(relation: &Relation, column_index: u64) -> ColumnOrigin {
    match relation {
        // Base case: From relation has the original field names
        Relation::From {
            collection,
            columns,
            ..
        } => {
            if let Some(field_name) = columns.get(column_index as usize) {
                ColumnOrigin::from_field(collection.as_str(), field_name.as_str())
            } else {
                ColumnOrigin::computed() // Index out of bounds
            }
        }

        // Filter, Sort, Paginate don't change column indices - pass through
        Relation::Filter { input, .. }
        | Relation::Sort { input, .. }
        | Relation::Paginate { input, .. } => trace_column_origin(input, column_index),

        // Project remaps columns - need to trace through the projection expression
        Relation::Project { input, exprs } => {
            if let Some(expr) = exprs.get(column_index as usize) {
                trace_expression_origin(input, expr)
            } else {
                ColumnOrigin::computed() // Index out of bounds
            }
        }

        // Aggregate outputs are always computed (group keys + aggregates)
        Relation::Aggregate {
            input,
            group_by,
            aggregates: _,
        } => {
            let group_count = group_by.len() as u64;
            if column_index < group_count {
                // This is a group-by column - trace the expression
                if let Some(expr) = group_by.get(column_index as usize) {
                    trace_expression_origin(input, expr)
                } else {
                    ColumnOrigin::computed()
                }
            } else {
                // This is an aggregate column - always computed
                ColumnOrigin::computed()
            }
        }

        // Window adds new columns at the end - existing columns pass through
        Relation::Window { input, exprs: _ } => {
            let input_column_count = column_count(input);
            if column_index < input_column_count {
                trace_column_origin(input, column_index)
            } else {
                // Window function output - always computed
                ColumnOrigin::computed()
            }
        }

        // Join combines columns from left and right
        Relation::Join {
            left,
            right,
            join_type,
            ..
        } => {
            let left_count = column_count(left);

            // For semi/anti joins, only left columns are output
            if matches!(
                join_type,
                ndc_models::JoinType::LeftSemi
                    | ndc_models::JoinType::LeftAnti
                    | ndc_models::JoinType::RightSemi
                    | ndc_models::JoinType::RightAnti
            ) {
                if matches!(
                    join_type,
                    ndc_models::JoinType::RightSemi | ndc_models::JoinType::RightAnti
                ) {
                    trace_column_origin(right, column_index)
                } else {
                    trace_column_origin(left, column_index)
                }
            } else if column_index < left_count {
                trace_column_origin(left, column_index)
            } else {
                trace_column_origin(right, column_index - left_count)
            }
        }

        // Union - trace through the first relation (all should have same schema)
        Relation::Union { relations } => {
            if let Some(first) = relations.first() {
                trace_column_origin(first, column_index)
            } else {
                ColumnOrigin::computed()
            }
        }
    }
}

/// Trace an expression back to its origin field.
///
/// If the expression is a simple column reference, traces it back through
/// the input relation. If it's a GetField on a column, traces and appends the field.
/// Otherwise returns a computed origin.
fn trace_expression_origin(input: &Relation, expr: &RelationalExpression) -> ColumnOrigin {
    match expr {
        RelationalExpression::Column { index } => trace_column_origin(input, *index),

        // GetField accesses a nested field - trace the base column and append the field name
        RelationalExpression::GetField { column, field } => {
            let base_origin = trace_expression_origin(input, column);
            match base_origin.original_path {
                Some(base_path) => ColumnOrigin {
                    original_path: Some(format!("{}.{}", base_path, field)),
                    collection: base_origin.collection,
                },
                None => ColumnOrigin::computed(),
            }
        }

        // All other expressions are computed
        _ => ColumnOrigin::computed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traces_column_from_from_relation() {
        let relation = Relation::From {
            collection: "users".into(),
            columns: vec!["name".into(), "age".into(), "email".into()],
            arguments: Default::default(),
        };

        let origin = trace_column_origin(&relation, 0);
        assert_eq!(origin.collection, Some("users".to_string()));
        assert_eq!(origin.original_path, Some("name".to_string()));

        let origin = trace_column_origin(&relation, 2);
        assert_eq!(origin.original_path, Some("email".to_string()));
    }

    #[test]
    fn traces_column_through_filter() {
        let relation = Relation::Filter {
            input: Box::new(Relation::From {
                collection: "users".into(),
                columns: vec!["name".into(), "age".into()],
                arguments: Default::default(),
            }),
            predicate: RelationalExpression::Gt {
                left: Box::new(RelationalExpression::Column { index: 1 }),
                right: Box::new(RelationalExpression::Literal {
                    literal: ndc_models::RelationalLiteral::Int64 { value: 18 },
                }),
            },
        };

        let origin = trace_column_origin(&relation, 1);
        assert_eq!(origin.collection, Some("users".to_string()));
        assert_eq!(origin.original_path, Some("age".to_string()));
    }

    #[test]
    fn traces_column_through_simple_project() {
        // Project that reorders columns: select age, name from users
        let relation = Relation::Project {
            input: Box::new(Relation::From {
                collection: "users".into(),
                columns: vec!["name".into(), "age".into(), "email".into()],
                arguments: Default::default(),
            }),
            exprs: vec![
                RelationalExpression::Column { index: 1 }, // age
                RelationalExpression::Column { index: 0 }, // name
            ],
        };

        // col_0 in project output = age
        let origin = trace_column_origin(&relation, 0);
        assert_eq!(origin.original_path, Some("age".to_string()));

        // col_1 in project output = name
        let origin = trace_column_origin(&relation, 1);
        assert_eq!(origin.original_path, Some("name".to_string()));
    }

    #[test]
    fn computed_column_has_no_origin() {
        // Project with computed expression
        let relation = Relation::Project {
            input: Box::new(Relation::From {
                collection: "products".into(),
                columns: vec!["price".into(), "quantity".into()],
                arguments: Default::default(),
            }),
            exprs: vec![
                RelationalExpression::Column { index: 0 },
                RelationalExpression::Multiply {
                    left: Box::new(RelationalExpression::Column { index: 0 }),
                    right: Box::new(RelationalExpression::Column { index: 1 }),
                },
            ],
        };

        // col_0 = price (original field)
        let origin = trace_column_origin(&relation, 0);
        assert!(origin.is_original_field());
        assert_eq!(origin.original_path, Some("price".to_string()));

        // col_1 = price * quantity (computed)
        let origin = trace_column_origin(&relation, 1);
        assert!(!origin.is_original_field());
        assert_eq!(origin.original_path, None);
    }

    #[test]
    fn traces_through_nested_projects() {
        // Two levels of projection
        let relation = Relation::Project {
            input: Box::new(Relation::Project {
                input: Box::new(Relation::From {
                    collection: "data".into(),
                    columns: vec!["a".into(), "b".into(), "c".into()],
                    arguments: Default::default(),
                }),
                exprs: vec![
                    RelationalExpression::Column { index: 2 }, // c -> col_0
                    RelationalExpression::Column { index: 0 }, // a -> col_1
                ],
            }),
            exprs: vec![
                RelationalExpression::Column { index: 1 }, // col_1 (a) -> col_0
            ],
        };

        let origin = trace_column_origin(&relation, 0);
        assert_eq!(origin.original_path, Some("a".to_string()));
        assert_eq!(origin.collection, Some("data".to_string()));
    }

    #[test]
    fn count_columns_for_various_relations() {
        let from = Relation::From {
            collection: "t".into(),
            columns: vec!["a".into(), "b".into(), "c".into()],
            arguments: Default::default(),
        };
        assert_eq!(column_count(&from), 3);

        let project = Relation::Project {
            input: Box::new(from.clone()),
            exprs: vec![
                RelationalExpression::Column { index: 0 },
                RelationalExpression::Column { index: 1 },
            ],
        };
        assert_eq!(column_count(&project), 2);

        let filter = Relation::Filter {
            input: Box::new(project),
            predicate: RelationalExpression::Literal {
                literal: ndc_models::RelationalLiteral::Boolean { value: true },
            },
        };
        assert_eq!(column_count(&filter), 2);
    }
}
