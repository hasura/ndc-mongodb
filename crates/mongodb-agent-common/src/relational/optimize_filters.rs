//! Filter optimization for relational queries.
//!
//! This module provides functionality to extract early `$match` stages from
//! filter predicates when the columns trace back to original field names.
//! This enables MongoDB to use indexes on those fields.

use mongodb::bson::{doc, Bson, Document};
use ndc_models::{Float32, Float64, Relation, RelationalExpression, RelationalLiteral};

use super::column_origin::trace_column_origin;

/// Result of extracting an early match from a relation.
#[derive(Debug)]
pub struct EarlyMatchResult {
    /// The MongoDB query document for the early match stage.
    /// Uses original field paths that MongoDB can use with indexes.
    pub query_document: Option<Document>,
}

impl EarlyMatchResult {
    /// Create an empty result (no early match possible).
    pub fn empty() -> Self {
        Self {
            query_document: None,
        }
    }

    /// Create a result with a query document.
    pub fn with_query(doc: Document) -> Self {
        Self {
            query_document: Some(doc),
        }
    }
}

/// Extract an early match stage from the relation tree.
///
/// This function analyzes the relation tree to find Filter nodes where
/// the predicate columns trace back to original field names from the base
/// collection. For such predicates, it generates a query document that
/// can be used as an early `$match` stage before any projections.
///
/// # Arguments
/// * `relation` - The relation tree to analyze
///
/// # Returns
/// An `EarlyMatchResult` containing the query document if extraction is possible.
pub fn extract_early_match(relation: &Relation) -> EarlyMatchResult {
    // Walk down to find Filter nodes and analyze their predicates
    match relation {
        Relation::Filter { input, predicate } => {
            // Try to convert the predicate to a query document using original field paths
            if let Some(doc) = try_make_early_query_document(input, predicate) {
                return EarlyMatchResult::with_query(doc);
            }
            // If this filter can't be optimized, check the input
            extract_early_match(input)
        }
        Relation::Sort { input, .. }
        | Relation::Paginate { input, .. }
        | Relation::Project { input, .. } => extract_early_match(input),
        Relation::Aggregate { input, .. } => extract_early_match(input),
        Relation::Window { input, .. } => extract_early_match(input),
        Relation::Join { left, right, .. } => {
            // Try left side first, then right side
            let left_result = extract_early_match(left);
            if left_result.query_document.is_some() {
                return left_result;
            }
            extract_early_match(right)
        }
        Relation::Union { relations: _ } => {
            // Try to find a common early match across all branches
            // For now, don't optimize unions
            EarlyMatchResult::empty()
        }
        Relation::From { .. } => EarlyMatchResult::empty(),
    }
}

/// Try to convert a relational predicate to a query document using original field paths.
///
/// Returns `None` if the predicate cannot be expressed as an index-friendly query document.
fn try_make_early_query_document(
    input: &Relation,
    predicate: &RelationalExpression,
) -> Option<Document> {
    match predicate {
        // Binary comparisons can be converted if the left side is a column that traces to an original field
        RelationalExpression::Eq { left, right }
        | RelationalExpression::Gt { left, right }
        | RelationalExpression::GtEq { left, right }
        | RelationalExpression::Lt { left, right }
        | RelationalExpression::LtEq { left, right }
        | RelationalExpression::NotEq { left, right } => {
            try_make_comparison_query(input, left, right, predicate)
        }

        // AND: both sub-expressions must be convertible
        RelationalExpression::And { left, right } => {
            let left_doc = try_make_early_query_document(input, left)?;
            let right_doc = try_make_early_query_document(input, right)?;
            Some(doc! { "$and": [left_doc, right_doc] })
        }

        // OR: both sub-expressions must be convertible
        RelationalExpression::Or { left, right } => {
            let left_doc = try_make_early_query_document(input, left)?;
            let right_doc = try_make_early_query_document(input, right)?;
            Some(doc! { "$or": [left_doc, right_doc] })
        }

        // IsNull check
        RelationalExpression::IsNull { expr } => {
            let field_path = trace_expression_to_path(input, expr)?;
            Some(doc! { field_path: { "$eq": Bson::Null } })
        }

        // IsNotNull check
        RelationalExpression::IsNotNull { expr } => {
            let field_path = trace_expression_to_path(input, expr)?;
            Some(doc! { field_path: { "$ne": Bson::Null } })
        }

        // Other expressions cannot be converted to early match
        _ => None,
    }
}

/// Try to make a comparison query document from a binary comparison expression.
fn try_make_comparison_query(
    input: &Relation,
    left: &RelationalExpression,
    right: &RelationalExpression,
    original_expr: &RelationalExpression,
) -> Option<Document> {
    // Left side should be a field reference (Column or GetField chain)
    let field_path = trace_expression_to_path(input, left)?;

    // Right side should be a literal
    let value = literal_to_bson(right)?;

    // Generate the appropriate comparison operator
    let operator = match original_expr {
        RelationalExpression::Eq { .. } => "$eq",
        RelationalExpression::NotEq { .. } => "$ne",
        RelationalExpression::Gt { .. } => "$gt",
        RelationalExpression::GtEq { .. } => "$gte",
        RelationalExpression::Lt { .. } => "$lt",
        RelationalExpression::LtEq { .. } => "$lte",
        _ => return None,
    };

    Some(doc! { field_path: { operator: value } })
}

