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
    FieldsOnly(&'a IndexMap<FieldName, Field>),
    GroupsOnly(&'a Grouping),
}

impl ResponseFacets<'_> {
    pub fn from_parameters<'a>(
        aggregates: Option<&'a IndexMap<FieldName, Aggregate>>,
        fields: Option<&'a IndexMap<FieldName, Field>>,
        groups: Option<&'a Grouping>,
    ) -> ResponseFacets<'a> {
        let aggregates_score = if has_aggregates(aggregates) { 2 } else { 0 };
        let fields_score = if has_fields(fields) { 1 } else { 0 };
        let groups_score = if has_groups(groups) { 1 } else { 0 };

        if aggregates_score + fields_score + groups_score > 1 {
            ResponseFacets::Combination {
                aggregates: if has_aggregates(aggregates) {
                    aggregates
                } else {
                    None
                },
                fields: if has_fields(fields) { fields } else { None },
                groups: if has_groups(groups) { groups } else { None },
            }
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

/// A query that includes aggregates will be run using a $facet pipeline stage. A query that
/// combines two ore more of rows, groups, and aggregates will also use facets. The choice affects
/// how result rows are mapped to a QueryResponse.
///
/// If we have aggregate pipelines they should be combined with the fields pipeline (if there is
/// one) in a single facet stage. If we have fields, and no aggregates then the fields pipeline
/// can instead be appended to `pipeline`.
pub fn is_response_faceted(query: &Query) -> bool {
    match ResponseFacets::from_query(query) {
        ResponseFacets::Combination { .. } => true,
        _ => false,
    }
}

fn has_aggregates(aggregates: Option<&IndexMap<FieldName, Aggregate>>) -> bool {
    if let Some(aggregates) = aggregates {
        !aggregates.is_empty()
    } else {
        false
    }
}

fn has_fields(fields: Option<&IndexMap<FieldName, Field>>) -> bool {
    if let Some(fields) = fields {
        !fields.is_empty()
    } else {
        false
    }
}

fn has_groups(groups: Option<&Grouping>) -> bool {
    groups.is_some()
}
