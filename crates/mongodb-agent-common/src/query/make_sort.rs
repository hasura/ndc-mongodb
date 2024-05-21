use itertools::Itertools as _;
use mongodb::bson::{bson, Document};
use ndc_models::OrderDirection;

use crate::{
    interface_types::MongoAgentError,
    mongo_query_plan::{OrderBy, OrderByTarget},
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
                OrderByTarget::Column { name, path } => {
                    Ok((column_ref_with_path(name, path), direction))
                }
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

fn column_ref_with_path(name: &String, path: &[String]) -> String {
    std::iter::once(name).chain(path.iter()).join(".")
}