/// Trace an expression to its original field path.
///
/// Handles:
/// - `Column { index }` -> traces through the relation tree
/// - `GetField { column, field }` -> recursively traces and appends field names
///
/// Returns `None` if the expression doesn't trace to an original field.
fn trace_expression_to_path(input: &Relation, expr: &RelationalExpression) -> Option<String> {
    match expr {
        RelationalExpression::Column { index } => {
            let origin = trace_column_origin(input, *index);
            origin.original_path
        }
        RelationalExpression::GetField { column, field } => {
            let base_path = trace_expression_to_path(input, column)?;
            Some(format!("{}.{}", base_path, field))
        }
        _ => None,
    }
}

/// Convert a RelationalExpression::Literal to Bson.
fn literal_to_bson(expr: &RelationalExpression) -> Option<Bson> {
    let RelationalExpression::Literal { literal } = expr else {
        return None;
    };

    match literal {
        RelationalLiteral::Boolean { value } => Some(Bson::Boolean(*value)),
        RelationalLiteral::String { value } => Some(Bson::String(value.clone())),
        RelationalLiteral::Int8 { value } => Some(Bson::Int32(*value as i32)),
        RelationalLiteral::Int16 { value } => Some(Bson::Int32(*value as i32)),
        RelationalLiteral::Int32 { value } => Some(Bson::Int32(*value)),
        RelationalLiteral::Int64 { value } => Some(Bson::Int64(*value)),
        RelationalLiteral::Float32 { value: Float32(v) } => Some(Bson::Double(f64::from(*v))),
        RelationalLiteral::Float64 { value: Float64(v) } => Some(Bson::Double(*v)),
        RelationalLiteral::Null => Some(Bson::Null),
        // For other types like Date, Timestamp, etc., we'd need conversion
        // For now, return None and let the regular $expr path handle them
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_simple_comparison_from_filter() {
        // Filter on age > 18 directly on From
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

        let result = extract_early_match(&relation);
        assert!(result.query_document.is_some());

        let doc = result.query_document.unwrap();
        assert_eq!(doc, doc! { "age": { "$gt": 18_i64 } });
    }

    #[test]
    fn extracts_comparison_through_project() {
        // Project then Filter - should still trace back to original field
        let relation = Relation::Filter {
            input: Box::new(Relation::Project {
                input: Box::new(Relation::From {
                    collection: "users".into(),
                    columns: vec!["name".into(), "age".into(), "email".into()],
                    arguments: Default::default(),
                }),
                exprs: vec![
                    RelationalExpression::Column { index: 1 }, // age -> col_0
                    RelationalExpression::Column { index: 0 }, // name -> col_1
                ],
            }),
            predicate: RelationalExpression::GtEq {
                left: Box::new(RelationalExpression::Column { index: 0 }), // col_0 = age
                right: Box::new(RelationalExpression::Literal {
                    literal: RelationalLiteral::Int64 { value: 21 },
                }),
            },
        };

        let result = extract_early_match(&relation);
        assert!(result.query_document.is_some());

        let doc = result.query_document.unwrap();
        assert_eq!(doc, doc! { "age": { "$gte": 21_i64 } });
    }

    #[test]
    fn extracts_and_expression() {
        let relation = Relation::Filter {
            input: Box::new(Relation::From {
                collection: "users".into(),
                columns: vec!["name".into(), "age".into()],
                arguments: Default::default(),
            }),
            predicate: RelationalExpression::And {
                left: Box::new(RelationalExpression::GtEq {
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

        let result = extract_early_match(&relation);
        assert!(result.query_document.is_some());

        let doc = result.query_document.unwrap();
        assert_eq!(
            doc,
            doc! {
                "$and": [
                    { "age": { "$gte": 18_i64 } },
                    { "age": { "$lt": 65_i64 } }
                ]
            }
        );
    }

    #[test]
    fn returns_none_for_computed_column() {
        // Filter on a computed column (price * quantity)
        let relation = Relation::Filter {
            input: Box::new(Relation::Project {
                input: Box::new(Relation::From {
                    collection: "products".into(),
                    columns: vec!["price".into(), "quantity".into()],
                    arguments: Default::default(),
                }),
                exprs: vec![RelationalExpression::Multiply {
                    left: Box::new(RelationalExpression::Column { index: 0 }),
                    right: Box::new(RelationalExpression::Column { index: 1 }),
                }],
            }),
            predicate: RelationalExpression::Gt {
                left: Box::new(RelationalExpression::Column { index: 0 }),
                right: Box::new(RelationalExpression::Literal {
                    literal: RelationalLiteral::Int64 { value: 100 },
                }),
            },
        };

        let result = extract_early_match(&relation);
        assert!(result.query_document.is_none());
    }

    #[test]
    fn extracts_string_comparison() {
        let relation = Relation::Filter {
            input: Box::new(Relation::From {
                collection: "users".into(),
                columns: vec!["status".into()],
                arguments: Default::default(),
            }),
            predicate: RelationalExpression::Eq {
                left: Box::new(RelationalExpression::Column { index: 0 }),
                right: Box::new(RelationalExpression::Literal {
                    literal: RelationalLiteral::String {
                        value: "active".to_string(),
                    },
                }),
            },
        };

        let result = extract_early_match(&relation);
        assert!(result.query_document.is_some());

        let doc = result.query_document.unwrap();
        assert_eq!(doc, doc! { "status": { "$eq": "active" } });
    }
}
