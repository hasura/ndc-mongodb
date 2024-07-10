use itertools::Itertools as _;
use mongodb::bson::{bson, Document};
use ndc_models::OrderDirection;

use crate::{
    interface_types::MongoAgentError,
    mongo_query_plan::{OrderBy, OrderByTarget},
    mongodb::sanitize::safe_name,
};

pub fn make_sort(order_by: &OrderBy) -> Result<Document, MongoAgentError> {
    let OrderBy { elements } = order_by;

    elements
        .clone()
        .iter()
        .map(|obe| {
            let direction = match obe.clone().order_direction {
                OrderDirection::Asc => bson!(1),
                OrderDirection::Desc => bson!(-1),
            };
            match &obe.target {
                OrderByTarget::Column {
                    name,
                    field_path,
                    path,
                } => Ok((
                    column_ref_with_path(name, field_path.as_deref(), path)?,
                    direction,
                )),
                OrderByTarget::SingleColumnAggregate {
                    column: _,
                    function: _,
                    path: _,
                    result_type: _,
                } =>
                // TODO: MDB-150
                {
                    Err(MongoAgentError::NotImplemented(
                        "ordering by single column aggregate",
                    ))
                }
                OrderByTarget::StarCountAggregate { path: _ } => Err(
                    // TODO: MDB-151
                    MongoAgentError::NotImplemented("ordering by star count aggregate"),
                ),
            }
        })
        .collect()
}

// TODO: MDB-159 Replace use of [safe_name] with [ColumnRef].
fn column_ref_with_path(
    name: &ndc_models::FieldName,
    field_path: Option<&[ndc_models::FieldName]>,
    relation_path: &[ndc_models::RelationshipName],
) -> Result<String, MongoAgentError> {
    relation_path
        .iter()
        .map(|n| n.as_str())
        .chain(std::iter::once(name.as_str()))
        .chain(field_path.into_iter().flatten().map(|n| n.as_str()))
        .map(|x| safe_name(x))
        .process_results(|mut iter| iter.join("."))
}
