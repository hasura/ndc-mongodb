pub mod field;
mod query;
mod relationships;
mod type_helpers;

use std::{collections::BTreeMap, fmt::Display};

use enum_iterator::Sequence;
use lazy_static::lazy_static;
use ndc::TypeRepresentation;
use ndc_models as ndc;
use ndc_test_helpers::{
    array_of, collection, make_primary_key_uniqueness_constraint, named_type, nullable,
};

use crate::{ConnectorTypes, QueryContext, QueryPlanError, Type};

#[allow(unused_imports)]
pub use self::{
    query::{query, QueryBuilder},
    relationships::{relationship, RelationshipBuilder},
    type_helpers::{date, double, int, object_type, string},
};

#[derive(Clone, Debug, Default)]
pub struct TestContext {
    pub collections: BTreeMap<ndc::CollectionName, ndc::CollectionInfo>,
    pub functions: BTreeMap<ndc::FunctionName, (ndc::FunctionInfo, ndc::CollectionInfo)>,
    pub procedures: BTreeMap<ndc::ProcedureName, ndc::ProcedureInfo>,
    pub object_types: BTreeMap<ndc::ObjectTypeName, ndc::ObjectType>,
}

impl ConnectorTypes for TestContext {
    type AggregateFunction = AggregateFunction;
    type ComparisonOperator = ComparisonOperator;
    type ScalarType = ScalarType;
}

impl QueryContext for TestContext {
    fn lookup_scalar_type(type_name: &ndc::ScalarTypeName) -> Option<Self::ScalarType> {
        ScalarType::find_by_name(type_name.as_str())
    }

    fn lookup_aggregation_function(
        &self,
        input_type: &Type<Self::ScalarType>,
        function_name: &ndc::AggregateFunctionName,
    ) -> Result<(Self::AggregateFunction, &ndc::AggregateFunctionDefinition), QueryPlanError> {
        let function =
            AggregateFunction::find_by_name(function_name.as_str()).ok_or_else(|| {
                QueryPlanError::UnknownAggregateFunction {
                    aggregate_function: function_name.to_owned(),
                }
            })?;
        let definition = scalar_type_name(input_type)
            .and_then(|name| SCALAR_TYPES.get(name))
            .and_then(|scalar_type_def| scalar_type_def.aggregate_functions.get(function_name))
            .ok_or_else(|| QueryPlanError::UnknownAggregateFunction {
                aggregate_function: function_name.to_owned(),
            })?;
        Ok((function, definition))
    }

    fn lookup_comparison_operator(
        &self,
        left_operand_type: &Type<Self::ScalarType>,
        operator_name: &ndc::ComparisonOperatorName,
    ) -> Result<(Self::ComparisonOperator, &ndc::ComparisonOperatorDefinition), QueryPlanError>
    where
        Self: Sized,
    {
        let operator = ComparisonOperator::find_by_name(operator_name.as_str())
            .ok_or_else(|| QueryPlanError::UnknownComparisonOperator(operator_name.to_owned()))?;
        let definition = scalar_type_name(left_operand_type)
            .and_then(|name| SCALAR_TYPES.get(name))
            .and_then(|scalar_type_def| scalar_type_def.comparison_operators.get(operator_name))
            .ok_or_else(|| QueryPlanError::UnknownComparisonOperator(operator_name.to_owned()))?;
        Ok((operator, definition))
    }

    fn collections(&self) -> &BTreeMap<ndc::CollectionName, ndc::CollectionInfo> {
        &self.collections
    }

    fn functions(&self) -> &BTreeMap<ndc::FunctionName, (ndc::FunctionInfo, ndc::CollectionInfo)> {
        &self.functions
    }

    fn object_types(&self) -> &BTreeMap<ndc::ObjectTypeName, ndc::ObjectType> {
        &self.object_types
    }

    fn procedures(&self) -> &BTreeMap<ndc::ProcedureName, ndc::ProcedureInfo> {
        &self.procedures
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Sequence)]
pub enum AggregateFunction {
    Average,
}

impl NamedEnum for AggregateFunction {
    fn name(self) -> &'static str {
        match self {
            AggregateFunction::Average => "Average",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Sequence)]
pub enum ComparisonOperator {
    Equal,
    Regex,
}

impl NamedEnum for ComparisonOperator {
    fn name(self) -> &'static str {
        match self {
            ComparisonOperator::Equal => "Equal",
            ComparisonOperator::Regex => "Regex",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Sequence)]
pub enum ScalarType {
    Bool,
    Date,
    Double,
    Int,
    String,
}

impl NamedEnum for ScalarType {
    fn name(self) -> &'static str {
        match self {
            ScalarType::Bool => "Bool",
            ScalarType::Date => "Date",
            ScalarType::Double => "Double",
            ScalarType::Int => "Int",
            ScalarType::String => "String",
        }
    }
}

impl Display for ScalarType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

trait NamedEnum {
    fn name(self) -> &'static str;
    fn find_by_name(name: &str) -> Option<Self>
    where
        Self: Clone + Sequence,
    {
        enum_iterator::all::<Self>().find(|s| s.clone().name() == name)
    }
}

