use itertools::Itertools;
use mongodb_support::aggregate::{Pipeline, Stage};
use tracing::instrument;

use crate::{
    interface_types::MongoAgentError,
    mongo_query_plan::{MongoConfiguration, Query, QueryPlan},
    mongodb::sanitize::get_field,
};

use super::{
    aggregates::facet_pipelines_for_query, foreach::pipeline_for_foreach,
    groups::pipeline_for_groups, is_response_faceted::is_response_faceted, make_selector,
    make_sort::make_sort_stages, native_query::pipeline_for_native_query, query_level::QueryLevel,
    relations::pipeline_for_relations, selection::selection_for_fields,
};

/// Shared logic to produce a MongoDB aggregation pipeline for a query request.
#[instrument(name = "Build Query Pipeline" skip_all, fields(internal.visibility = "user"))]
pub fn pipeline_for_query_request(
    config: &MongoConfiguration,
    query_plan: &QueryPlan,
) -> Result<Pipeline, MongoAgentError> {
    if let Some(variable_sets) = &query_plan.variables {
        pipeline_for_foreach(variable_sets, config, query_plan)
    } else {
        pipeline_for_non_foreach(config, query_plan, QueryLevel::Top)
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
    } else if let Some(grouping) = &query.groups {
        pipeline_for_groups(grouping)?
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

    let mut selection = selection_for_fields(query_plan.query.fields.as_ref())?;
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
