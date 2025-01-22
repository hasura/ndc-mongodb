use std::collections::BTreeMap;

use derivative::Derivative;
use indexmap::IndexMap;
use ndc_models as ndc;

use crate::Type;

use super::{Aggregate, ConnectorTypes, Expression, RelationshipArgument};

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub enum Field<T: ConnectorTypes> {
    Column {
        column: ndc::FieldName,

        /// When the type of the column is a (possibly-nullable) array or object,
        /// the caller can request a subset of the complete column data,
        /// by specifying fields to fetch here.
        /// If omitted, the column data will be fetched in full.
        fields: Option<NestedField<T>>,

        column_type: Type<T::ScalarType>,
    },
    Relationship {
        /// The name of the relationship to follow for the subquery - this is the key in the
        /// [Query] relationships map in this module, it is **not** the key in the
        /// [ndc::QueryRequest] collection_relationships map.
        relationship: ndc::RelationshipName,
        aggregates: Option<IndexMap<ndc::FieldName, Aggregate<T>>>,
        fields: Option<IndexMap<ndc::FieldName, Field<T>>>,
    },
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub struct NestedObject<T: ConnectorTypes> {
    pub fields: IndexMap<ndc::FieldName, Field<T>>,
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub struct NestedArray<T: ConnectorTypes> {
    pub fields: Box<NestedField<T>>,
}

// TODO: ENG-1464 define NestedCollection struct

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub enum NestedField<T: ConnectorTypes> {
    Object(NestedObject<T>),
    Array(NestedArray<T>),
    // TODO: ENG-1464 add `Collection(NestedCollection)` variant
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""), Debug(bound = ""), PartialEq(bound = ""))]
pub struct PathElement<T: ConnectorTypes> {
    /// Path to a nested field within an object column that must be navigated
    /// before the relationship is navigated.
    /// Only non-empty if the 'relationships.nested' capability is supported.
    pub field_path: Option<Vec<ndc::FieldName>>,
    /// The name of the relationship to follow
    pub relationship: ndc::RelationshipName,
    /// Values to be provided to any collection arguments
    pub arguments: BTreeMap<ndc::ArgumentName, RelationshipArgument<T>>,
    /// A predicate expression to apply to the target collection
    pub predicate: Option<Box<Expression<T>>>,
}