fn scalar_type_name(t: &Type<ScalarType>) -> Option<&'static str> {
    match t {
        Type::Scalar(s) => Some(s.name()),
        Type::Nullable(t) => scalar_type_name(t),
        _ => None,
    }
}

fn scalar_types() -> BTreeMap<String, ndc::ScalarType> {
    [
        (
            ScalarType::Double.name().to_owned(),
            ndc::ScalarType {
                representation: Some(TypeRepresentation::Float64),
                aggregate_functions: [(
                    AggregateFunction::Average.name().into(),
                    ndc::AggregateFunctionDefinition {
                        result_type: ndc::Type::Named {
                            name: ScalarType::Double.name().into(),
                        },
                    },
                )]
                .into(),
                comparison_operators: [(
                    ComparisonOperator::Equal.name().into(),
                    ndc::ComparisonOperatorDefinition::Equal,
                )]
                .into(),
            },
        ),
        (
            ScalarType::Int.name().to_owned(),
            ndc::ScalarType {
                representation: Some(TypeRepresentation::Int32),
                aggregate_functions: [(
                    AggregateFunction::Average.name().into(),
                    ndc::AggregateFunctionDefinition {
                        result_type: ndc::Type::Named {
                            name: ScalarType::Double.name().into(),
                        },
                    },
                )]
                .into(),
                comparison_operators: [(
                    ComparisonOperator::Equal.name().into(),
                    ndc::ComparisonOperatorDefinition::Equal,
                )]
                .into(),
            },
        ),
        (
            ScalarType::String.name().to_owned(),
            ndc::ScalarType {
                representation: Some(TypeRepresentation::String),
                aggregate_functions: Default::default(),
                comparison_operators: [
                    (
                        ComparisonOperator::Equal.name().into(),
                        ndc::ComparisonOperatorDefinition::Equal,
                    ),
                    (
                        ComparisonOperator::Regex.name().into(),
                        ndc::ComparisonOperatorDefinition::Custom {
                            argument_type: named_type(ScalarType::String),
                        },
                    ),
                ]
                .into(),
            },
        ),
    ]
    .into()
}

lazy_static! {
    static ref SCALAR_TYPES: BTreeMap<String, ndc::ScalarType> = scalar_types();
}

pub fn make_flat_schema() -> TestContext {
    TestContext {
        collections: BTreeMap::from([
            (
                "authors".into(),
                ndc::CollectionInfo {
                    name: "authors".into(),
                    description: None,
                    collection_type: "Author".into(),
                    arguments: Default::default(),
                    uniqueness_constraints: make_primary_key_uniqueness_constraint("authors"),
                    foreign_keys: Default::default(),
                },
            ),
            (
                "articles".into(),
                ndc::CollectionInfo {
                    name: "articles".into(),
                    description: None,
                    collection_type: "Article".into(),
                    arguments: Default::default(),
                    uniqueness_constraints: make_primary_key_uniqueness_constraint("articles"),
                    foreign_keys: Default::default(),
                },
            ),
        ]),
        functions: Default::default(),
        object_types: BTreeMap::from([
            (
                "Author".into(),
                ndc_test_helpers::object_type([
                    ("id", named_type(ScalarType::Int)),
                    ("last_name", named_type(ScalarType::String)),
                ]),
            ),
            (
                "Article".into(),
                ndc_test_helpers::object_type([
                    ("author_id", named_type(ScalarType::Int)),
                    ("title", named_type(ScalarType::String)),
                    ("year", nullable(named_type(ScalarType::Int))),
                ]),
            ),
        ]),
        procedures: Default::default(),
    }
}

pub fn make_nested_schema() -> TestContext {
    TestContext {
        collections: BTreeMap::from([
            (
                "authors".into(),
                ndc::CollectionInfo {
                    name: "authors".into(),
                    description: None,
                    collection_type: "Author".into(),
                    arguments: Default::default(),
                    uniqueness_constraints: make_primary_key_uniqueness_constraint("authors"),
                    foreign_keys: Default::default(),
                },
            ),
            collection("appearances"), // new helper gives more concise syntax
        ]),
        functions: Default::default(),
        object_types: BTreeMap::from([
            (
                "Author".into(),
                ndc_test_helpers::object_type([
                    ("name", named_type(ScalarType::String)),
                    ("address", named_type("Address")),
                    ("articles", array_of(named_type("Article"))),
                    ("array_of_arrays", array_of(array_of(named_type("Article")))),
                ]),
            ),
            (
                "Address".into(),
                ndc_test_helpers::object_type([
                    ("country", named_type(ScalarType::String)),
                    ("street", named_type(ScalarType::String)),
                    ("apartment", nullable(named_type(ScalarType::String))),
                    ("geocode", nullable(named_type("Geocode"))),
                ]),
            ),
            (
                "Article".into(),
                ndc_test_helpers::object_type([("title", named_type(ScalarType::String))]),
            ),
            (
                "Geocode".into(),
                ndc_test_helpers::object_type([
                    ("latitude", named_type(ScalarType::Double)),
                    ("longitude", named_type(ScalarType::Double)),
                ]),
            ),
            (
                "appearances".into(),
                ndc_test_helpers::object_type([("authorId", named_type(ScalarType::Int))]),
            ),
        ]),
        procedures: Default::default(),
    }
}
