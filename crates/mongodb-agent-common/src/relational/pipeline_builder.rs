//! Pipeline builder for relational queries.
//!
//! This module translates relational query nodes into MongoDB aggregation pipelines.

use std::collections::BTreeMap;

use mongodb::bson::{bson, doc, Bson, Document};
use mongodb_support::aggregate::{Accumulator, Pipeline, SortDocument, Stage};
use ndc_models::{
    JoinOn, JoinType, NullsSort, OrderDirection, Relation, RelationalExpression, Sort,
};

use crate::mongo_query_plan::MongoConfiguration;

use super::{
    expression::{translate_aggregate_expression, translate_expression, ExpressionContext},
    normalize_joins::normalize_right_joins,
    optimize_filters::extract_early_match_with_config,
    pushdown_predicates::pushdown_predicates,
    type_lookup::{literal_to_bson_with_field_type, lookup_field_type},
    ColumnMapping, RelationalError, RelationalPipelineResult,
};

/// Build a MongoDB aggregation pipeline from a relational query.
///
/// This function applies several optimizations before building the pipeline:
/// 1. Normalizes right joins (Right, RightSemi, RightAnti) into left joins
/// 2. Pushes predicates down through projections when possible
/// 3. Extracts early `$match` stages with original field names for index usage
pub fn build_relational_pipeline(
    relation: &Relation,
) -> Result<RelationalPipelineResult, RelationalError> {
    build_relational_pipeline_with_config(relation, None)
}

pub fn build_relational_pipeline_with_config(
    relation: &Relation,
    config: Option<&MongoConfiguration>,
) -> Result<RelationalPipelineResult, RelationalError> {
    // Step 1: Normalize right joins to left joins
    let normalized_relation = normalize_right_joins(relation);

    // Step 2: Push predicates down through projections
    let optimized_relation = pushdown_predicates(&normalized_relation);

    // Step 3: Extract early match optimization - generate index-friendly $match if possible
    let early_match = extract_early_match_with_config(&optimized_relation, config);

    let mut ctx = PipelineContext::new(config);
    build_relation(&optimized_relation, &mut ctx)?;

    let collection = ctx.collection.ok_or(RelationalError::NoCollection)?;

    // Prepend early match stage if we found one
    let mut stages = ctx.stages;
    if let Some(query_doc) = early_match.query_document {
        stages.insert(0, Stage::Match(query_doc));
    }

    Ok(RelationalPipelineResult {
        collection,
        pipeline: Pipeline::new(stages),
        output_columns: ctx.column_mapping,
    })
}

/// Context used while building the pipeline.
struct PipelineContext<'a> {
    /// Connector configuration used to resolve field types for literal coercion.
    config: Option<&'a MongoConfiguration>,
    /// The collection to query (set by From relation).
    collection: Option<String>,
    /// Current column mapping.
    column_mapping: ColumnMapping,
    /// Accumulated pipeline stages.
    stages: Vec<Stage>,
}

impl<'a> PipelineContext<'a> {
    fn new(config: Option<&'a MongoConfiguration>) -> Self {
        Self {
            config,
            collection: None,
            column_mapping: ColumnMapping::default(),
            stages: Vec::new(),
        }
    }
}

/// Recursively build the pipeline from a relation node.
fn build_relation(
    relation: &Relation,
    ctx: &mut PipelineContext<'_>,
) -> Result<(), RelationalError> {
    match relation {
        Relation::From {
            collection,
            columns,
            arguments,
        } => {
            if !arguments.is_empty() {
                return Err(RelationalError::UnsupportedRelation(
                    "From with arguments is not supported".to_string(),
                ));
            }
            build_from(collection, columns, ctx)
        }

        Relation::Filter { input, predicate } => {
            build_relation(input, ctx)?;
            build_filter(predicate, ctx)
        }

        Relation::Sort { input, exprs } => {
            build_relation(input, ctx)?;
            build_sort(exprs, ctx)
        }

        Relation::Paginate { input, fetch, skip } => {
            build_relation(input, ctx)?;
            build_paginate(*fetch, *skip, ctx)
        }

        Relation::Project { input, exprs } => build_project(input, exprs, ctx),
        Relation::Join {
            left,
            right,
            on,
            join_type,
        } => build_join(left, right, on, join_type, ctx),
        Relation::Aggregate {
            input,
            group_by,
            aggregates,
        } => {
            build_relation(input, ctx)?;
            build_aggregate(group_by, aggregates, ctx)
        }
        Relation::Window { input, exprs } => {
            build_relation(input, ctx)?;
            build_window(exprs, ctx)
        }
        Relation::Union { relations } => build_union(relations, ctx),
    }
}

/// Build the From relation (collection scan).
fn build_from(
    collection: &ndc_models::CollectionName,
    columns: &[ndc_models::FieldName],
    ctx: &mut PipelineContext<'_>,
) -> Result<(), RelationalError> {
    ctx.collection = Some(collection.to_string());
    ctx.column_mapping = ColumnMapping::new(columns.iter().map(|c| c.as_str()));
    // No pipeline stages needed - collection scan is implicit
    Ok(())
}

