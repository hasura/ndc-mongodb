use mongodb::bson::{bson, Document};

use dc_api_types::{OrderBy, OrderByTarget, OrderDirection};

pub fn make_sort(order_by: &OrderBy) -> Document {
    let OrderBy {
        elements,
        relations: _,
    } = order_by;

    elements
        .clone()
        .iter()
        .filter_map(|obe| {
            let direction = match obe.clone().order_direction {
                OrderDirection::Asc => bson!(1),
                OrderDirection::Desc => bson!(-1),
            };
            match obe.target {
                OrderByTarget::Column { ref column } => Some((column.as_path(), direction)),
                OrderByTarget::SingleColumnAggregate {
                    column: _,
                    function: _,
                    result_type: _,
                } => None,
                OrderByTarget::StarCountAggregate {} => None,
            }
        })
        .collect()
}
