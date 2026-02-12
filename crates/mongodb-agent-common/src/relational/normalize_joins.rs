//! Join normalization - transforms right joins into left joins with swapped inputs.
//!
//! This module provides a transformation pass that converts Right, RightSemi, and RightAnti
//! joins into their Left equivalents by swapping the inputs and adjusting column indices.
//! This allows us to support all right join types using the existing left join implementation.

use ndc_models::{JoinOn, JoinType, Relation, RelationalExpression};

/// Normalize right joins to left joins by swapping inputs.
///
/// This transforms:
/// - `RightJoin(L, R)` → `Project(LeftJoin(R, L), [reorder columns])`
/// - `RightSemi(L, R)` → `LeftSemi(R, L)` (no reorder needed, only right columns output)
/// - `RightAnti(L, R)` → `LeftAnti(R, L)` (no reorder needed, only right columns output)
pub fn normalize_right_joins(relation: &Relation) -> Relation {
    match relation {
        Relation::Join {
            left,
            right,
            on,
            join_type,
        } if matches!(
            join_type,
            JoinType::Right | JoinType::RightSemi | JoinType::RightAnti
        ) =>
        {
            let left_count = column_count(left);
            let right_count = column_count(right);

            // Recursively normalize children first
            let normalized_left = normalize_right_joins(left);
            let normalized_right = normalize_right_joins(right);

            // Swap the join conditions
            let swapped_on = swap_join_on(on);

            // Convert join type
            let swapped_type = match join_type {
                JoinType::Right => JoinType::Left,
                JoinType::RightSemi => JoinType::LeftSemi,
                JoinType::RightAnti => JoinType::LeftAnti,
                _ => unreachable!(),
            };

            tracing::debug!(
                original_join_type = ?join_type,
                new_join_type = ?swapped_type,
                left_columns = left_count,
                right_columns = right_count,
                "transforming right join to left join with swapped inputs"
            );

            // Build the swapped join
            let swapped_join = Relation::Join {
                left: Box::new(normalized_right), // Swapped!
                right: Box::new(normalized_left), // Swapped!
                on: swapped_on,
                join_type: swapped_type.clone(),
            };

            // For semi/anti joins, only one side's columns are output, so no reorder needed
            if matches!(swapped_type, JoinType::LeftSemi | JoinType::LeftAnti) {
                swapped_join
            } else {
                // For full Right join, we need to reorder columns back to [L, R] order
                Relation::Project {
                    input: Box::new(swapped_join),
                    exprs: reorder_columns(left_count, right_count),
                }
            }
        }

        // Recursively normalize other relation types
        Relation::From { .. } => relation.clone(),

        Relation::Filter { input, predicate } => Relation::Filter {
            input: Box::new(normalize_right_joins(input)),
            predicate: predicate.clone(),
        },

        Relation::Sort { input, exprs } => Relation::Sort {
            input: Box::new(normalize_right_joins(input)),
            exprs: exprs.clone(),
        },

        Relation::Paginate { input, fetch, skip } => Relation::Paginate {
            input: Box::new(normalize_right_joins(input)),
            fetch: *fetch,
            skip: *skip,
        },

        Relation::Project { input, exprs } => Relation::Project {
            input: Box::new(normalize_right_joins(input)),
            exprs: exprs.clone(),
        },

        Relation::Join {
            left,
            right,
            on,
            join_type,
        } => Relation::Join {
            left: Box::new(normalize_right_joins(left)),
            right: Box::new(normalize_right_joins(right)),
            on: on.clone(),
            join_type: join_type.clone(),
        },

        Relation::Aggregate {
            input,
            group_by,
            aggregates,
        } => Relation::Aggregate {
            input: Box::new(normalize_right_joins(input)),
            group_by: group_by.clone(),
            aggregates: aggregates.clone(),
        },

        Relation::Window { input, exprs } => Relation::Window {
            input: Box::new(normalize_right_joins(input)),
            exprs: exprs.clone(),
        },

        Relation::Union { relations } => Relation::Union {
            relations: relations.iter().map(normalize_right_joins).collect(),
        },
    }
}

/// Count the number of output columns for a relation.
pub fn column_count(relation: &Relation) -> u64 {
    match relation {
        Relation::From { columns, .. } => columns.len() as u64,

        Relation::Project { exprs, .. } => exprs.len() as u64,

        Relation::Filter { input, .. }
        | Relation::Sort { input, .. }
        | Relation::Paginate { input, .. } => column_count(input),

        Relation::Aggregate {
            group_by,
            aggregates,
            ..
        } => (group_by.len() + aggregates.len()) as u64,

        Relation::Join {
            left,
            right,
            join_type,
            ..
        } => {
            match join_type {
                // Semi/anti joins only output left side columns
                JoinType::LeftSemi | JoinType::LeftAnti => column_count(left),
                JoinType::RightSemi | JoinType::RightAnti => column_count(right),
                // Full joins output all columns from both sides
                _ => column_count(left) + column_count(right),
            }
        }

        Relation::Window { input, exprs, .. } => column_count(input) + exprs.len() as u64,

        Relation::Union { relations } => {
            // All inputs should have the same column count, use first
            relations.first().map(column_count).unwrap_or(0)
        }
    }
}

/// Swap the join conditions when swapping left and right inputs.
///
/// In JoinOn, the `left` expression uses column indices relative to the left input (0..left_count),
/// and the `right` expression uses column indices relative to the right input (0..right_count).
/// When we swap the inputs, we just need to swap which expression is left vs right.
fn swap_join_on(on: &[JoinOn]) -> Vec<JoinOn> {
    on.iter()
        .map(|join_on| {
            JoinOn {
                // Original right becomes new left
                left: join_on.right.clone(),
                // Original left becomes new right
                right: join_on.left.clone(),
            }
        })
        .collect()
}

/// Generate projection expressions to reorder columns from [R, L] back to [L, R].
///
/// After swap, the layout is [right cols (0..R), left cols (R..R+L)]
/// We need output [left cols, right cols]
fn reorder_columns(left_count: u64, right_count: u64) -> Vec<RelationalExpression> {
    let mut exprs = Vec::with_capacity((left_count + right_count) as usize);

    // First, emit left columns (now at positions R..R+L)
    for i in 0..left_count {
        exprs.push(RelationalExpression::Column {
            index: right_count + i,
        });
    }
    // Then, emit right columns (now at positions 0..R)
    for i in 0..right_count {
        exprs.push(RelationalExpression::Column { index: i });
    }

    exprs
}