/// Build the Filter relation ($match stage).
///
/// This function first tries to generate an index-friendly query document
/// (e.g., `{ field: { $gt: value } }`). If that's not possible, it falls
/// back to using `$expr` which is less index-friendly.
fn build_filter(
    predicate: &RelationalExpression,
    ctx: &mut PipelineContext<'_>,
) -> Result<(), RelationalError> {
    // Try to generate a query document first (more index-friendly)
    if let Some(query_doc) = try_make_query_document(
        predicate,
        &ctx.column_mapping,
        ctx.collection.as_deref(),
        ctx.config,
    ) {
        ctx.stages.push(Stage::Match(query_doc));
        return Ok(());
    }

    // Fall back to $expr (less index-friendly)
    let expr_ctx = ExpressionContext::new(&ctx.column_mapping);
    let match_expr = translate_expression(predicate, &expr_ctx)?;
    ctx.stages.push(Stage::Match(doc! { "$expr": match_expr }));

    Ok(())
}

/// Try to generate an index-friendly query document for a predicate.
///
/// Returns `Some(document)` if the predicate can be expressed as a query document,
/// `None` if we need to fall back to `$expr`.
fn try_make_query_document(
    predicate: &RelationalExpression,
    column_mapping: &ColumnMapping,
    collection: Option<&str>,
    config: Option<&MongoConfiguration>,
) -> Option<Document> {
    match predicate {
        // Binary comparisons: column/getfield op literal
        RelationalExpression::Eq { left, right }
        | RelationalExpression::NotEq { left, right }
        | RelationalExpression::Lt { left, right }
        | RelationalExpression::LtEq { left, right }
        | RelationalExpression::Gt { left, right }
        | RelationalExpression::GtEq { left, right } => {
            // Left side should be a field reference (Column or GetField chain)
            let field_path = extract_field_path(left, column_mapping)?;

            // Right side should be a literal
            let value = literal_to_bson(right, collection, config, &field_path)?;

            // Determine the operator
            let operator = match predicate {
                RelationalExpression::Eq { .. } => "$eq",
                RelationalExpression::NotEq { .. } => "$ne",
                RelationalExpression::Lt { .. } => "$lt",
                RelationalExpression::LtEq { .. } => "$lte",
                RelationalExpression::Gt { .. } => "$gt",
                RelationalExpression::GtEq { .. } => "$gte",
                _ => return None,
            };

            Some(doc! { field_path: { operator: value } })
        }

        // Logical AND: all sub-expressions must be convertible
        RelationalExpression::And { left, right } => {
            let left_doc = try_make_query_document(left, column_mapping, collection, config)?;
            let right_doc = try_make_query_document(right, column_mapping, collection, config)?;
            Some(doc! { "$and": [left_doc, right_doc] })
        }

        // Logical OR: all sub-expressions must be convertible
        RelationalExpression::Or { left, right } => {
            let left_doc = try_make_query_document(left, column_mapping, collection, config)?;
            let right_doc = try_make_query_document(right, column_mapping, collection, config)?;
            Some(doc! { "$or": [left_doc, right_doc] })
        }

        // IsNull
        RelationalExpression::IsNull { expr } => {
            let field_path = extract_field_path(expr, column_mapping)?;
            Some(doc! { field_path: { "$eq": Bson::Null } })
        }

        // IsNotNull
        RelationalExpression::IsNotNull { expr } => {
            let field_path = extract_field_path(expr, column_mapping)?;
            Some(doc! { field_path: { "$ne": Bson::Null } })
        }

        // For other expression types, fall back to $expr
        _ => None,
    }
}

/// Extract a field path from an expression.
///
/// Handles:
/// - `Column { index }` -> returns the mapped field name
/// - `GetField { column, field }` -> recursively builds path like "parent.child.field"
///
/// Returns `None` if the expression is not a simple field reference.
fn extract_field_path(
    expr: &RelationalExpression,
    column_mapping: &ColumnMapping,
) -> Option<String> {
    match expr {
        RelationalExpression::Column { index } => column_mapping
            .field_for_index(*index)
            .map(|s| s.to_string()),
        RelationalExpression::GetField { column, field } => {
            let base_path = extract_field_path(column, column_mapping)?;
            Some(format!("{}.{}", base_path, field))
        }
        _ => None,
    }
}

/// Convert a literal RelationalExpression to Bson.
fn literal_to_bson(
    expr: &RelationalExpression,
    collection: Option<&str>,
    config: Option<&MongoConfiguration>,
    field_path: &str,
) -> Option<Bson> {
    let RelationalExpression::Literal { literal } = expr else {
        return None;
    };

    let field_type =
        collection.and_then(|name| config.and_then(|cfg| lookup_field_type(cfg, name, field_path)));

    literal_to_bson_with_field_type(literal, field_type)
}

/// Check if an expression is a simple field reference (Column or GetField chain on a Column).
///
/// Simple field references can be used directly in $sort stages.
fn is_simple_field_expr(expr: &RelationalExpression) -> bool {
    match expr {
        RelationalExpression::Column { .. } => true,
        RelationalExpression::GetField { column, .. } => is_simple_field_expr(column),
        _ => false,
    }
}

