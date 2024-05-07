use std::collections::BTreeMap;

use ndc_models as ndc;

use super::query_plan_error::QueryPlanError;

pub fn lookup_relationship<'a>(
    relationships: &'a BTreeMap<String, ndc::Relationship>,
    relationship: &str,
) -> Result<&'a ndc::Relationship, QueryPlanError> {
    relationships
        .get(relationship)
        .ok_or_else(|| QueryPlanError::UnspecifiedRelation(relationship.to_owned()))
}
