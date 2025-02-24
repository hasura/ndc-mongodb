//! Centralized logic for query response packing.

use indexmap::IndexMap;
use lazy_static::lazy_static;
use ndc_models::FieldName;

use crate::mongo_query_plan::{Aggregate, Field, Grouping, Query};

lazy_static! {
    static ref DEFAULT_FIELDS: IndexMap<FieldName, Field> = IndexMap::new();
}

/// In some queries we may need to "fork" the query to provide data that requires incompatible
/// pipelines. For example queries that combine two or more of row, group, and aggregates, or
/// queries that use multiple aggregates that use different buckets. In these cases we use the
/// `$facet` aggregation stage which runs multiple sub-pipelines, and stores the results of
/// each in fields of the output pipeline document with array values.
///
/// In other queries we don't need to fork - instead of providing data in a nested array the stream
/// of pipeline output documents is itself the requested data.
///
/// Depending on whether or not a pipeline needs to use `$facet` to fork response processing needs
/// to be done differently.
pub enum ResponseFacets<'a> {
    /// When matching on the Combination variant assume that requested data has already been checked to make sure that maps are not empty.
    Combination {
        aggregates: Option<&'a IndexMap<FieldName, Aggregate>>,
        fields: Option<&'a IndexMap<FieldName, Field>>,
        groups: Option<&'a Grouping>,
    },
    AggregatesOnly(&'a IndexMap<FieldName, Aggregate>),
    FieldsOnly(&'a IndexMap<FieldName, Field>),
    GroupsOnly(&'a Grouping),
}

impl ResponseFacets<'_> {
    pub fn from_parameters<'a>(
        aggregates: Option<&'a IndexMap<FieldName, Aggregate>>,
        fields: Option<&'a IndexMap<FieldName, Field>>,
        groups: Option<&'a Grouping>,
    ) -> ResponseFacets<'a> {
        let facet_score = [
            get_aggregates(aggregates).map(|_| ()),
            get_fields(fields).map(|_| ()),
            get_groups(groups).map(|_| ()),
        ]
        .into_iter()
        .flatten()
        .count();

        if facet_score > 1 {
            ResponseFacets::Combination {
                aggregates: get_aggregates(aggregates),
                fields: get_fields(fields),
                groups: get_groups(groups),
            }
        } else if let Some(aggregates) = aggregates {
            ResponseFacets::AggregatesOnly(aggregates)
        } else if let Some(grouping) = groups {
            ResponseFacets::GroupsOnly(grouping)
        } else {
            ResponseFacets::FieldsOnly(fields.unwrap_or(&DEFAULT_FIELDS))
        }
    }

    pub fn from_query(query: &Query) -> ResponseFacets<'_> {
        Self::from_parameters(
            query.aggregates.as_ref(),
            query.fields.as_ref(),
            query.groups.as_ref(),
        )
    }
}

fn get_aggregates(
    aggregates: Option<&IndexMap<FieldName, Aggregate>>,
) -> Option<&IndexMap<FieldName, Aggregate>> {
    if let Some(aggregates) = aggregates {
        if !aggregates.is_empty() {
            return Some(aggregates);
        }
    }
    None
}

fn get_fields(fields: Option<&IndexMap<FieldName, Field>>) -> Option<&IndexMap<FieldName, Field>> {
    if let Some(fields) = fields {
        if !fields.is_empty() {
            return Some(fields);
        }
    }
    None
}

fn get_groups(groups: Option<&Grouping>) -> Option<&Grouping> {
    groups
}
