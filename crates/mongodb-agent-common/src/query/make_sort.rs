use anyhow::anyhow;
use mongodb::bson::{bson, Document};
use ndc_models::OrderDirection;

use crate::{column_ref::ColumnRef, interface_types::MongoAgentError, mongo_query_plan::OrderBy};

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
            let column_ref = ColumnRef::from_order_by_target(&obe.target)?;
            match column_ref {
                ColumnRef::MatchKey(key) => Ok((key.to_string(), direction)),
                // TODO: NDC-176
                ColumnRef::Expression(_) => Err(MongoAgentError::BadQuery(anyhow!("sorting by field names that contain dollar signs or dots is not yet supported."))),
            }
        })
        .collect()
}
