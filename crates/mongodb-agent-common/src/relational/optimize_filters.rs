//! Filter optimization for relational queries.
//!
//! This module provides functionality to extract early `$match` stages from
//! filter predicates when the columns trace back to original field names.
//! This enables MongoDB to use indexes on those fields.

use mongodb::bson::{doc, Bson, Document};
use ndc_models::{Relation, RelationalExpression};

use crate::mongo_query_plan::MongoConfiguration;

use super::{
    column_origin::trace_column_origin,
    type_lookup::{literal_to_bson_with_field_type, lookup_field_type},
};

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
    extract_early_match_with_config(relation, None)
}

pub fn extract_early_match_with_config(
    relation: &Relation,
    config: Option<&MongoConfiguration>,
) -> EarlyMatchResult {
    // Walk down to find Filter nodes and analyze their predicates
    match relation {
        Relation::Filter { input, predicate } => {
            // Try to convert the predicate to a query document using original field paths
            if let Some(doc) = try_make_early_query_document(input, predicate, config) {
                return EarlyMatchResult::with_query(doc);
            }
            // If this filter can't be optimized, check the input
            extract_early_match_with_config(input, config)
        }
        Relation::Sort { input, .. }
        | Relation::Paginate { input, .. }
        | Relation::Project { input, .. } => extract_early_match_with_config(input, config),
        Relation::Aggregate { input, .. } => extract_early_match_with_config(input, config),
        Relation::Window { input, .. } => extract_early_match_with_config(input, config),
        Relation::Join { left, .. } => {
            // The root pipeline runs against the left/root collection only.
            // Right-side filters must stay inside the $lookup pipeline; prepending
            // them to the root collection would apply the predicate to the wrong
            // collection and can eliminate all rows.
            extract_early_match_with_config(left, config)
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
    config: Option<&MongoConfiguration>,
) -> Option<Document> {
    let root_collection = root_collection_name(input)?;

    match predicate {
        // Binary comparisons can be converted if the left side is a column that traces to an original field
        RelationalExpression::Eq { left, right }
        | RelationalExpression::Gt { left, right }
        | RelationalExpression::GtEq { left, right }
        | RelationalExpression::Lt { left, right }
        | RelationalExpression::LtEq { left, right }
        | RelationalExpression::NotEq { left, right } => {
            try_make_comparison_query(input, &root_collection, left, right, predicate, config)
        }

        // AND: both sub-expressions must be convertible
        RelationalExpression::And { left, right } => {
            let left_doc = try_make_early_query_document(input, left, config)?;
            let right_doc = try_make_early_query_document(input, right, config)?;
            Some(doc! { "$and": [left_doc, right_doc] })
        }

        // OR: both sub-expressions must be convertible
        RelationalExpression::Or { left, right } => {
            let left_doc = try_make_early_query_document(input, left, config)?;
            let right_doc = try_make_early_query_document(input, right, config)?;
            Some(doc! { "$or": [left_doc, right_doc] })
        }

        // IsNull check
        RelationalExpression::IsNull { expr } => {
            let field_path = trace_expression_to_path(input, expr, &root_collection)?;
            Some(doc! { field_path: { "$eq": Bson::Null } })
        }

        // IsNotNull check
        RelationalExpression::IsNotNull { expr } => {
            let field_path = trace_expression_to_path(input, expr, &root_collection)?;
            Some(doc! { field_path: { "$ne": Bson::Null } })
        }

        // Other expressions cannot be converted to early match
        _ => None,
    }
}

/// Try to make a comparison query document from a binary comparison expression.
fn try_make_comparison_query(
    input: &Relation,
    root_collection: &str,
    left: &RelationalExpression,
    right: &RelationalExpression,
    original_expr: &RelationalExpression,
    config: Option<&MongoConfiguration>,
) -> Option<Document> {
    // Left side should be a field reference (Column or GetField chain)
    let field_path = trace_expression_to_path(input, left, root_collection)?;

    // Right side should be a literal
    let value = literal_to_bson(right, root_collection, &field_path, config)?;

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
fn trace_expression_to_path(
    input: &Relation,
    expr: &RelationalExpression,
    root_collection: &str,
) -> Option<String> {
    match expr {
        RelationalExpression::Column { index } => {
            let origin = trace_column_origin(input, *index);
            if origin.collection.as_deref() == Some(root_collection) {
                origin.original_path
            } else {
                None
            }
        }
        RelationalExpression::GetField { column, field } => {
            let base_path = trace_expression_to_path(input, column, root_collection)?;
            Some(format!("{}.{}", base_path, field))
        }
        _ => None,
    }
}

fn root_collection_name(relation: &Relation) -> Option<String> {
    match relation {
        Relation::From { collection, .. } => Some(collection.to_string()),
        Relation::Filter { input, .. }
        | Relation::Sort { input, .. }
        | Relation::Paginate { input, .. }
        | Relation::Project { input, .. }
        | Relation::Aggregate { input, .. }
        | Relation::Window { input, .. } => root_collection_name(input),
        Relation::Join { left, .. } => root_collection_name(left),
        Relation::Union { .. } => None,
    }
}

/// Convert a RelationalExpression::Literal to Bson.
fn literal_to_bson(
    expr: &RelationalExpression,
    collection: &str,
    field_path: &str,
    config: Option<&MongoConfiguration>,
) -> Option<Bson> {
    let RelationalExpression::Literal { literal } = expr else {
        return None;
    };

    let field_type =
        config.and_then(|cfg| lookup_field_type(cfg, collection, field_path));

    literal_to_bson_with_field_type(literal, field_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndc_models::{Float64, RelationalLiteral};

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

    #[test]
    fn does_not_extract_filter_on_right_side_of_join() {
        let relation = Relation::Filter {
            input: Box::new(Relation::Join {
                left: Box::new(Relation::From {
                    collection: "orders".into(),
                    columns: vec!["product_id".into(), "quantity".into()],
                    arguments: Default::default(),
                }),
                right: Box::new(Relation::From {
                    collection: "products".into(),
                    columns: vec!["_id".into(), "price".into()],
                    arguments: Default::default(),
                }),
                on: vec![ndc_models::JoinOn {
                    left: RelationalExpression::Column { index: 0 },
                    right: RelationalExpression::Column { index: 0 },
                }],
                join_type: ndc_models::JoinType::Inner,
            }),
            predicate: RelationalExpression::Gt {
                left: Box::new(RelationalExpression::Column { index: 3 }),
                right: Box::new(RelationalExpression::Literal {
                    literal: RelationalLiteral::Float64 {
                        value: Float64(10.0),
                    },
                }),
            },
        };

        let result = extract_early_match(&relation);
        assert!(result.query_document.is_none());
    }

    #[test]
    fn does_not_extract_embedded_right_side_filter_from_join() {
        let relation = Relation::Join {
            left: Box::new(Relation::From {
                collection: "orders".into(),
                columns: vec!["product_id".into(), "quantity".into()],
                arguments: Default::default(),
            }),
            right: Box::new(Relation::Filter {
                input: Box::new(Relation::From {
                    collection: "products".into(),
                    columns: vec!["_id".into(), "price".into()],
                    arguments: Default::default(),
                }),
                predicate: RelationalExpression::Gt {
                    left: Box::new(RelationalExpression::Column { index: 1 }),
                    right: Box::new(RelationalExpression::Literal {
                        literal: RelationalLiteral::Float64 {
                            value: Float64(10.0),
                        },
                    }),
                },
            }),
            on: vec![ndc_models::JoinOn {
                left: RelationalExpression::Column { index: 0 },
                right: RelationalExpression::Column { index: 0 },
            }],
            join_type: ndc_models::JoinType::Inner,
        };

        let result = extract_early_match(&relation);
        assert!(result.query_document.is_none());
    }
}
