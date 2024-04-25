use std::collections::BTreeMap;

use configuration::Configuration;
use dc_api_types::{Aggregate, Query, QueryRequest, VariableSet};
use mongodb::bson::{self, doc, Bson};

use crate::{
    aggregation_function::AggregationFunction,
    interface_types::MongoAgentError,
    mongodb::{sanitize::get_field, Accumulator, Pipeline, Selection, Stage},
};

use super::{
    constants::{RESULT_FIELD, ROWS_FIELD},
    foreach::{foreach_variants, pipeline_for_foreach},
    make_selector, make_sort,
    native_query::pipeline_for_native_query,
    relations::pipeline_for_relations,
};

/// A query that includes aggregates will be run using a $facet pipeline stage, while a query
/// without aggregates will not. The choice affects how result rows are mapped to a QueryResponse.
///
/// If we have aggregate pipelines they should be combined with the fields pipeline (if there is
/// one) in a single facet stage. If we have fields, and no aggregates then the fields pipeline
/// can instead be appended to `pipeline`.
pub fn is_response_faceted(query: &Query) -> bool {
    match &query.aggregates {
        Some(aggregates) => !aggregates.is_empty(),
        _ => false,
    }
}

/// Shared logic to produce a MongoDB aggregation pipeline for a query request.
///
/// Returns a pipeline paired with a value that indicates whether the response requires
/// post-processing in the agent.
pub fn pipeline_for_query_request(
    config: &Configuration,
    query_request: &QueryRequest,
) -> Result<Pipeline, MongoAgentError> {
    let foreach = foreach_variants(query_request);
    if let Some(foreach) = foreach {
        pipeline_for_foreach(foreach, config, query_request)
    } else {
        pipeline_for_non_foreach(config, None, query_request)
    }
}

/// Produces a pipeline for a non-foreach query request, or for one variant of a foreach query
/// request.
///
/// Returns a pipeline paired with a value that indicates whether the response requires
/// post-processing in the agent.
pub fn pipeline_for_non_foreach(
    config: &Configuration,
    variables: Option<&VariableSet>,
    query_request: &QueryRequest,
) -> Result<Pipeline, MongoAgentError> {
    let query = &*query_request.query;
    let Query {
        offset,
        order_by,
        r#where,
        ..
    } = query;
    let mut pipeline = Pipeline::empty();

    // If this is a native query then we start with the native query's pipeline
    pipeline.append(pipeline_for_native_query(config, variables, query_request)?);

    // Stages common to aggregate and row queries.
    pipeline.append(pipeline_for_relations(config, variables, query_request)?);

    let match_stage = r#where
        .as_ref()
        .map(|expression| make_selector(variables, expression))
        .transpose()?
        .map(Stage::Match);
    let sort_stage: Option<Stage> = order_by.iter().map(|o| Stage::Sort(make_sort(o))).next();
    let skip_stage = offset.map(Stage::Skip);

    [match_stage, sort_stage, skip_stage]
        .into_iter()
        .flatten()
        .for_each(|stage| pipeline.push(stage));

    // `diverging_stages` includes either a $facet stage if the query includes aggregates, or the
    // sort and limit stages if we are requesting rows only. In both cases the last stage is
    // a $replaceWith.
    let diverging_stages = if is_response_faceted(query) {
        let (facet_pipelines, select_facet_results) = facet_pipelines_for_query(query_request)?;
        let aggregation_stages = Stage::Facet(facet_pipelines);
        let replace_with_stage = Stage::ReplaceWith(select_facet_results);
        let stages = Pipeline::from_iter([aggregation_stages, replace_with_stage]);
        stages
    } else {
        let stages = pipeline_for_fields_facet(query_request)?;
        stages
    };

    pipeline.append(diverging_stages);
    Ok(pipeline)
}