/// Check if a sort's nulls_sort requires a non-default null ordering.
///
/// MongoDB default: nulls sort first in ascending order, last in descending order.
/// Returns true if we need to add an explicit null-ordering key.
fn needs_null_sort_key(direction: &OrderDirection, nulls_sort: &NullsSort) -> bool {
    matches!(
        (direction, nulls_sort),
        (OrderDirection::Asc, NullsSort::NullsLast)
            | (OrderDirection::Desc, NullsSort::NullsFirst)
    )
}

/// Build the Sort relation ($sort stage).
///
/// For simple field references (Column or GetField chains), we sort directly on the field.
/// For complex expressions (like Case), we use $addFields to compute temporary
/// sort keys, then $sort on those keys, and finally $project to remove them.
///
/// When `nulls_sort` doesn't match MongoDB's default behavior, we add computed
/// null-ordering keys that sort nulls to the correct position.
fn build_sort(sort_exprs: &[Sort], ctx: &mut PipelineContext<'_>) -> Result<(), RelationalError> {
    let expr_ctx = ExpressionContext::new(&ctx.column_mapping);

    // Check if we have any complex expressions or non-default null sorting
    let has_complex_exprs = sort_exprs.iter().any(|s| !is_simple_field_expr(&s.expr));
    let has_non_default_nulls = sort_exprs
        .iter()
        .any(|s| needs_null_sort_key(&s.direction, &s.nulls_sort));

    // When we need null ordering keys or complex expressions, we always use
    // $addFields + $sort + $unset pattern
    if has_complex_exprs || has_non_default_nulls {
        let mut add_fields_doc = Document::new();
        let mut sort_doc = Document::new();
        let mut temp_field_names = Vec::new();

        for (i, sort) in sort_exprs.iter().enumerate() {
            let field_name = match &sort.expr {
                RelationalExpression::Column { index } => {
                    let field = ctx
                        .column_mapping
                        .field_for_index(*index)
                        .ok_or(RelationalError::InvalidColumnIndex(*index))?;
                    field.to_string()
                }
                _ => {
                    // Compute expression into a temporary field
                    let temp_field = format!("__sort_key_{}", i);
                    let expr_bson = translate_expression(&sort.expr, &expr_ctx)?;
                    add_fields_doc.insert(&temp_field, expr_bson);
                    temp_field_names.push(temp_field.clone());
                    temp_field
                }
            };

            // Add null-ordering key if needed
            if needs_null_sort_key(&sort.direction, &sort.nulls_sort) {
                let null_key = format!("__null_order_{}", i);
                // For NullsLast with Asc: nulls get 1 (sorted after 0)
                // For NullsFirst with Desc: nulls get 0 (sorted before 1 in ascending)
                let null_value = match sort.nulls_sort {
                    NullsSort::NullsLast => 1,
                    NullsSort::NullsFirst => 0,
                };
                let non_null_value = 1 - null_value;
                add_fields_doc.insert(
                    &null_key,
                    bson!({ "$cond": [{ "$eq": [format!("${}", field_name), null] }, null_value, non_null_value] }),
                );
                temp_field_names.push(null_key.clone());
                // Sort by null key first (always ascending)
                sort_doc.insert(null_key, 1);
            }

            let direction = match sort.direction {
                OrderDirection::Asc => 1,
                OrderDirection::Desc => -1,
            };
            sort_doc.insert(&field_name, direction);
        }

        // Add the $addFields stage if there are computed expressions or null keys
        if !add_fields_doc.is_empty() {
            ctx.stages.push(Stage::AddFields(add_fields_doc));
        }

        // Add the $sort stage
        ctx.stages.push(Stage::Sort(SortDocument(sort_doc)));

        // Remove temporary fields using $unset
        if !temp_field_names.is_empty() {
            let unset_doc = doc! { "$unset": temp_field_names };
            ctx.stages.push(Stage::Other(unset_doc));
        }
    } else {
        // Simple case: all expressions are field references with default null ordering
        let mut sort_doc = Document::new();

        for sort in sort_exprs {
            let field_name =
                extract_field_path(&sort.expr, &ctx.column_mapping).ok_or_else(|| {
                    RelationalError::InvalidSortExpression(format!("{:?}", sort.expr))
                })?;
            let direction = match sort.direction {
                OrderDirection::Asc => 1,
                OrderDirection::Desc => -1,
            };
            sort_doc.insert(field_name, direction);
        }

        ctx.stages.push(Stage::Sort(SortDocument(sort_doc)));
    }

    Ok(())
}

