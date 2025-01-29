use std::collections::BTreeMap;

use indexmap::IndexMap;
use mongodb::bson::{self, bson, doc};
use mongodb_support::aggregate::{Accumulator, Pipeline, Selection, Stage};
use ndc_models::FieldName;

use crate::{
    aggregation_function::AggregationFunction,
    interface_types::MongoAgentError,
    mongo_query_plan::{Aggregate, Dimension, Grouping},
};

use super::column_ref::ColumnRef;

pub fn pipeline_for_groups(grouping: &Grouping) -> Pipeline {
    let group_stage = Stage::Group {
        key_expression: dimensions_to_expression(&grouping.dimensions).into(),
        accumulators: accumulators_for_aggregates(&grouping.aggregates),
    };

    // TODO: to implement 'query.aggregates.group_by.paginate' apply grouping.limit and
    // grouping.offset **after** group stage because those options count groups, not documents

    let replace_with_stage = Stage::ReplaceWith(selection(grouping));

    Pipeline::new(vec![group_stage, replace_with_stage])
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

fn accumulators_for_aggregates(
    aggregates: &IndexMap<FieldName, Aggregate>,
) -> BTreeMap<String, Accumulator> {
    aggregates
        .into_iter()
        .map(|(name, aggregate)| (name.to_string(), aggregate_to_accumulator(aggregate)))
        .collect()
}

fn aggregate_to_accumulator(aggregate: &Aggregate) -> Accumulator {
    use Aggregate as A;
    match aggregate {
        A::ColumnCount {
            column,
            arguments,
            field_path,
            distinct,
        } => todo!(),
        A::SingleColumn {
            column,
            field_path,
            function,
            ..
        } => {
            use AggregationFunction as A;

            let field_ref = ColumnRef::from_column_and_field_path(column, field_path.as_ref())
                .into_aggregate_expression()
                .into_bson();

            match function {
                A::Avg => Accumulator::Avg(field_ref),
                A::Min => Accumulator::Min(field_ref),
                A::Max => Accumulator::Max(field_ref),
                A::Sum => Accumulator::Sum(field_ref),
            }
        }
        A::StarCount => todo!(),
    }
}

fn selection(grouping: &Grouping) -> Selection {
    let dimensions = ("dimensions".to_string(), bson!("$_id"));
    let selected_aggregates = grouping.aggregates.keys().map(|key| {
        (
            key.to_string(),
            bson!({
                "$ifNull": [ColumnRef::from_field(key).into_aggregate_expression(), null]
            }),
        )
    });
    let selection_doc = std::iter::once(dimensions)
        .chain(selected_aggregates)
        .collect();
    Selection::new(selection_doc)
}
