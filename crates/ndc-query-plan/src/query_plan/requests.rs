use std::collections::BTreeMap;

use derivative::Derivative;
use indexmap::IndexMap;
use ndc_models::{self as ndc, RelationshipType};
use nonempty::NonEmpty;

use crate::{vec_set::VecSet, Type};

use super::{Aggregate, ConnectorTypes, Expression, Field, Grouping, OrderBy};

#[derive(Derivative)]
#[derivative(
    Clone(bound = ""),
    Debug(bound = ""),
    PartialEq(bound = "T::ScalarType: PartialEq")
)]
pub struct QueryPlan<T: ConnectorTypes> {
    pub collection: ndc::CollectionName,
    pub query: Query<T>,
    pub arguments: BTreeMap<ndc::ArgumentName, Argument<T>>,
    pub variables: Option<Vec<VariableSet>>,

    /// Types for values from the `variables` map as inferred by usages in the query request. It is
    /// possible for the same variable to be used in multiple contexts with different types. This
    /// map provides sets of all observed types.
    ///
    /// The observed type may be `None` if the type of a variable use could not be inferred.
    pub variable_types: VariableTypes<T::ScalarType>,

    // TODO: type for unrelated collection
    pub unrelated_collections: BTreeMap<String, UnrelatedJoin<T>>,
}

impl<T: ConnectorTypes> QueryPlan<T> {
    pub fn has_variables(&self) -> bool {
        self.variables.is_some()
    }
}

pub type Relationships<T> = BTreeMap<ndc::RelationshipName, Relationship<T>>;
pub type VariableSet = BTreeMap<ndc::VariableName, serde_json::Value>;
pub type VariableTypes<T> = BTreeMap<ndc::VariableName, VecSet<Type<T>>>;

#[derive(Derivative)]
#[derivative(
    Clone(bound = ""),
    Debug(bound = ""),
    Default(bound = ""),
    PartialEq(bound = "")
)]
pub struct Query<T: ConnectorTypes> {
    pub aggregates: Option<IndexMap<ndc::FieldName, Aggregate<T>>>,
    pub fields: Option<IndexMap<ndc::FieldName, Field<T>>>,
    pub limit: Option<u32>,
    pub aggregates_limit: Option<u32>,
    pub offset: Option<u32>,
    pub order_by: Option<OrderBy<T>>,
    pub predicate: Option<Expression<T>>,
    pub groups: Option<Grouping<T>>,

    /// Relationships referenced by fields and expressions in this query or sub-query. Does not
    /// include relationships in sub-queries nested under this one.
    pub relationships: Relationships<T>,

    /// Some relationship references may introduce a named "scope" so that other parts of the query
    /// request can reference fields of documents in the related collection. The connector must
    /// introduce a variable, or something similar, for such references.
    pub scope: Option<Scope>,
}

impl<T: ConnectorTypes> Query<T> {
    pub fn has_aggregates(&self) -> bool {
        if let Some(aggregates) = &self.aggregates {
            !aggregates.is_empty()
        } else {
            false
        }
    }

    pub fn has_fields(&self) -> bool {
        if let Some(fields) = &self.fields {
            !fields.is_empty()
        } else {
            false
        }
    }
}

#[derive(Derivative)]
#[derivative(
    Clone(bound = ""),
    Debug(bound = ""),
    Hash(bound = ""),
    PartialEq(bound = ""),
    Eq(bound = "")
)]
pub enum Argument<T: ConnectorTypes> {
    /// The argument is provided by reference to a variable
    Variable {
        name: ndc::VariableName,
        argument_type: Type<T::ScalarType>,
    },
    /// The argument is provided as a literal value
    Literal {
        value: serde_json::Value,
        argument_type: Type<T::ScalarType>,
    },
    /// The argument was a literal value that has been parsed as an [Expression]
    Predicate { expression: Expression<T> },
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub struct Relationship<T: ConnectorTypes> {
    /// A mapping between columns on the source row to columns on the target collection.
    /// The column on the target collection is specified via a field path (ie. an array of field
    /// names that descend through nested object fields). The field path will only contain a single item,
    /// meaning a column on the target collection's type, unless the 'relationships.nested'
    /// capability is supported, in which case multiple items denotes a nested object field.
    pub column_mapping: BTreeMap<ndc::FieldName, NonEmpty<ndc::FieldName>>,
    pub relationship_type: RelationshipType,
    /// The name of a collection
    pub target_collection: ndc::CollectionName,
    /// Values to be provided to any collection arguments
    pub arguments: BTreeMap<ndc::ArgumentName, RelationshipArgument<T>>,
    pub query: Query<T>,
}

#[derive(Derivative)]
#[derivative(
    Clone(bound = ""),
    Debug(bound = ""),
    PartialEq(bound = "T::ScalarType: PartialEq")
)]
pub enum RelationshipArgument<T: ConnectorTypes> {
    /// The argument is provided by reference to a variable
    Variable {
        name: ndc::VariableName,
        argument_type: Type<T::ScalarType>,
    },
    /// The argument is provided as a literal value
    Literal {
        value: serde_json::Value,
        argument_type: Type<T::ScalarType>,
    },
    // The argument is provided based on a column of the source collection
    Column {
        name: ndc::FieldName,
        argument_type: Type<T::ScalarType>,
    },
    /// The argument was a literal value that has been parsed as an [Expression]
    Predicate { expression: Expression<T> },
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub struct UnrelatedJoin<T: ConnectorTypes> {
    pub target_collection: ndc::CollectionName,
    pub arguments: BTreeMap<ndc::ArgumentName, RelationshipArgument<T>>,
    pub query: Query<T>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    Root,
    Named(String),
}