/// Build the Paginate relation ($skip and $limit stages).
fn build_paginate(
    fetch: Option<u64>,
    skip: u64,
    ctx: &mut PipelineContext<'_>,
) -> Result<(), RelationalError> {
    if skip > 0 {
        let skip_i64 = i64::try_from(skip).map_err(|_| {
            RelationalError::UnsupportedExpression("Skip value too large".to_string())
        })?;
        ctx.stages.push(Stage::Skip(Bson::Int64(skip_i64)));
    }

    if let Some(limit) = fetch {
        let limit_i64 = i64::try_from(limit).map_err(|_| {
            RelationalError::UnsupportedExpression("Limit value too large".to_string())
        })?;
        ctx.stages.push(Stage::Limit(Bson::Int64(limit_i64)));
    }

    Ok(())
}

/// Build the Project relation ($project stage).
fn build_project(
    input: &Relation,
    exprs: &[RelationalExpression],
    ctx: &mut PipelineContext<'_>,
) -> Result<(), RelationalError> {
    build_relation(input, ctx)?;

    let expr_ctx = ExpressionContext::new(&ctx.column_mapping);

    // Build $project document with positional field names
    let mut project_doc = Document::new();
    project_doc.insert("_id", 0); // Exclude _id by default

    for (i, expr) in exprs.iter().enumerate() {
        let field_name = format!("col_{}", i);
        let translated = translate_expression(expr, &expr_ctx)?;
        project_doc.insert(&field_name, translated);
    }

    ctx.stages.push(Stage::Project(project_doc));

    // Update column mapping - projection redefines all columns with positional names
    let new_mapping: Vec<String> = (0..exprs.len()).map(|i| format!("col_{}", i)).collect();
    ctx.column_mapping = ColumnMapping::new(new_mapping.iter().map(|s| s.as_str()));

    Ok(())
}

/// Extract the field name from a column reference expression.
/// Handles both `Column` and `GetField` chains (e.g., `col.nested_field`).
fn extract_column_field(
    expr: &RelationalExpression,
    column_mapping: &ColumnMapping,
) -> Result<String, RelationalError> {
    match expr {
        RelationalExpression::Column { index } => column_mapping
            .field_for_index(*index)
            .map(|s| s.to_string())
            .ok_or(RelationalError::InvalidColumnIndex(*index)),
        RelationalExpression::GetField { column, field } => {
            let base = extract_column_field(column, column_mapping)?;
            Ok(format!("{}.{}", base, field))
        }
        other => Err(RelationalError::InvalidSortExpression(format!("{other:?}"))),
    }
}

/// Build the Aggregate relation ($group and $project stages).
fn build_aggregate(
    group_by: &[RelationalExpression],
    aggregates: &[RelationalExpression],
    ctx: &mut PipelineContext<'_>,
) -> Result<(), RelationalError> {
    let expr_ctx = ExpressionContext::new(&ctx.column_mapping);

    // Build _id field based on group_by expressions
    let key_expression = match group_by.len() {
        0 => Bson::Null,
        1 => translate_expression(&group_by[0], &expr_ctx)?,
        _ => {
            // Multiple group by expressions: { _g0: expr0, _g1: expr1, ... }
            let mut id_doc = Document::new();
            for (i, expr) in group_by.iter().enumerate() {
                let key = format!("_g{}", i);
                let translated = translate_expression(expr, &expr_ctx)?;
                id_doc.insert(key, translated);
            }
            Bson::Document(id_doc)
        }
    };

    // Build accumulators for aggregate expressions
    let mut accumulators = BTreeMap::new();
    for (i, agg_expr) in aggregates.iter().enumerate() {
        let key = format!("_a{}", i);
        let accumulator =
            bson_to_accumulator(translate_aggregate_expression(agg_expr, &expr_ctx)?)?;
        accumulators.insert(key, accumulator);
    }

    ctx.stages.push(Stage::Group {
        key_expression,
        accumulators,
    });

    // Build the $project stage to remap columns to col_0, col_1, etc.
    // Some aggregates need post-processing (e.g., count distinct needs $size)
    let mut project_doc = Document::new();
    project_doc.insert("_id", 0); // Exclude _id

    let total_columns = group_by.len() + aggregates.len();

    // Group by columns come first
    for i in 0..group_by.len() {
        let field_name = format!("col_{}", i);
        let source = match group_by.len() {
            1 => "$_id".to_string(),
            _ => format!("$_id._g{}", i),
        };
        project_doc.insert(&field_name, source);
    }

    // Then aggregate columns - apply post-processing where needed
    for (i, agg_expr) in aggregates.iter().enumerate() {
        let field_name = format!("col_{}", group_by.len() + i);
        let source = format!("$_a{}", i);
        let projected_value = apply_aggregate_post_processing(agg_expr, &source)?;
        project_doc.insert(&field_name, projected_value);
    }

    ctx.stages.push(Stage::Project(project_doc));

    // Update column mapping
    let new_mapping: Vec<String> = (0..total_columns).map(|i| format!("col_{}", i)).collect();
    ctx.column_mapping = ColumnMapping::new(new_mapping.iter().map(|s| s.as_str()));

    Ok(())
}

