use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

use ndc_models as ndc;

use crate::{
    plan_for_query_request::helpers::lookup_relationship,
    query_plan::{Scope, UnrelatedJoin, VariableTypes},
    vec_set::VecSet,
    ConnectorTypes, Query, QueryContext, QueryPlanError, Relationship, Type,
};

use super::{
    plan_for_arguments::plan_for_relationship_arguments,
    unify_relationship_references::unify_relationship_references,
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
    pub collection_relationships: &'a BTreeMap<ndc::RelationshipName, ndc::Relationship>,
    pub scope: Scope,
    relationships: BTreeMap<ndc_models::RelationshipName, Relationship<T>>,
    unrelated_joins: Rc<RefCell<BTreeMap<String, UnrelatedJoin<T>>>>,
    relationship_name_counter: Rc<Cell<i32>>,
    scope_name_counter: Rc<Cell<i32>>,
    variable_types: Rc<RefCell<VariableTypes<T::ScalarType>>>,
}

impl<T: QueryContext> QueryPlanState<'_, T> {
    pub fn new<'a>(
        query_context: &'a T,
        collection_relationships: &'a BTreeMap<ndc::RelationshipName, ndc::Relationship>,
    ) -> QueryPlanState<'a, T> {
        QueryPlanState {
            context: query_context,
            collection_relationships,
            scope: Scope::Root,
            relationships: Default::default(),
            unrelated_joins: Rc::new(RefCell::new(Default::default())),
            relationship_name_counter: Rc::new(Cell::new(0)),
            scope_name_counter: Rc::new(Cell::new(0)),
            variable_types: Rc::new(RefCell::new(Default::default())),
        }
    }

    /// When traversing a query request into a sub-query we enter a new scope for relationships.
    /// Use this function to get a new plan for the new scope. Shares query-request-level state
    /// with the parent plan.
    pub fn state_for_subquery(&self) -> QueryPlanState<'_, T> {
        QueryPlanState {
            context: self.context,
            collection_relationships: self.collection_relationships,
            scope: self.scope.clone(),
            relationships: Default::default(),
            unrelated_joins: self.unrelated_joins.clone(),
            relationship_name_counter: self.relationship_name_counter.clone(),
            scope_name_counter: self.scope_name_counter.clone(),
            variable_types: self.variable_types.clone(),
        }
    }

    pub fn new_scope(&mut self) {
        let name = self.unique_scope_name();
        self.scope = Scope::Named(name)
    }

    /// Record a relationship reference so that it is added to the list of joins for the query
    /// plan, and get back an identifier than can be used to access the joined collection.
    pub fn register_relationship(
        &mut self,
        ndc_relationship_name: ndc::RelationshipName,
        arguments: BTreeMap<ndc::ArgumentName, ndc::RelationshipArgument>,
        query: Query<T>,
    ) -> Result<ndc::RelationshipName> {
        let ndc_relationship =
            lookup_relationship(self.collection_relationships, &ndc_relationship_name)?;

        let arguments = if !arguments.is_empty() {
            let collection = self
                .context
                .find_collection(&ndc_relationship.target_collection)?;
            plan_for_relationship_arguments(self, &collection.arguments, arguments)?
        } else {
            Default::default()
        };

        let relationship = Relationship {
            column_mapping: ndc_relationship.column_mapping.clone(),
            relationship_type: ndc_relationship.relationship_type,
            target_collection: ndc_relationship.target_collection.clone(),
            arguments,
            query,
        };

        let (key, relationship) = match self.relationships.remove_entry(&ndc_relationship_name) {
            Some((existing_key, already_registered_relationship)) => {
                match unify_relationship_references(
                    already_registered_relationship.clone(),
                    relationship.clone(),
                ) {
                    Ok(unified_relationship) => (ndc_relationship_name, unified_relationship),
                    Err(_) => {
                        // If relationships couldn't be unified then we need to store the new
                        // relationship under a new key. We also need to put back the existing
                        // relationship that we just removed.
                        self.relationships
                            .insert(existing_key, already_registered_relationship);
                        let key = self.unique_relationship_name(ndc_relationship_name).into();
                        (key, relationship)
                    }
                }
            }
            None => (ndc_relationship_name, relationship),
        };

        self.relationships.insert(key.clone(), relationship);

        Ok(key)
    }

    /// Record a collection reference so that it is added to the list of joins for the query
    /// plan, and get back an identifier than can be used to access the joined collection.
    pub fn register_unrelated_join(
        &mut self,
        target_collection: ndc::CollectionName,
        arguments: BTreeMap<ndc::ArgumentName, ndc::RelationshipArgument>,
        query: Query<T>,
    ) -> Result<String> {
        let arguments = if !arguments.is_empty() {
            let collection = self.context.find_collection(&target_collection)?;
            plan_for_relationship_arguments(self, &collection.arguments, arguments)?
        } else {
            Default::default()
        };

        let join = UnrelatedJoin {
            target_collection,
            arguments,
            query,
        };

        let key = self.unique_relationship_name(format!("__join_{}", join.target_collection));
        self.unrelated_joins.borrow_mut().insert(key.clone(), join);

        // Unlike [Self::register_relationship] this method does not return a reference to the
        // registered join. If we need that reference then we need another [RefCell::borrow] call
        // here, and we need to return the [std::cell::Ref] value that is produced. (We can't
        // borrow map values through a RefCell without keeping a live Ref.) But if that Ref is
        // still alive the next time [Self::register_unrelated_join] is called then the borrow_mut
        // call will fail.
        Ok(key)
    }

    /// It's important to call this for every use of a variable encountered when building
    /// a [crate::QueryPlan] so we can capture types for each variable.
    pub fn register_variable_use(
        &mut self,
        variable_name: &ndc::VariableName,
        expected_type: Type<T::ScalarType>,
    ) {
        // self.register_variable_use_helper(variable_name, Some(expected_type))
        let mut type_map = self.variable_types.borrow_mut();
        match type_map.get_mut(variable_name) {
            None => {
                type_map.insert(variable_name.clone(), VecSet::singleton(expected_type));
            }
            Some(entry) => {
                entry.insert(expected_type);
            }
        }
    }

    /// Use this for subquery plans to get the relationships for each sub-query
    pub fn into_relationships(self) -> BTreeMap<ndc::RelationshipName, Relationship<T>> {
        self.relationships
    }

    pub fn into_scope(self) -> Scope {
        self.scope
    }

    /// Use this with the top-level plan to get unrelated joins and variable types
    pub fn into_query_plan_info(self) -> QueryPlanInfo<T> {
        QueryPlanInfo {
            unrelated_joins: self.unrelated_joins.take(),
            variable_types: self.variable_types.take(),
        }
    }

    fn unique_relationship_name(&mut self, name: impl std::fmt::Display) -> String {
        let count = self.relationship_name_counter.get();
        self.relationship_name_counter.set(count + 1);
        format!("{name}_{count}")
    }

    fn unique_scope_name(&mut self) -> String {
        let count = self.scope_name_counter.get();
        self.scope_name_counter.set(count + 1);
        format!("scope_{count}")
    }
}

/// Data extracted from [QueryPlanState] for use in building top-level [crate::QueryPlan]
#[derive(Debug)]
pub struct QueryPlanInfo<T: ConnectorTypes> {
    pub unrelated_joins: BTreeMap<String, UnrelatedJoin<T>>,
    pub variable_types: VariableTypes<T::ScalarType>,
}
