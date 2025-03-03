use std::borrow::Cow;

use mongodb::bson::{self, bson};
use mongodb_support::aggregate::{Pipeline, Selection, SortDocument, Stage};
use ndc_models::OrderDirection;

use crate::{
    constants::GROUP_DIMENSIONS_KEY,
    interface_types::MongoAgentError,
    mongo_query_plan::{Dimension, GroupOrderBy, GroupOrderByTarget, Grouping},
};

use super::{
    aggregates::{accumulators_for_aggregates, selection_for_aggregate},
    column_ref::ColumnRef,
};

type Result<T> = std::result::Result<T, MongoAgentError>;

// TODO: This function can be infallible once ENG-1562 is implemented.
pub fn pipeline_for_groups(grouping: &Grouping) -> Result<Pipeline> {
    let group_stage = Stage::Group {
        key_expression: dimensions_to_expression(&grouping.dimensions).into(),
        accumulators: accumulators_for_aggregates(&grouping.aggregates),
    };

    // TODO: ENG-1562 This implementation does not fully implement the
    // 'query.aggregates.group_by.order' capability! This only orders by dimensions. Before
    // enabling the capability we also need to be able to order by aggregates. We need partial
    // support for order by to get consistent integration test snapshots.
    let sort_groups_stage = grouping
        .order_by
        .as_ref()
        .map(sort_stage_for_grouping)
        .transpose()?;

    // TODO: ENG-1563 to implement 'query.aggregates.group_by.paginate' apply grouping.limit and
    // grouping.offset **after** group stage because those options count groups, not documents

    let replace_with_stage = Stage::ReplaceWith(selection_for_grouping(grouping, "_id"));

    Ok(Pipeline::new(
        [
            Some(group_stage),
            sort_groups_stage,
            Some(replace_with_stage),
        ]
        .into_iter()
        .flatten()
        .collect(),
    ))
}

/// Converts each dimension to a MongoDB aggregate expression that evaluates to the appropriate
/// value when applied to each input document. The array of expressions can be used directly as the
/// group stage key expression.
fn dimensions_to_expression(dimensions: &[Dimension]) -> bson::Array {
    dimensions
        .iter()
        .map(|dimension| {
            let column_ref = match dimension {
                Dimension::Column {
                    path,
                    column_name,
                    field_path,
                    ..
                } => ColumnRef::from_relationship_path_column_and_field_path(
                    path,
                    column_name,
                    field_path.as_ref(),
                ),
            };
            column_ref.into_aggregate_expression().into_bson()
        })
        .collect()
}

fn selection_for_grouping(grouping: &Grouping, dimensions_field_name: &str) -> Selection {
    let dimensions = (
        GROUP_DIMENSIONS_KEY.to_string(),
        bson!(format!("${dimensions_field_name}")),
    );
    let selected_aggregates = grouping
        .aggregates
        .iter()
        .map(|(key, aggregate)| selection_for_aggregate(key, aggregate));
    let selection_doc = std::iter::once(dimensions)
        .chain(selected_aggregates)
        .collect();
    Selection::new(selection_doc)
}

// TODO: ENG-1562 This is where we need to implement sorting by aggregates
fn sort_stage_for_grouping(order_by: &GroupOrderBy) -> Result<Stage> {
    let sort_doc = order_by
        .elements
        .iter()
        .map(|element| match element.target {
            GroupOrderByTarget::Dimension { index } => {
                let key = format!("_id.{index}");
                let direction = match element.order_direction {
                    OrderDirection::Asc => bson!(1),
                    OrderDirection::Desc => bson!(-1),
                };
                Ok((key, direction))
            }
            GroupOrderByTarget::Aggregate { .. } => Err(MongoAgentError::NotImplemented(
                Cow::Borrowed("sorting groups by aggregate"),
            )),
        })
        .collect::<Result<_>>()?;
    Ok(Stage::Sort(SortDocument::from_doc(sort_doc)))
}