/// Apply post-processing for aggregates that need it in the $project stage.
/// Some aggregates like count distinct use $addToSet in $group and need $size in $project.
fn apply_aggregate_post_processing(
    agg_expr: &RelationalExpression,
    source: &str,
) -> Result<Bson, RelationalError> {
    match agg_expr {
        // Count distinct: $addToSet produces array, need $size to get count
        RelationalExpression::Count { distinct: true, .. } => {
            Ok(bson!({ "$size": source }))
        }

        // BoolAnd: $push produces array, need $allElementsTrue
        RelationalExpression::BoolAnd { .. } => {
            Ok(bson!({ "$allElementsTrue": source }))
        }

        // BoolOr: $push produces array, need $anyElementTrue
        RelationalExpression::BoolOr { .. } => {
            Ok(bson!({ "$anyElementTrue": source }))
        }

        // ArrayAgg with order_by: sort the collected array using $sortArray (MongoDB 5.2+)
        RelationalExpression::ArrayAgg {
            order_by: Some(order_by),
            ..
        } if !order_by.is_empty() => {
            let sort_by = build_sort_by_for_sort_array(order_by)?;
            Ok(bson!({
                "$sortArray": {
                    "input": source,
                    "sortBy": sort_by
                }
            }))
        }

        // StringAgg: $push/$addToSet produces array, optionally sort, then $reduce + $concat
        RelationalExpression::StringAgg {
            separator,
            order_by,
            ..
        } => {
            // Build the input expression - apply sorting if order_by is specified
            let input_expr = if let Some(sorts) = order_by {
                if !sorts.is_empty() {
                    let sort_by = build_sort_by_for_sort_array(sorts)?;
                    bson!({
                        "$sortArray": {
                            "input": source,
                            "sortBy": sort_by
                        }
                    })
                } else {
                    Bson::String(source.to_string())
                }
            } else {
                Bson::String(source.to_string())
            };

            // Use $reduce to concatenate array elements with separator
            // Result: "elem1, elem2, elem3" (or empty string if array is empty)
            Ok(bson!({
                "$reduce": {
                    "input": input_expr,
                    "initialValue": "",
                    "in": {
                        "$cond": {
                            "if": { "$eq": ["$$value", ""] },
                            "then": { "$toString": "$$this" },
                            "else": { "$concat": ["$$value", separator, { "$toString": "$$this" }] }
                        }
                    }
                }
            }))
        }

        // All other aggregates: no post-processing needed
        _ => Ok(Bson::String(source.to_string())),
    }
}

/// Build a sortBy document for $sortArray from a list of Sort expressions.
/// For simple scalar aggregates, we sort by the value itself (represented as empty string key).
///
/// Multi-key sorting on scalar values is not supported by $sortArray and returns an error.
fn build_sort_by_for_sort_array(
    order_by: &[Sort],
) -> Result<Bson, RelationalError> {
    if order_by.len() == 1 {
        let sort = &order_by[0];
        Ok(match sort.direction {
            OrderDirection::Asc => Bson::Int32(1),
            OrderDirection::Desc => Bson::Int32(-1),
        })
    } else {
        Err(RelationalError::UnsupportedExpression(
            "Multi-key sorting on scalar aggregate values is not supported by $sortArray"
                .to_string(),
        ))
    }
}

/// Convert BSON aggregate expression to Accumulator.
/// The BSON is expected to be in the form { "$sum": expr }, { "$avg": expr }, etc.
fn bson_to_accumulator(bson: Bson) -> Result<Accumulator, RelationalError> {
    match bson {
        Bson::Document(doc) => {
            if let Some(value) = doc.get("$sum") {
                Ok(Accumulator::Sum(value.clone()))
            } else if let Some(value) = doc.get("$avg") {
                Ok(Accumulator::Avg(value.clone()))
            } else if let Some(value) = doc.get("$min") {
                Ok(Accumulator::Min(value.clone()))
            } else if let Some(value) = doc.get("$max") {
                Ok(Accumulator::Max(value.clone()))
            } else if let Some(value) = doc.get("$addToSet") {
                Ok(Accumulator::AddToSet(value.clone()))
            } else if let Some(value) = doc.get("$push") {
                Ok(Accumulator::Push(value.clone()))
            } else if doc.get("$count").is_some() {
                Ok(Accumulator::Count)
            } else if let Some(value) = doc.get("$first") {
                Ok(Accumulator::First(value.clone()))
            } else if let Some(value) = doc.get("$last") {
                Ok(Accumulator::Last(value.clone()))
            } else if let Some(value) = doc.get("$stdDevSamp") {
                Ok(Accumulator::StdDevSamp(value.clone()))
            } else if let Some(value) = doc.get("$stdDevPop") {
                Ok(Accumulator::StdDevPop(value.clone()))
            } else if let Some(Bson::Document(median_doc)) = doc.get("$median") {
                Ok(Accumulator::Median(median_doc.clone()))
            } else if let Some(Bson::Document(percentile_doc)) = doc.get("$percentile") {
                Ok(Accumulator::Percentile(percentile_doc.clone()))
            } else {
                Err(RelationalError::UnsupportedExpression(format!(
                    "Unknown accumulator in BSON: {doc:?}"
                )))
            }
        }
        _ => Err(RelationalError::UnsupportedExpression(format!(
            "Expected document for accumulator, got: {bson:?}"
        ))),
    }
}

