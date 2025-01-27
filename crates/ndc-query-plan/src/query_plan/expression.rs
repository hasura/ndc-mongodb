use std::{borrow::Cow, collections::BTreeMap, iter};

use derivative::Derivative;
use itertools::Either;
use ndc_models::{self as ndc, ArgumentName, FieldName};

use crate::Type;

use super::{Argument, ConnectorTypes};

#[derive(Derivative)]
#[derivative(
    Clone(bound = ""),
    Debug(bound = ""),
    Hash(bound = ""),
    PartialEq(bound = ""),
    Eq(bound = "")
)]
pub enum Expression<T: ConnectorTypes> {
    And {
        expressions: Vec<Expression<T>>,
    },
    Or {
        expressions: Vec<Expression<T>>,
    },
    Not {
        expression: Box<Expression<T>>,
    },
    UnaryComparisonOperator {
        column: ComparisonTarget<T>,
        operator: ndc::UnaryComparisonOperator,
    },
    BinaryComparisonOperator {
        column: ComparisonTarget<T>,
        operator: T::ComparisonOperator,
        value: ComparisonValue<T>,
    },
    /// A comparison against a nested array column.
    /// Only used if the 'query.nested_fields.filter_by.nested_arrays' capability is supported.
    ArrayComparison {
        column: ComparisonTarget<T>,
        comparison: ArrayComparison<T>,
    },
    Exists {
        in_collection: ExistsInCollection<T>,
        predicate: Option<Box<Expression<T>>>,
    },
}

impl<T: ConnectorTypes> Expression<T> {
    /// Get an iterator of columns referenced by the expression, not including columns of related
    /// collections. This is used to build a plan for joining the referenced collection - we need
    /// to include fields in the join that the expression needs to access.
    //
    // TODO: ENG-1457 When we implement query.aggregates.filter_by we'll need to collect aggregates
    // references. That's why this function returns [ComparisonTarget] instead of [Field].
    pub fn query_local_comparison_targets<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = Cow<'a, ComparisonTarget<T>>> + 'a> {
        match self {
            Expression::And { expressions } => Box::new(
                expressions
                    .iter()
                    .flat_map(|e| e.query_local_comparison_targets()),
            ),
            Expression::Or { expressions } => Box::new(
                expressions
                    .iter()
                    .flat_map(|e| e.query_local_comparison_targets()),
            ),
            Expression::Not { expression } => expression.query_local_comparison_targets(),
            Expression::UnaryComparisonOperator { column, .. } => {
                Box::new(std::iter::once(Cow::Borrowed(column)))
            }
            Expression::BinaryComparisonOperator { column, value, .. } => Box::new(
                std::iter::once(Cow::Borrowed(column))
                    .chain(Self::local_targets_from_comparison_value(value).map(Cow::Owned)),
            ),
            Expression::ArrayComparison { column, comparison } => {
                let value_targets = match comparison {
                    ArrayComparison::Contains { value } => Either::Left(
                        Self::local_targets_from_comparison_value(value).map(Cow::Owned),
                    ),
                    ArrayComparison::IsEmpty => Either::Right(std::iter::empty()),
                };
                Box::new(std::iter::once(Cow::Borrowed(column)).chain(value_targets))
            }
            Expression::Exists { .. } => Box::new(iter::empty()),
        }
    }

    fn local_targets_from_comparison_value(
        value: &ComparisonValue<T>,
    ) -> impl Iterator<Item = ComparisonTarget<T>> {
        match value {
            ComparisonValue::Column {
                path,
                name,
                arguments,
                field_path,
                field_type,
                ..
            } => {
                if path.is_empty() {
                    Either::Left(iter::once(ComparisonTarget::Column {
                        name: name.clone(),
                        arguments: arguments.clone(),
                        field_path: field_path.clone(),
                        field_type: field_type.clone(),
                    }))
                } else {
                    Either::Right(iter::empty())
                }
            }
            _ => Either::Right(std::iter::empty()),
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
pub enum ArrayComparison<T: ConnectorTypes> {
    /// Check if the array contains the specified value.
    /// Only used if the 'query.nested_fields.filter_by.nested_arrays.contains' capability is supported.
    Contains { value: ComparisonValue<T> },
    /// Check is the array is empty.
    /// Only used if the 'query.nested_fields.filter_by.nested_arrays.is_empty' capability is supported.
    IsEmpty,
}

#[derive(Derivative)]
#[derivative(
    Clone(bound = ""),
    Debug(bound = ""),
    Hash(bound = ""),
    PartialEq(bound = ""),
    Eq(bound = "")
)]
pub enum ComparisonTarget<T: ConnectorTypes> {
    /// The comparison targets a column.
    Column {
        /// The name of the column
        name: ndc::FieldName,

        /// Arguments to satisfy the column specified by 'name'
        arguments: BTreeMap<ArgumentName, Argument<T>>,

        /// Path to a nested field within an object column
        field_path: Option<Vec<ndc::FieldName>>,

        /// Type of the field that you get *after* follwing `field_path` to a possibly-nested
        /// field.
        field_type: Type<T::ScalarType>,
    },
    // TODO: ENG-1457 Add this variant to support query.aggregates.filter_by
    // /// The comparison targets the result of aggregation.
    // /// Only used if the 'query.aggregates.filter_by' capability is supported.
    // Aggregate {
    //     /// Non-empty collection of relationships to traverse
    //     path: Vec<RelationshipName>,
    //     /// The aggregation method to use
    //     aggregate: Aggregate<T>,
    // },
}

impl<T: ConnectorTypes> ComparisonTarget<T> {
    pub fn column(name: impl Into<ndc::FieldName>, field_type: Type<T::ScalarType>) -> Self {
        Self::Column {
            name: name.into(),
            arguments: Default::default(),
            field_path: Default::default(),
            field_type,
        }
    }