/// Generate a pipeline to select fields requested by the given query. This is intended to be used
/// within a $facet stage. We assume that the query's `where`, `order_by`, `offset` criteria (which
/// are shared with aggregates) have already been applied, and that we have already joined
/// relations.
pub fn pipeline_for_fields_facet(
    query_request: &QueryRequest,
) -> Result<Pipeline, MongoAgentError> {
    let Query { limit, .. } = &*query_request.query;

    let limit_stage = limit.map(Stage::Limit);
    let replace_with_stage: Stage =
        Stage::ReplaceWith(Selection::from_query_request(query_request)?);

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
    query_request: &QueryRequest,
) -> Result<(BTreeMap<String, Pipeline>, Selection), MongoAgentError> {
    let query = &*query_request.query;
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
                key.clone(),
                pipeline_for_aggregate(aggregate.clone(), *aggregates_limit)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, MongoAgentError>>()?;

    if let Some(_) = fields {
        let fields_pipeline = pipeline_for_fields_facet(query_request)?;
        facet_pipelines.insert(ROWS_FIELD.to_owned(), fields_pipeline);
    }

    // This builds a map that feeds into a `$replaceWith` pipeline stage to build a map of
    // aggregation results.
    let aggregate_selections: bson::Document = aggregates
        .iter()
        .flatten()
        .map(|(key, _aggregate)| {
            // The facet result for each aggregate is an array containing a single document which
            // has a field called `result`. This code selects each facet result by name, and pulls
            // out the `result` value.
            (
                // TODO: Is there a way we can prevent potential code injection in the use of `key`
                // here?
                key.clone(),
                doc! {
                    "$getField": {
                        "field": RESULT_FIELD, // evaluates to the value of this field
                        "input": { "$first": get_field(key) }, // field is accessed from this document
                    },
                }
                .into(),
            )
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

    let selection = Selection(
        [select_aggregates, select_rows]
            .into_iter()
            .flatten()
            .collect(),
    );

    Ok((facet_pipelines, selection))
}

fn pipeline_for_aggregate(
    aggregate: Aggregate,
    limit: Option<i64>,
) -> Result<Pipeline, MongoAgentError> {
    // Group expressions use a dollar-sign prefix to indicate a reference to a document field.
    // TODO: I don't think we need sanitizing, but I could use a second opinion -Jesse H.
    let field_ref = |column: &str| Bson::String(format!("${column}"));

    let pipeline = match aggregate {
        Aggregate::ColumnCount { column, distinct } if distinct => Pipeline::from_iter(
            [
                Some(Stage::Match(
                    bson::doc! { &column: { "$exists": true, "$ne": null } },
                )),
                limit.map(Stage::Limit),
                Some(Stage::Group {
                    key_expression: field_ref(&column),
                    accumulators: [].into(),
                }),
                Some(Stage::Count(RESULT_FIELD.to_string())),
            ]
            .into_iter()
            .flatten(),
        ),

        Aggregate::ColumnCount { column, .. } => Pipeline::from_iter(
            [
                Some(Stage::Match(
                    bson::doc! { &column: { "$exists": true, "$ne": null } },
                )),
                limit.map(Stage::Limit),
                Some(Stage::Group {
                    key_expression: field_ref(&column),
                    accumulators: [(RESULT_FIELD.to_string(), Accumulator::Count)].into(),
                }),
                Some(Stage::Group {
                    key_expression: Bson::Null,
                    // Sums field values from the `result` field of the previous stage, and writes
                    // a new field which is also called `result`.
                    accumulators: [(
                        RESULT_FIELD.to_string(),
                        Accumulator::Sum(field_ref(RESULT_FIELD)),
                    )]
                    .into(),
                }),
            ]
            .into_iter()
            .flatten(),
        ),

        Aggregate::SingleColumn {
            column, function, ..
        } => {
            use AggregationFunction::*;

            let accumulator = match AggregationFunction::from_graphql_name(&function)? {
                Avg => Accumulator::Avg(field_ref(&column)),
                Count => Accumulator::Count,
                Min => Accumulator::Min(field_ref(&column)),
                Max => Accumulator::Max(field_ref(&column)),
                Sum => Accumulator::Sum(field_ref(&column)),
            };
            Pipeline::from_iter(
                [
                    Some(Stage::Match(
                        bson::doc! { column: { "$exists": true, "$ne": null } },
                    )),
                    limit.map(Stage::Limit),
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
                limit.map(Stage::Limit),
                Some(Stage::Count(RESULT_FIELD.to_string())),
            ]
            .into_iter()
            .flatten(),
        ),
    };
    Ok(pipeline)
}