/// Build a Join relation.
fn build_join(
    left: &Relation,
    right: &Relation,
    on: &[JoinOn],
    join_type: &JoinType,
    ctx: &mut PipelineContext<'_>,
) -> Result<(), RelationalError> {
    // Build left pipeline first (this sets the collection context)
    build_relation(left, ctx)?;
    let left_column_count = ctx.column_mapping.len();

    // Build the full right relation into the lookup pipeline so we preserve
    // projections, filters, casts, and any other shaping applied by the SQL planner.
    let mut right_ctx = PipelineContext::new(ctx.config);
    build_relation(right, &mut right_ctx)?;
    let right_collection = right_ctx.collection.ok_or(RelationalError::NoCollection)?;

    // Build the $lookup stage with join conditions
    let lookup_stage = build_lookup_stage(
        &ctx.column_mapping,
        &right_collection,
        &right_ctx.column_mapping,
        right_ctx.stages,
        on,
    )?;
    ctx.stages.push(lookup_stage);

    // Apply join-type-specific logic
    match join_type {
        JoinType::Left => {
            // $unwind with preserveNullAndEmptyArrays: true for left join semantics
            ctx.stages.push(Stage::Unwind {
                path: "$_joined".to_string(),
                preserve_null_and_empty_arrays: Some(true),
                include_array_index: None,
            });

            // Add $project to remap columns
            let project = build_join_project(&ctx.column_mapping, &right_ctx.column_mapping)?;
            ctx.stages.push(Stage::Project(project));

            // Update column mapping
            update_join_column_mapping(ctx, left_column_count, &right_ctx.column_mapping);
        }
        JoinType::Inner => {
            // $unwind without preserveNullAndEmptyArrays filters out non-matches
            ctx.stages.push(Stage::Unwind {
                path: "$_joined".to_string(),
                preserve_null_and_empty_arrays: None,
                include_array_index: None,
            });

            // Add $project to remap columns
            let project = build_join_project(&ctx.column_mapping, &right_ctx.column_mapping)?;
            ctx.stages.push(Stage::Project(project));

            // Update column mapping
            update_join_column_mapping(ctx, left_column_count, &right_ctx.column_mapping);
        }
        JoinType::LeftSemi => {
            // Keep rows where _joined is non-empty, but only output left columns
            ctx.stages.push(Stage::Match(doc! {
                "_joined": { "$ne": [] }
            }));

            // Project to remove _joined and keep only left columns
            let project = build_semi_anti_project(&ctx.column_mapping)?;
            ctx.stages.push(Stage::Project(project));
            // Column mapping unchanged - only left columns in output
        }
        JoinType::LeftAnti => {
            // Keep rows where _joined is empty
            ctx.stages.push(Stage::Match(doc! {
                "_joined": { "$eq": [] }
            }));

            // Project to remove _joined and keep only left columns
            let project = build_semi_anti_project(&ctx.column_mapping)?;
            ctx.stages.push(Stage::Project(project));
            // Column mapping unchanged - only left columns in output
        }
        _ => {
            return Err(RelationalError::UnsupportedJoinType(format!(
                "{:?}",
                join_type
            )))
        }
    }

    Ok(())
}

/// Build the $lookup stage for a join.
fn build_lookup_stage(
    left_columns: &ColumnMapping,
    right_collection: &str,
    right_columns: &ColumnMapping,
    mut right_pipeline_stages: Vec<Stage>,
    on: &[JoinOn],
) -> Result<Stage, RelationalError> {
    // Build `let` clause - pass left column values as variables
    let let_vars = build_let_variables(left_columns, on)?;

    // Build the join predicate against the right relation's output schema and
    // append it after any right-side shaping stages.
    let match_expr = build_join_match_expr(right_columns, on)?;
    right_pipeline_stages.push(Stage::Match(doc! { "$expr": match_expr }));

    let lookup_pipeline = Pipeline::new(right_pipeline_stages);

    Ok(Stage::Lookup {
        from: Some(right_collection.to_string()),
        local_field: None,
        foreign_field: None,
        r#let: Some(let_vars),
        pipeline: Some(lookup_pipeline),
        r#as: "_joined".to_string(),
    })
}

/// Build the `let` variables for a $lookup stage.
/// These pass left-side values into the lookup pipeline.
fn build_let_variables(
    left_columns: &ColumnMapping,
    on: &[JoinOn],
) -> Result<Document, RelationalError> {
    let mut let_doc = Document::new();

    for (i, join_on) in on.iter().enumerate() {
        // Translate the left expression to get the field reference
        let ctx = ExpressionContext::new(left_columns);
        let left_expr = translate_expression(&join_on.left, &ctx)?;

        // Create variable name for this join condition
        let var_name = format!("left_{}", i);

        // The expression should be a field reference like "$col_0"
        // We store it as the variable value
        let_doc.insert(var_name, left_expr);
    }

    Ok(let_doc)
}

