use std::iter::once;

use mongodb::bson::{doc, Bson};

use crate::{
    mongo_query_plan::{ExistsInCollection, Expression},
    query::column_ref::ColumnRef,
};

use super::Result;

pub struct AggregationExpression(pub Bson);

pub fn make_aggregation_expression(expr: &Expression) -> Result<AggregationExpression> {
    todo!("make_aggregation_expression")
}

// TODO: Move predicate application to the join step instead of filtering the entire related or
// unrelated collection here
pub fn make_aggregation_expression_for_exists(
    in_collection: &ExistsInCollection,
    predicate: Option<&Expression>,
) -> Result<AggregationExpression> {
    let expression = match (in_collection, predicate) {
        (ExistsInCollection::Related { relationship }, Some(predicate)) => {
            let relationship_ref = ColumnRef::from_relationship(relationship);
            exists_in_array(relationship_ref, predicate)?
        }
        (ExistsInCollection::Related { relationship }, None) => {
            let relationship_ref = ColumnRef::from_relationship(relationship);
            exists_in_array_no_predicate(relationship_ref)
        }
        (
            ExistsInCollection::Unrelated {
                unrelated_collection,
            },
            Some(predicate),
        ) => {
            let collection_ref = ColumnRef::from_unrelated_collection(unrelated_collection);
            exists_in_array(collection_ref, predicate)?
        }
        (
            ExistsInCollection::Unrelated {
                unrelated_collection,
            },
            None,
        ) => {
            let collection_ref = ColumnRef::from_unrelated_collection(unrelated_collection);
            exists_in_array_no_predicate(collection_ref)
        }
        (
            ExistsInCollection::NestedCollection {
                column_name,
                field_path,
                ..
            },
            Some(predicate),
        ) => {
            let column_ref = ColumnRef::from_field_path(field_path.iter().chain(once(column_name)));
            exists_in_array(column_ref, predicate)?
        }
        (
            ExistsInCollection::NestedCollection {
                column_name,
                field_path,
                ..
            },
            None,
        ) => {
            let column_ref = ColumnRef::from_field_path(field_path.iter().chain(once(column_name)));
            exists_in_array_no_predicate(column_ref)
        }
    };
    Ok(expression)
}

fn exists_in_array(
    array_ref: ColumnRef<'_>,
    predicate: &Expression,
) -> Result<AggregationExpression> {
    let AggregationExpression(sub_expression) = make_aggregation_expression(predicate)?;
    Ok(AggregationExpression(
        doc! {
            "$anyElementTrue": {
                "$map": {
                    "input": array_ref.into_aggregate_expression(),
                    "as": "CURRENT", // implicitly changes the document root in `exp` to be the array element
                    "in": sub_expression,
                }
            }
        }
        .into(),
    ))
}

fn exists_in_array_no_predicate(array_ref: ColumnRef<'_>) -> AggregationExpression {
    let index_zero = "0".into();
    let first_element_ref = array_ref.into_nested_field(&index_zero);
    AggregationExpression(
        doc! {
            "$ne": [first_element_ref.into_aggregate_expression(), null]
        }
        .into(),
    )
}
