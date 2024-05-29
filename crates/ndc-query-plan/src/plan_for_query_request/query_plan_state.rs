use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

use ndc::RelationshipArgument;
use ndc_models as ndc;

use crate::{
    plan_for_query_request::helpers::lookup_relationship, query_plan::UnrelatedJoin, Query,
    QueryContext, QueryPlanError, Relationship,
};

use super::unify_relationship_references::unify_relationship_references;

type Result<T> = std::result::Result<T, QueryPlanError>;

/// Records relationship and other join references in a mutable struct. Relations are scoped to
/// a sub-query (a value of type [Query]), unrelated joins are scoped to the entire query plan.
///
/// This does two things:
/// - Accumulate all of the details needed for joins for each sub-query in one place
/// - Associate an identifier for each join that can be used at each reference site
#[derive(Debug)]
pub struct QueryPlanState<'a, T: QueryContext> {
    pub context: &'a T,
    pub collection_relationships: &'a BTreeMap<String, ndc::Relationship>,
    relationships: BTreeMap<String, Relationship<T>>,
    unrelated_joins: Rc<RefCell<BTreeMap<String, UnrelatedJoin<T>>>>,
    counter: Rc<Cell<i32>>,
}

// TODO: We may be able to unify relationships that are not identical, but that are compatible.
// For example two relationships that differ only in field selection could be merged into one
// with the union of both field selections.

impl<T: QueryContext> QueryPlanState<'_, T> {
    pub fn new<'a>(
        query_context: &'a T,
        collection_relationships: &'a BTreeMap<String, ndc::Relationship>,
    ) -> QueryPlanState<'a, T> {
        QueryPlanState {
            context: query_context,
            collection_relationships,
            relationships: Default::default(),
            unrelated_joins: Rc::new(RefCell::new(Default::default())),
            counter: Rc::new(Cell::new(0)),
        }
    }

    /// When traversing a query request into a sub-query we enter a new scope for relationships.
    /// Use this function to get a new plan for the new scope. Shares query-request-level state
    /// with the parent plan.
    pub fn state_for_subquery(&self) -> QueryPlanState<'_, T> {
        QueryPlanState {
            context: self.context,
            collection_relationships: self.collection_relationships,
            relationships: Default::default(),
            unrelated_joins: self.unrelated_joins.clone(),
            counter: self.counter.clone(),
        }
    }

    /// Record a relationship reference so that it is added to the list of joins for the query
    /// plan, and get back an identifier than can be used to access the joined collection.
    pub fn register_relationship(
        &mut self,
        ndc_relationship_name: String,
        arguments: BTreeMap<String, RelationshipArgument>,
        query: Query<T>,
    ) -> Result<(&str, &Relationship<T>)> {
        let ndc_relationship =
            lookup_relationship(self.collection_relationships, &ndc_relationship_name)?;

        let relationship = Relationship {
            column_mapping: ndc_relationship.column_mapping.clone(),
            relationship_type: ndc_relationship.relationship_type,
            target_collection: ndc_relationship.target_collection.clone(),
            arguments,
            query,
        };

        let relationship = match self.relationships.remove(&ndc_relationship_name) {
            Some(already_registered_relationship) => {
                unify_relationship_references(already_registered_relationship, relationship)?
            }
            None => relationship,
        };

        self.relationships
            .insert(ndc_relationship_name.clone(), relationship);

        // Safety: we just inserted this key
        let (key, relationship) = self
            .relationships
            .get_key_value(&ndc_relationship_name)
            .unwrap();
        Ok((key, relationship))
    }

    /// Record a collection reference so that it is added to the list of joins for the query
    /// plan, and get back an identifier than can be used to access the joined collection.
    pub fn register_unrelated_join(
        &mut self,
        target_collection: String,
        arguments: BTreeMap<String, RelationshipArgument>,
        query: Query<T>,
    ) -> String {
        let join = UnrelatedJoin {
            target_collection,
            arguments,
            query,
        };

        let key = self.unique_name(format!("__join_{}", join.target_collection));
        self.unrelated_joins.borrow_mut().insert(key.clone(), join);

        // Unlike [Self::register_relationship] this method does not return a reference to the
        // registered join. If we need that reference then we need another [RefCell::borrow] call
        // here, and we need to return the [std::cell::Ref] value that is produced. (We can't
        // borrow map values through a RefCell without keeping a live Ref.) But if that Ref is
        // still alive the next time [Self::register_unrelated_join] is called then the borrow_mut
        // call will fail.
        key
    }

    /// Use this for subquery plans to get the relationships for each sub-query
    pub fn into_relationships(self) -> BTreeMap<String, Relationship<T>> {
        self.relationships
    }

    /// Use this with the top-level plan to get unrelated joins.
    pub fn into_unrelated_collections(self) -> BTreeMap<String, UnrelatedJoin<T>> {
        self.unrelated_joins.take()
    }

    fn unique_name(&mut self, name: String) -> String {
        let count = self.counter.get();
        self.counter.set(count + 1);
        format!("{name}_{count}")
    }
}