/// Build the $match expression for the lookup pipeline.
/// This compares right-side columns to left-side variables.
fn build_join_match_expr(
    right_columns: &ColumnMapping,
    on: &[JoinOn],
) -> Result<Bson, RelationalError> {
    let conditions: Result<Vec<Bson>, RelationalError> = on
        .iter()
        .enumerate()
        .map(|(i, join_on)| {
            // Translate the right expression using right-side column mapping
            let ctx = ExpressionContext::new(right_columns);
            let right_expr = translate_expression(&join_on.right, &ctx)?;

            // Left side is referenced via variable
            let left_var = format!("$$left_{}", i);

            Ok(Bson::Document(
                doc! { "$eq": [right_expr, Bson::String(left_var)] },
            ))
        })
        .collect();

    let conditions = conditions?;

    if conditions.len() == 1 {
        Ok(conditions.into_iter().next().unwrap())
    } else {
        Ok(Bson::Document(doc! { "$and": conditions }))
    }
}

/// Build the $project stage for left/inner joins.
/// Left columns keep their names, right columns are extracted from _joined.
fn build_join_project(
    left_columns: &ColumnMapping,
    right_columns: &ColumnMapping,
) -> Result<Document, RelationalError> {
    let mut project_doc = Document::new();
    project_doc.insert("_id", 0);

    let left_count = left_columns.len();

    // Left columns keep their indices
    for i in 0..left_count {
        let field_name = format!("col_{}", i);
        let left_field = left_columns
            .field_for_index(i as u64)
            .ok_or(RelationalError::InvalidColumnIndex(i as u64))?;
        project_doc.insert(&field_name, format!("${}", left_field));
    }

    // Right columns get offset indices
    for (i, right_col) in right_columns.iter().enumerate() {
        let field_name = format!("col_{}", left_count + i);
        project_doc.insert(&field_name, format!("$_joined.{right_col}"));
    }

    Ok(project_doc)
}

/// Build the $project stage for semi/anti joins.
/// Only left columns are output, _joined is removed.
fn build_semi_anti_project(left_columns: &ColumnMapping) -> Result<Document, RelationalError> {
    let mut project_doc = Document::new();
    project_doc.insert("_id", 0);

    for i in 0..left_columns.len() {
        let field_name = format!("col_{}", i);
        let left_field = left_columns
            .field_for_index(i as u64)
            .ok_or(RelationalError::InvalidColumnIndex(i as u64))?;
        project_doc.insert(&field_name, format!("${}", left_field));
    }

    Ok(project_doc)
}

/// Update the column mapping after a join to include right columns.
fn update_join_column_mapping(
    ctx: &mut PipelineContext<'_>,
    left_count: usize,
    right_columns: &ColumnMapping,
) {
    // Create new mapping with positional names for all columns
    let total_columns = left_count + right_columns.len();
    let new_mapping: Vec<String> = (0..total_columns).map(|i| format!("col_{}", i)).collect();
    ctx.column_mapping = ColumnMapping::new(new_mapping.iter().map(|s| s.as_str()));
}

// ============================================================================
// Window Functions (Phase 5)
// ============================================================================

/// Parsed window expression with its partition/order specifications.
struct WindowSpec {
    /// MongoDB operator (e.g., $documentNumber, $rank, $denseRank)
    operator: Bson,
    /// Partition by expressions
    partition_by: Bson,
    /// Sort by document
    sort_by: Document,
}

/// Build the Window relation.
fn build_window(
    exprs: &[RelationalExpression],
    ctx: &mut PipelineContext<'_>,
) -> Result<(), RelationalError> {
    let input_column_count = ctx.column_mapping.len();

    // Group window expressions by their partition/sort spec to minimize stages
    // For now, we create one $setWindowFields per expression (could optimize later)
    for (i, expr) in exprs.iter().enumerate() {
        let window_spec = translate_window_expression(expr, &ctx.column_mapping)?;
        let temp_field = format!("_w{}", i);

        // Build $setWindowFields stage
        let mut output_doc = Document::new();
        output_doc.insert(&temp_field, window_spec.operator);

        let set_window_fields = doc! {
            "$setWindowFields": {
                "partitionBy": window_spec.partition_by,
                "sortBy": window_spec.sort_by,
                "output": output_doc
            }
        };

        ctx.stages.push(Stage::Other(set_window_fields));
    }

    // Build $project stage to:
    // 1. Keep all original columns
    // 2. Rename window outputs from _w* to col_N format
    let mut project_doc = Document::new();
    project_doc.insert("_id", 0);

    // Keep original columns
    for i in 0..input_column_count {
        let field_name = ctx
            .column_mapping
            .field_for_index(i as u64)
            .ok_or(RelationalError::InvalidColumnIndex(i as u64))?;
        project_doc.insert(format!("col_{}", i), format!("${}", field_name));
    }

    // Rename window outputs
    for i in 0..exprs.len() {
        let output_col = format!("col_{}", input_column_count + i);
        let temp_field = format!("$_w{}", i);
        project_doc.insert(output_col, temp_field);
    }

    ctx.stages.push(Stage::Project(project_doc));

    // Update column mapping to include new window columns
    let total_columns = input_column_count + exprs.len();
    let new_mapping: Vec<String> = (0..total_columns).map(|i| format!("col_{}", i)).collect();
    ctx.column_mapping = ColumnMapping::new(new_mapping.iter().map(|s| s.as_str()));

    Ok(())
}

