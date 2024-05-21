use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    ops::Deref,
    rc::Rc,
};

use ndc::RelationshipArgument;
use ndc_models as ndc;

use crate::{
    plan_for_query_request::helpers::lookup_relationship, query_plan::UnrelatedJoin, Query,
    QueryContext, QueryPlanError, Relationship,
};

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
    pub fn state_for_subquery<'a>(&'a self) -> QueryPlanState<'a, T> {
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
    pub fn register_relationship<'a>(
        &'a mut self,
        ndc_relationship_name: String,
        arguments: BTreeMap<String, RelationshipArgument>,
        query: Query<T>,
    ) -> Result<(&'a str, &'a Relationship<T>)> {
        let ndc_relationship =
            lookup_relationship(self.collection_relationships, &ndc_relationship_name)?;

        let relationship = Relationship {
            column_mapping: ndc_relationship.column_mapping.clone(),
            relationship_type: ndc_relationship.relationship_type,
            target_collection: ndc_relationship.target_collection.clone(),
            arguments,
            query,
        };

        // We want to insert the relationship into the internal map if there isn't already
        // a matching relationship in there, then return references to the key and to the
        // relationship. Lifetime analysis makes this fiddly. We get the key (creating one if
        // necessary), and then do the lookup.

        let key = match self.find_matching_relationship_key(&relationship) {
            Some(key) => key,
            None => {
                let key = self.unique_name(ndc_relationship_name);
                self.relationships.insert(key.clone(), relationship);
                key
            }
        };

        // Safety: we just inserted this key if it wasn't already present
        let (key, relationship) = self.relationships.get_key_value(&key).unwrap();
        Ok((key, relationship))
    }

    fn find_matching_relationship_key<'a>(
        &'a self,
        relationship: &Relationship<T>,
    ) -> Option<String> {
        self.relationships.iter().find_map(|(key, rel)| {
            if rel == relationship {
                Some(key.clone())
            } else {
                None
            }
        })
    }

    /// Record a collection reference so that it is added to the list of joins for the query
    /// plan, and get back an identifier than can be used to access the joined collection.
    pub fn register_unrelated_join<'a>(
        &'a mut self,
        target_collection: String,
        arguments: BTreeMap<String, RelationshipArgument>,
        query: Query<T>,
    ) -> String {
        let join = UnrelatedJoin {
            target_collection,
            arguments,
            query,
        };

        let matching_key = {
            let unrelated_joins = RefCell::borrow(&self.unrelated_joins);
            Self::find_matching_join_key(unrelated_joins, &join)
        };

        let key = match matching_key {
            Some(key) => key,
            None => {
                let key = self.unique_name(format!("__join_{}", join.target_collection));
                self.unrelated_joins.borrow_mut().insert(key.clone(), join);
                key
            }
        };

        // Unlike [Self::register_relationship] this method does not return a reference to the
        // registered join. If we need that reference then we need another [RefCell::borrow] call
        // here, and we need to return the [std::cell::Ref] value that is produced. (We can't
        // borrow map values through a RefCell without keeping a live Ref.) But if that Ref is
        // still alive the next time [Self::register_unrelated_join] is called then the borrow_mut
        // call will fail.
        key
    }

    fn find_matching_join_key<'a>(
        registered_joins: impl Deref<Target = BTreeMap<String, UnrelatedJoin<T>>>,
        join: &UnrelatedJoin<T>,
    ) -> Option<String> {
        registered_joins.iter().find_map(
            |(key, jn)| {
                if jn == join {
                    Some(key.clone())
                } else {
                    None
                }
            },
        )
    }

    /// Use this for subquery plans to get the relationships for each sub-query
    pub fn into_relationships(self) -> BTreeMap<String, Relationship<T>> {
        self.relationships
    }

    /// Use this with the top-level plan to get unrelated joins.
    pub fn into_unrelated_collections(self) -> BTreeMap<String, UnrelatedJoin<T>> {
        self.unrelated_joins.take()
    }

    // pub fn into_join_plan(self) -> JoinPlan<T> {
    //     JoinPlan {
    //         relationships: self.relationships.into_iter().collect(),
    //         unrelated_joins: self.unrelated_joins.take().into_iter().collect(),
    //     }
    // }

    fn unique_name(&mut self, name: String) -> String {
        let count = self.counter.get();
        self.counter.set(count + 1);
        format!("{name}_{count}")
    }
}

// pub struct JoinPlan<T: ConnectorTypes> {
//     pub relationships: BTreeMap<String, Relationship<T>>,
//     pub unrelated_joins: BTreeMap<String, UnrelatedJoin<T>>,
// }