    pub fn target_type(&self) -> &Type<T::ScalarType> {
        match self {
            ComparisonTarget::Column { field_type, .. } => field_type,
            // TODO: ENG-1457
            // ComparisonTarget::Aggregate { aggregate, .. } => aggregate.result_type,
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
pub enum ComparisonValue<T: ConnectorTypes> {
    Column {
        /// Any relationships to traverse to reach this column.
        /// Only non-empty if the 'relationships.relation_comparisons' is supported.
        path: Vec<ndc::RelationshipName>,
        /// The name of the column
        name: ndc::FieldName,
        /// Arguments to satisfy the column specified by 'name'
        arguments: BTreeMap<ArgumentName, Argument<T>>,
        /// Path to a nested field within an object column.
        /// Only non-empty if the 'query.nested_fields.filter_by' capability is supported.
        field_path: Option<Vec<ndc::FieldName>>,
        /// Type of the field that you get *after* follwing `field_path` to a possibly-nested
        /// field.
        field_type: Type<T::ScalarType>,
        /// The scope in which this column exists, identified
        /// by an top-down index into the stack of scopes.
        /// The stack grows inside each `Expression::Exists`,
        /// so scope 0 (the default) refers to the current collection,
        /// and each subsequent index refers to the collection outside
        /// its predecessor's immediately enclosing `Expression::Exists`
        /// expression.
        /// Only used if the 'query.exists.named_scopes' capability is supported.
        scope: Option<usize>,
    },
    Scalar {
        value: serde_json::Value,
        value_type: Type<T::ScalarType>,
    },
    Variable {
        name: ndc::VariableName,
        variable_type: Type<T::ScalarType>,
    },
}

impl<T: ConnectorTypes> ComparisonValue<T> {
    pub fn column(name: impl Into<ndc::FieldName>, field_type: Type<T::ScalarType>) -> Self {
        Self::Column {
            path: Default::default(),
            name: name.into(),
            arguments: Default::default(),
            field_path: Default::default(),
            field_type,
            scope: Default::default(),
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
pub enum ExistsInCollection<T: ConnectorTypes> {
    /// The rows to evaluate the exists predicate against come from a related collection.
    /// Only used if the 'relationships' capability is supported.
    Related {
        /// Key of the relation in the [Query] joins map. Relationships are scoped to the sub-query
        /// that defines the relation source.
        relationship: ndc::RelationshipName,
    },
    /// The rows to evaluate the exists predicate against come from an unrelated collection
    /// Only used if the 'query.exists.unrelated' capability is supported.
    Unrelated {
        /// Key of the relation in the [QueryPlan] joins map. Unrelated collections are not scoped
        /// to a sub-query, instead they are given in the root [QueryPlan].
        unrelated_collection: String,
    },
    /// The rows to evaluate the exists predicate against come from a nested array field.
    /// Only used if the 'query.exists.nested_collections' capability is supported.
    NestedCollection {
        column_name: ndc::FieldName,
        arguments: BTreeMap<ndc::ArgumentName, Argument<T>>,
        /// Path to a nested collection via object columns
        field_path: Vec<ndc::FieldName>,
    },
    /// Specifies a column that contains a nested array of scalars. The
    /// array will be brought into scope of the nested expression where
    /// each element becomes an object with one '__value' column that
    /// contains the element value.
    /// Only used if the 'query.exists.nested_scalar_collections' capability is supported.
    NestedScalarCollection {
        column_name: FieldName,
        arguments: BTreeMap<ArgumentName, Argument<T>>,
        /// Path to a nested collection via object columns
        field_path: Vec<ndc::FieldName>,
    },
}
