use std::collections::BTreeMap;

use configuration::MongoScalarType;
use itertools::Itertools;
use mongodb::bson::{self, doc, Bson};
use mongodb_support::{
    aggregate::{Accumulator, AggregateCommand, Pipeline, Selection, Stage},
    BsonScalarType,
};
use ndc_models::FieldName;
use tracing::instrument;

use crate::{
    aggregation_function::AggregationFunction,
    comparison_function::ComparisonFunction,
    interface_types::MongoAgentError,
    mongo_query_plan::{
        Aggregate, ComparisonTarget, ComparisonValue, Expression, MongoConfiguration, Query,
        QueryPlan, Type,
    },
    mongodb::{sanitize::get_field, selection_from_query_request},
};

use super::{
    column_ref::ColumnRef,
    constants::{RESULT_FIELD, ROWS_FIELD},
    foreach::command_for_foreach,
    make_selector,
    make_sort::make_sort_stages,
    native_query::pipeline_for_native_query,
    query_level::QueryLevel,
    relations::pipeline_for_relations,
    QueryTarget,
};

/// A query that includes aggregates will be run using a $facet pipeline stage, while a query
/// without aggregates will not. The choice affects how result rows are mapped to a QueryResponse.
///
/// If we have aggregate pipelines they should be combined with the fields pipeline (if there is
/// one) in a single facet stage. If we have fields, and no aggregates then the fields pipeline
/// can instead be appended to `pipeline`.
pub fn is_response_faceted(query: &Query) -> bool {
    query.has_aggregates()
}

/// Shared logic to produce a MongoDB aggregation pipeline for a query request.
#[instrument(name = "Build Query Pipeline" skip_all, fields(internal.visibility = "user"))]
pub fn command_for_query_request(
    config: &MongoConfiguration,
    query_plan: &QueryPlan,
) -> Result<AggregateCommand, MongoAgentError> {
    if let Some(variable_sets) = &query_plan.variables {
        command_for_foreach(variable_sets, config, query_plan)
    } else {
        let target = QueryTarget::for_request(config, query_plan);
        let pipeline = pipeline_for_non_foreach(config, query_plan, QueryLevel::Top)?;
        Ok(AggregateCommand {
            collection: target.input_collection().map(ToString::to_string),
            pipeline,
            let_vars: None,
        })
    }
}

/// Produces a pipeline for a query request that does not include variable sets, or produces
/// a sub-pipeline to be used inside of a larger pipeline for a query request that does include
/// variable sets.
pub fn pipeline_for_non_foreach(
    config: &MongoConfiguration,
    query_plan: &QueryPlan,
    query_level: QueryLevel,
) -> Result<Pipeline, MongoAgentError> {
    let query = &query_plan.query;
    let Query {
        offset,
        order_by,
        predicate,
        ..
    } = query;
    let mut pipeline = Pipeline::empty();

    // If this is a native query then we start with the native query's pipeline
    pipeline.append(pipeline_for_native_query(config, query_plan)?);

    // Stages common to aggregate and row queries.
    pipeline.append(pipeline_for_relations(config, query_plan)?);

    let match_stage = predicate
        .as_ref()
        .map(make_selector)
        .transpose()?
        .map(Stage::Match);
    let sort_stages: Vec<Stage> = order_by
        .iter()
        .map(make_sort_stages)
        .flatten_ok()
        .collect::<Result<Vec<_>, _>>()?;
    let skip_stage = offset.map(Into::into).map(Stage::Skip);

    match_stage
        .into_iter()
        .chain(sort_stages)
        .chain(skip_stage)
        .for_each(|stage| pipeline.push(stage));

    // `diverging_stages` includes either a $facet stage if the query includes aggregates, or the
    // sort and limit stages if we are requesting rows only. In both cases the last stage is
    // a $replaceWith.
    let diverging_stages = if is_response_faceted(query) {
        let (facet_pipelines, select_facet_results) =
            facet_pipelines_for_query(query_plan, query_level)?;
        let aggregation_stages = Stage::Facet(facet_pipelines);
        let replace_with_stage = Stage::ReplaceWith(select_facet_results);
        Pipeline::from_iter([aggregation_stages, replace_with_stage])
    } else {
        pipeline_for_fields_facet(query_plan, query_level)?
    };

    pipeline.append(diverging_stages);
    Ok(pipeline)
}

