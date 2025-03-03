use std::collections::BTreeMap;

use itertools::Itertools;
use mongodb::bson::{bson, Bson};
use mongodb_support::aggregate::{Pipeline, Selection, Stage};
use tracing::instrument;

use crate::{
    constants::{ROW_SET_AGGREGATES_KEY, ROW_SET_GROUPS_KEY, ROW_SET_ROWS_KEY},
    interface_types::MongoAgentError,
    mongo_query_plan::{MongoConfiguration, Query, QueryPlan},
};

use super::{
    aggregates::pipeline_for_aggregates, column_ref::ColumnRef, foreach::pipeline_for_foreach,
    groups::pipeline_for_groups, is_response_faceted::ResponseFacets, make_selector,
    make_sort::make_sort_stages, native_query::pipeline_for_native_query, query_level::QueryLevel,
    relations::pipeline_for_relations, selection::selection_for_fields,
};

type Result<T> = std::result::Result<T, MongoAgentError>;

/// Shared logic to produce a MongoDB aggregation pipeline for a query request.
#[instrument(name = "Build Query Pipeline" skip_all, fields(internal.visibility = "user"))]
pub fn pipeline_for_query_request(
    config: &MongoConfiguration,
    query_plan: &QueryPlan,
) -> Result<Pipeline> {
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
) -> Result<Pipeline> {
    let query = &query_plan.query;
    let Query {
        limit,
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
        .collect::<Result<Vec<_>>>()?;
    let limit_stage = limit.map(Into::into).map(Stage::Limit);
    let skip_stage = offset.map(Into::into).map(Stage::Skip);

    match_stage
        .into_iter()
        .chain(sort_stages)
        .chain(skip_stage)
        .chain(limit_stage)
        .for_each(|stage| pipeline.push(stage));

    let diverging_stages = match ResponseFacets::from_query(query) {
        ResponseFacets::Combination { .. } => {
            let (facet_pipelines, select_facet_results) =
                facet_pipelines_for_query(query_plan, query_level)?;
            let facet_stage = Stage::Facet(facet_pipelines);
            let replace_with_stage = Stage::ReplaceWith(select_facet_results);
            Pipeline::new(vec![facet_stage, replace_with_stage])
        }
        ResponseFacets::AggregatesOnly(aggregates) => pipeline_for_aggregates(aggregates),
        ResponseFacets::FieldsOnly(_) => pipeline_for_fields_facet(query_plan, query_level)?,
        ResponseFacets::GroupsOnly(grouping) => pipeline_for_groups(grouping)?,
    };

    pipeline.append(diverging_stages);
    Ok(pipeline)
}

/// Returns a map of pipelines for evaluating each aggregate independently, paired with
/// a `Selection` that converts results of each pipeline to a format compatible with
/// `QueryResponse`.
fn facet_pipelines_for_query(
    query_plan: &QueryPlan,
    query_level: QueryLevel,
) -> Result<(BTreeMap<String, Pipeline>, Selection)> {
    let query = &query_plan.query;
    let Query {
        aggregates,
        fields,
        groups,
        ..
    } = query;
    let mut facet_pipelines = BTreeMap::new();

    let (aggregates_pipeline_facet, select_aggregates) = match aggregates {
        Some(aggregates) => {
            let internal_key = "__AGGREGATES__";
            let aggregates_pipeline = pipeline_for_aggregates(aggregates);
            let facet = (internal_key.to_string(), aggregates_pipeline);
            let selection = (
                ROW_SET_AGGREGATES_KEY.to_string(),
                bson!({ "$first": format!("${internal_key}") }),
            );
            (Some(facet), Some(selection))
        }
        None => (None, None),
    };

    let (groups_pipeline_facet, select_groups) = match groups {
        Some(grouping) => {
            let internal_key = "__GROUPS__";
            let groups_pipeline = pipeline_for_groups(grouping)?;
            let facet = (internal_key.to_string(), groups_pipeline);
            let selection = (
                ROW_SET_GROUPS_KEY.to_string(),
                Bson::String(format!("${internal_key}")),
            );
            (Some(facet), Some(selection))
        }
        None => (None, None),
    };

    let (rows_pipeline_facet, select_rows) = match fields {
        Some(_) => {
            let internal_key = "__ROWS__";
            let rows_pipeline = pipeline_for_fields_facet(query_plan, query_level)?;
            let facet = (internal_key.to_string(), rows_pipeline);
            let selection = (
                ROW_SET_ROWS_KEY.to_string().to_string(),
                Bson::String(format!("${internal_key}")),
            );
            (Some(facet), Some(selection))
        }
        None => (None, None),
    };

    for (key, pipeline) in [
        aggregates_pipeline_facet,
        groups_pipeline_facet,
        rows_pipeline_facet,
    ]
    .into_iter()
    .flatten()
    {
        facet_pipelines.insert(key, pipeline);
    }

    let selection = Selection::new(
        [select_aggregates, select_groups, select_rows]
            .into_iter()
            .flatten()
            .collect(),
    );

    Ok((facet_pipelines, selection))
}

/// Generate a pipeline to select fields requested by the given query. This is intended to be used
/// within a $facet stage. We assume that the query's `where`, `order_by`, `offset`, `limit`
/// criteria (which are shared with aggregates) have already been applied, and that we have already
/// joined relations.
pub fn pipeline_for_fields_facet(
    query_plan: &QueryPlan,
    query_level: QueryLevel,
) -> Result<Pipeline> {
    let Query { relationships, .. } = &query_plan.query;

    let mut selection = selection_for_fields(query_plan.query.fields.as_ref())?;
    if query_level != QueryLevel::Top {
        // Queries higher up the chain might need to reference relationships from this query. So we
        // forward relationship arrays if this is not the top-level query.
        for relationship_key in relationships.keys() {
            selection = selection.try_map_document(|mut doc| {
                doc.insert(
                    relationship_key.to_owned(),
                    ColumnRef::from_field(relationship_key.as_str()).into_aggregate_expression(),
                );
                doc
            })?;
        }
    }

    let replace_with_stage: Stage = Stage::ReplaceWith(selection);
    Ok(Pipeline::new(vec![replace_with_stage]))
}
