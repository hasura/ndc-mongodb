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
    relationships: Vec<(String, Relationship<T>)>,
    unrelated_joins: Rc<RefCell<Vec<(String, UnrelatedJoin<T>)>>>,
    counter: Rc<Cell<i32>>,
}

// impl<'a, T: QueryContext> ConnectorTypes for QueryPlanState<'a, T> {
//     type ScalarType = T::ScalarType;
//     type BinaryOperator = T::BinaryOperator;
// }
//
// impl<'a, T: QueryContext> QueryContext for QueryPlanState<'a, T> {
//     fn lookup_binary_operator(
//         left_operand_type: &crate::Type<Self::ScalarType>,
//         op_name: &str,
//     ) -> Option<Self::BinaryOperator> {
//         T::lookup_binary_operator(left_operand_type, op_name)
//     }
//
//     fn lookup_scalar_type(type_name: &str) -> Option<Self::ScalarType> {
//         T::lookup_scalar_type(type_name)
//     }
//
//     fn comparison_operator_definition(
//         &self,
//         op: &Self::BinaryOperator,
//     ) -> &ComparisonOperatorDefinition<Self>
//     where
//         Self: Sized,
//     {
//         self.context.comparison_operator_definition(op)
//     }
// }

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

    // TODO: We may be able to unify relationships that are not identical, but that are compatible.
    // For example two relationships that differ only in field selection could be merged into one
    // with the union of both field selections.
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

        let matching_relationship = self
            .relationships
            .iter()
            .find(|(_, rel)| rel == &relationship);
        if let Some((key, rel)) = matching_relationship {
            return Ok((key, rel));
        }

        self.relationships
            .push((self.unique_name(ndc_relationship_name), relationship));
        let (key, relationship) = &self.relationships[self.relationships.len() - 1];
        Ok((key, relationship))
    }

    pub fn register_unrelated_join<'a>(
        &'a mut self,
        target_collection: String,
        arguments: BTreeMap<String, RelationshipArgument>,
        query: Query<T>,
    ) -> (&'a str, &'a UnrelatedJoin<T>) {
        // Err(QueryPlanError::NotImplemented("unrelated joins"))

        let join = UnrelatedJoin {
            target_collection,
            arguments,
            query,
        };

        let mut unrelated_joins = self.unrelated_joins.borrow_mut();

        let matching_join = unrelated_joins.iter().find(|(_, jn)| jn == &join);
        if let Some((key, jn)) = matching_join {
            return (key, jn);
        }

        unrelated_joins.push((
            self.unique_name(format!("__join_{}", join.target_collection)),
            join,
        ));
        let (key, join) = &unrelated_joins[unrelated_joins.len() - 1];
        (key, join)
    }

    /// Use this for subquery plans to get the relationships for each sub-query
    pub fn into_relationships(self) -> BTreeMap<String, Relationship<T>> {
        self.relationships.into_iter().collect()
    }

    /// Use this with the top-level plan to get unrelated joins.
    pub fn into_unrelated_collections(self) -> BTreeMap<String, UnrelatedJoin<T>> {
        self.unrelated_joins.take().into_iter().collect()
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
