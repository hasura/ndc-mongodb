use std::{collections::BTreeMap, iter::once};

use itertools::join;
use mongodb::bson::bson;
use mongodb_support::aggregate::{SortDocument, Stage};
use ndc_models::OrderDirection;

use crate::{
    interface_types::MongoAgentError,
    mongo_query_plan::{OrderBy, OrderByTarget},
    mongodb::sanitize::escape_invalid_variable_chars,
};

use super::column_ref::ColumnRef;

/// In a [SortDocument] there is no way to reference field names that need to be escaped, such as
/// names that begin with dollar signs. To sort on such fields we need to insert an $addFields
/// stage _before_ the $sort stage to map safe aliases.
type RequiredAliases<'a> = BTreeMap<String, ColumnRef<'a>>;

type Result<T> = std::result::Result<T, MongoAgentError>;

pub fn make_sort_stages(order_by: &OrderBy) -> Result<Vec<Stage>> {
    let (sort_document, required_aliases) = make_sort(order_by)?;
    let mut stages = vec![];

    if !required_aliases.is_empty() {
        let fields = required_aliases
            .into_iter()
            .map(|(alias, expression)| (alias, expression.into_aggregate_expression().into_bson()))
            .collect();
        let stage = Stage::AddFields(fields);
        stages.push(stage);
    }

    let sort_stage = Stage::Sort(sort_document);
    stages.push(sort_stage);

    Ok(stages)
}

fn make_sort(order_by: &OrderBy) -> Result<(SortDocument, RequiredAliases<'_>)> {
    let OrderBy { elements } = order_by;

    let keys_directions_expressions: BTreeMap<String, (OrderDirection, Option<ColumnRef<'_>>)> =
        elements
            .iter()
            .map(|obe| {
                let col_ref = ColumnRef::from_order_by_target(&obe.target)?;
                let (key, required_alias) = match col_ref {
                    ColumnRef::MatchKey(key) => (key.to_string(), None),
                    ref_expr => (safe_alias(&obe.target)?, Some(ref_expr)),
                };
                Ok((key, (obe.order_direction, required_alias)))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;

    let sort_document = keys_directions_expressions
        .iter()
        .map(|(key, (direction, _))| {
            let direction_bson = match direction {
                OrderDirection::Asc => bson!(1),
                OrderDirection::Desc => bson!(-1),
            };
            (key.clone(), direction_bson)
        })
        .collect();

    let required_aliases = keys_directions_expressions
        .into_iter()
        .flat_map(|(key, (_, expr))| expr.map(|e| (key, e)))
        .collect();

    Ok((SortDocument(sort_document), required_aliases))
}

fn safe_alias(target: &OrderByTarget) -> Result<String> {
    match target {
        ndc_query_plan::OrderByTarget::Column {
            name,
            field_path,
            path,
            ..
        } => {
            let name_and_path = once("__sort_key_")
                .chain(path.iter().map(|n| n.as_str()))
                .chain([name.as_str()])
                .chain(
                    field_path
                        .iter()
                        .flatten()
                        .map(|field_name| field_name.as_str()),
                );
            let combine_all_elements_into_one_name = join(name_and_path, "_");
            Ok(escape_invalid_variable_chars(
                &combine_all_elements_into_one_name,
            ))
        }
        ndc_query_plan::OrderByTarget::Aggregate { .. } => {
            // TODO: ENG-1010, ENG-1011
            Err(MongoAgentError::NotImplemented("order by aggregate".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;
    use mongodb_support::aggregate::SortDocument;
    use ndc_models::{FieldName, OrderDirection};
    use ndc_query_plan::OrderByElement;
    use pretty_assertions::assert_eq;

    use crate::{mongo_query_plan::OrderBy, query::column_ref::ColumnRef};

    use super::make_sort;

    #[test]
    fn escapes_field_names() -> anyhow::Result<()> {
        let order_by = OrderBy {
            elements: vec![OrderByElement {
                order_direction: OrderDirection::Asc,
                target: ndc_query_plan::OrderByTarget::Column {
                    name: "$schema".into(),
                    field_path: Default::default(),
                    path: Default::default(),
                    arguments: Default::default(),
                },
            }],
        };
        let path: [FieldName; 1] = ["$schema".into()];

        let actual = make_sort(&order_by)?;
        let expected_sort_doc = SortDocument(doc! {
            "__sort_key__·24schema": 1
        });
        let expected_aliases = [(
            "__sort_key__·24schema".into(),
            ColumnRef::from_field_path(path.iter()),
        )]
        .into();
        assert_eq!(actual, (expected_sort_doc, expected_aliases));
        Ok(())
    }

    #[test]
    fn escapes_nested_field_names() -> anyhow::Result<()> {
        let order_by = OrderBy {
            elements: vec![OrderByElement {
                order_direction: OrderDirection::Asc,
                target: ndc_query_plan::OrderByTarget::Column {
                    name: "configuration".into(),
                    field_path: Some(vec!["$schema".into()]),
                    path: Default::default(),
                    arguments: Default::default(),
                },
            }],
        };
        let path: [FieldName; 2] = ["configuration".into(), "$schema".into()];

        let actual = make_sort(&order_by)?;
        let expected_sort_doc = SortDocument(doc! {
            "__sort_key__configuration_·24schema": 1
        });
        let expected_aliases = [(
            "__sort_key__configuration_·24schema".into(),
            ColumnRef::from_field_path(path.iter()),
        )]
        .into();
        assert_eq!(actual, (expected_sort_doc, expected_aliases));
        Ok(())
    }
}