/// Translate a window expression to its MongoDB operator and partition/sort specs.
fn translate_window_expression(
    expr: &RelationalExpression,
    columns: &ColumnMapping,
) -> Result<WindowSpec, RelationalError> {
    match expr {
        RelationalExpression::RowNumber {
            order_by,
            partition_by,
        } => {
            let partition = translate_partition_by(partition_by, columns)?;
            let sort = translate_window_order_by(order_by, columns)?;
            Ok(WindowSpec {
                operator: Bson::Document(doc! { "$documentNumber": {} }),
                partition_by: partition,
                sort_by: sort,
            })
        }
        RelationalExpression::Rank {
            order_by,
            partition_by,
        } => {
            let partition = translate_partition_by(partition_by, columns)?;
            let sort = translate_window_order_by(order_by, columns)?;
            Ok(WindowSpec {
                operator: Bson::Document(doc! { "$rank": {} }),
                partition_by: partition,
                sort_by: sort,
            })
        }
        RelationalExpression::DenseRank {
            order_by,
            partition_by,
        } => {
            let partition = translate_partition_by(partition_by, columns)?;
            let sort = translate_window_order_by(order_by, columns)?;
            Ok(WindowSpec {
                operator: Bson::Document(doc! { "$denseRank": {} }),
                partition_by: partition,
                sort_by: sort,
            })
        }
        RelationalExpression::NTile { .. } => Err(RelationalError::UnsupportedExpression(
            "NTile has no native MongoDB operator".to_string(),
        )),
        RelationalExpression::CumeDist { .. } => Err(RelationalError::UnsupportedExpression(
            "CumeDist has no native MongoDB operator".to_string(),
        )),
        RelationalExpression::PercentRank { .. } => Err(RelationalError::UnsupportedExpression(
            "PercentRank has no native MongoDB operator".to_string(),
        )),
        _ => Err(RelationalError::UnsupportedExpression(format!(
            "{:?} is not a window function",
            std::mem::discriminant(expr)
        ))),
    }
}

/// Translate partition_by expressions to MongoDB partitionBy clause.
fn translate_partition_by(
    partition_by: &[RelationalExpression],
    columns: &ColumnMapping,
) -> Result<Bson, RelationalError> {
    match partition_by.len() {
        0 => Ok(Bson::Null), // No partitioning - entire result set is one partition
        1 => {
            // Single column: partitionBy: "$col_0"
            let field = extract_column_field(&partition_by[0], columns)?;
            Ok(Bson::String(format!("${}", field)))
        }
        _ => {
            // Multiple columns: partitionBy: { col_0: "$col_0", col_1: "$col_1" }
            let mut doc = Document::new();
            for expr in partition_by {
                let field = extract_column_field(expr, columns)?;
                doc.insert(&field, format!("${}", field));
            }
            Ok(Bson::Document(doc))
        }
    }
}

/// Translate order_by for window functions to MongoDB sortBy document.
fn translate_window_order_by(
    order_by: &[Sort],
    columns: &ColumnMapping,
) -> Result<Document, RelationalError> {
    let mut sort_doc = Document::new();

    for sort in order_by {
        let field = extract_column_field(&sort.expr, columns)?;
        let direction = match sort.direction {
            OrderDirection::Asc => 1,
            OrderDirection::Desc => -1,
        };
        sort_doc.insert(field, direction);
    }

    Ok(sort_doc)
}

/// Build the Union relation ($unionWith stages).
///
/// Union combines results from multiple relations into a single result set.
/// The first relation determines the base collection; subsequent relations
/// are added using `$unionWith` stages.
fn build_union(
    relations: &[Relation],
    ctx: &mut PipelineContext<'_>,
) -> Result<(), RelationalError> {
    if relations.is_empty() {
        return Err(RelationalError::InvalidUnion(
            "Union requires at least one relation".to_string(),
        ));
    }

    // Build the first relation - this sets the collection and initial stages
    let first = &relations[0];
    build_relation(first, ctx)?;

    let first_column_count = ctx.column_mapping.len();

    // For each additional relation, add $unionWith
    for (i, relation) in relations.iter().enumerate().skip(1) {
        // Build the relation in a fresh context to get its pipeline
        let union_result = build_relational_pipeline_with_config(relation, ctx.config)?;

        // Verify column counts match
        if union_result.output_columns.len() != first_column_count {
            return Err(RelationalError::InvalidUnion(format!(
                "Union input {} has {} columns, but first input has {}",
                i,
                union_result.output_columns.len(),
                first_column_count
            )));
        }

        // Add $unionWith stage
        ctx.stages.push(Stage::UnionWith {
            coll: union_result.collection,
            pipeline: Some(union_result.pipeline),
        });
    }

    Ok(())
}
