use ndc_models as ndc;
use vector_map::VecMap;

use crate::{ConnectorTypes, QueryContext, QueryPlanError, Relationship};

type Result<T> = std::result::Result<T, QueryPlanError>;

pub struct QueryPlanState<'a, T: ConnectorTypes> {
    query_context: &'a QueryContext<'a, T>,

    // We're using [VecMap] here because it works with key types that implement [PartialEq].
    relationships: VecMap<(String, ndc::Query), (String, Relationship<T>)>,
}

impl<T: ConnectorTypes> QueryPlanState<'_, T> {
    pub fn new<'a>(query_context: &'a QueryContext<'a, T>) -> QueryPlanState<'a, T> {
        QueryPlanState {
            query_context,
            relationships: VecMap::new(),
        }
    }

    pub fn register_relationship<'a>(
        &'a mut self,
        ndc_relationship_name: &str,
        ndc_query: &ndc::Query,
    ) -> Result<(&'a str, &'a Relationship<T>)> {
        if let Some((relationship_key, plan_relationship)) = self
            .relationships
            .get(&(ndc_relationship_name.to_owned(), ndc_query.clone()))
        {
            return Ok((relationship_key, plan_relationship));
        }
        // TODO: Generate a unique name for the relationship, query pair; convert to a plan types;
        // insert in map and return references
        todo!()
    }
}