/// Generate a pipeline to select fields requested by the given query. This is intended to be used
/// within a $facet stage. We assume that the query's `where`, `order_by`, `offset` criteria (which
/// are shared with aggregates) have already been applied, and that we have already joined
/// relations.
pub fn pipeline_for_fields_facet(
    query_plan: &QueryPlan,
    query_level: QueryLevel,
) -> Result<Pipeline, MongoAgentError> {
    let Query {
        limit,
        relationships,
        ..
    } = &query_plan.query;

    let mut selection = selection_from_query_request(query_plan)?;
    if query_level != QueryLevel::Top {
        // Queries higher up the chain might need to reference relationships from this query. So we
        // forward relationship arrays if this is not the top-level query.
        for relationship_key in relationships.keys() {
            selection = selection.try_map_document(|mut doc| {
                doc.insert(
                    relationship_key.to_owned(),
                    get_field(relationship_key.as_str()),
                );
                doc
            })?;
        }
    }

    let limit_stage = limit.map(Into::into).map(Stage::Limit);
    let replace_with_stage: Stage = Stage::ReplaceWith(selection);

    Ok(Pipeline::from_iter(
        [limit_stage, replace_with_stage.into()]
            .into_iter()
            .flatten(),
    ))
}

/// Returns a map of pipelines for evaluating each aggregate independently, paired with
/// a `Selection` that converts results of each pipeline to a format compatible with
/// `QueryResponse`.
fn facet_pipelines_for_query(
    query_plan: &QueryPlan,
    query_level: QueryLevel,
) -> Result<(BTreeMap<String, Pipeline>, Selection), MongoAgentError> {
    let query = &query_plan.query;
    let Query {
        aggregates,
        aggregates_limit,
        fields,
        ..
    } = query;
    let mut facet_pipelines = aggregates
        .iter()
        .flatten()
        .map(|(key, aggregate)| {
            Ok((
                key.to_string(),
                pipeline_for_aggregate(aggregate.clone(), *aggregates_limit)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, MongoAgentError>>()?;

    if fields.is_some() {
        let fields_pipeline = pipeline_for_fields_facet(query_plan, query_level)?;
        facet_pipelines.insert(ROWS_FIELD.to_owned(), fields_pipeline);
    }

    // This builds a map that feeds into a `$replaceWith` pipeline stage to build a map of
    // aggregation results.
    let aggregate_selections: bson::Document = aggregates
        .iter()
        .flatten()
        .map(|(key, aggregate)| {
            // The facet result for each aggregate is an array containing a single document which
            // has a field called `result`. This code selects each facet result by name, and pulls
            // out the `result` value.
            let value_expr = doc! {
                "$getField": {
                    "field": RESULT_FIELD, // evaluates to the value of this field
                    "input": { "$first": get_field(key.as_str()) }, // field is accessed from this document
                },
            };

            // Matching SQL semantics, if a **count** aggregation does not match any rows we want
            // to return zero. Other aggregations should return null.
            let value_expr = if is_count(aggregate) {
                doc! {
                    "$ifNull": [value_expr, 0],
                }
            // Otherwise if the aggregate value is missing because the aggregation applied to an
            // empty document set then provide an explicit `null` value.
            } else {
                doc! {
                    "$ifNull": [value_expr, null]
                }
            };

            (key.to_string(), value_expr.into())
        })
        .collect();

    let select_aggregates = if !aggregate_selections.is_empty() {
        Some(("aggregates".to_owned(), aggregate_selections.into()))
    } else {
        None
    };

    let select_rows = match fields {
        Some(_) => Some(("rows".to_owned(), Bson::String(format!("${ROWS_FIELD}")))),
        _ => None,
    };

    let selection = Selection::new(
        [select_aggregates, select_rows]
            .into_iter()
            .flatten()
            .collect(),
    );

    Ok((facet_pipelines, selection))
}

fn is_count(aggregate: &Aggregate) -> bool {
    match aggregate {
        Aggregate::ColumnCount { .. } => true,
        Aggregate::StarCount { .. } => true,
        Aggregate::SingleColumn { function, .. } => function.is_count(),
    }
}

fn pipeline_for_aggregate(
    aggregate: Aggregate,
    limit: Option<u32>,
) -> Result<Pipeline, MongoAgentError> {
    fn mk_target_field(name: FieldName, field_path: Option<Vec<FieldName>>) -> ComparisonTarget {
        ComparisonTarget::Column {
            name,
            field_path,
            field_type: Type::Scalar(MongoScalarType::ExtendedJSON), // type does not matter here
            path: Default::default(),
        }
    }

    fn filter_to_documents_with_value(
        target_field: ComparisonTarget,
    ) -> Result<Stage, MongoAgentError> {
        Ok(Stage::Match(make_selector(
            &Expression::BinaryComparisonOperator {
                column: target_field,
                operator: ComparisonFunction::NotEqual,
                value: ComparisonValue::Scalar {
                    value: serde_json::Value::Null,
                    value_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::Null)),
                },
            },
        )?))
    }

    let pipeline = match aggregate {
        Aggregate::ColumnCount {
            column,
            field_path,
            distinct,
        } if distinct => {
            let target_field = mk_target_field(column, field_path);
            Pipeline::from_iter(
                [
                    Some(filter_to_documents_with_value(target_field.clone())?),
                    limit.map(Into::into).map(Stage::Limit),
                    Some(Stage::Group {
                        key_expression: ColumnRef::from_comparison_target(&target_field)
                            .into_aggregate_expression(),
                        accumulators: [].into(),
                    }),
                    Some(Stage::Count(RESULT_FIELD.to_string())),
                ]
                .into_iter()
                .flatten(),
            )
        }

        Aggregate::ColumnCount {
            column,
            field_path,
            distinct: _,
        } => Pipeline::from_iter(
            [
                Some(filter_to_documents_with_value(mk_target_field(
                    column, field_path,
                ))?),
                limit.map(Into::into).map(Stage::Limit),
                Some(Stage::Count(RESULT_FIELD.to_string())),
            ]
            .into_iter()
            .flatten(),
        ),

        Aggregate::SingleColumn {
            column,
            field_path,
            function,
            result_type: _,
        } => {
            use AggregationFunction::*;

            let target_field = ComparisonTarget::Column {
                name: column.clone(),
                field_path,
                field_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::Null)), // type does not matter here
                path: Default::default(),
            };
            let field_ref =
                ColumnRef::from_comparison_target(&target_field).into_aggregate_expression();

            let accumulator = match function {
                Avg => Accumulator::Avg(field_ref),
                Count => Accumulator::Count,
                Min => Accumulator::Min(field_ref),
                Max => Accumulator::Max(field_ref),
                Sum => Accumulator::Sum(field_ref),
            };
            Pipeline::from_iter(
                [
                    Some(filter_to_documents_with_value(target_field)?),
                    limit.map(Into::into).map(Stage::Limit),
                    Some(Stage::Group {
                        key_expression: Bson::Null,
                        accumulators: [(RESULT_FIELD.to_string(), accumulator)].into(),
                    }),
                ]
                .into_iter()
                .flatten(),
            )
        }

        Aggregate::StarCount {} => Pipeline::from_iter(
            [
                limit.map(Into::into).map(Stage::Limit),
                Some(Stage::Count(RESULT_FIELD.to_string())),
            ]
            .into_iter()
            .flatten(),
        ),
    };
    Ok(pipeline)
}
