use std::{
    collections::{BTreeMap, HashMap},
    ops::Deref,
};

use dc_api_types::{self as v2, ColumnSelector, Target};
use indexmap::IndexMap;
use itertools::Itertools;
use ndc_sdk::models::{self as v3, FunctionInfo, ScalarType};

use super::{
    helpers::{lookup_operator_definition, lookup_relationship},
    query_traversal::{query_traversal, Node, TraversalStep},
    ConversionError,
};

const UNKNOWN_TYPE: &str = "unknown";

#[derive(Clone, Debug)]
pub struct QueryContext {
    pub functions: Vec<FunctionInfo>,
    pub scalar_types: BTreeMap<String, ScalarType>,
}

pub fn v3_to_v2_query_request(
    context: &QueryContext,
    request: v3::QueryRequest,
) -> Result<v2::QueryRequest, ConversionError> {
    Ok(v2::QueryRequest {
        relationships: v3_to_v2_relationships(&request)?,
        target: Target::TTable {
            name: vec![request.collection],
        },
        query: Box::new(v3_to_v2_query(context, request.query)?),

        // We are using v2 types that have been augmented with a `variables` field (even though
        // that is not part of the v2 API). For queries translated from v3 we use `variables`
        // instead of `foreach`.
        foreach: None,
        variables: request.variables,
    })
}

fn v3_to_v2_query(context: &QueryContext, query: v3::Query) -> Result<v2::Query, ConversionError> {
    let aggregates: Option<Option<HashMap<String, v2::Aggregate>>> = query
        .aggregates
        .map(|aggregates| -> Result<_, ConversionError> {
            aggregates
                .into_iter()
                .map(|(name, aggregate)| {
                    Ok((name, v3_to_v2_aggregate(&context.functions, aggregate)?))
                })
                .collect()
        })
        .transpose()?
        .map(Some);

    let fields = v3_to_v2_fields(context, query.fields)?;

    let order_by: Option<Option<v2::OrderBy>> = query
        .order_by
        .map(|order_by| -> Result<_, ConversionError> {
            Ok(v2::OrderBy {
                elements: order_by
                    .elements
                    .into_iter()
                    .map(v3_to_v2_order_by_element)
                    .collect::<Result<_, ConversionError>>()?,
                relations: Default::default(),
            })
        })
        .transpose()?
        .map(Some);

    let limit = optional_32bit_number_to_64bit(query.limit);
    let offset = optional_32bit_number_to_64bit(query.offset);

    Ok(v2::Query {
        aggregates,
        aggregates_limit: limit,
        fields,
        order_by,
        limit,
        offset,
        r#where: query
            .predicate
            .map(|expr| v3_to_v2_expression(&context.scalar_types, expr))
            .transpose()?,
    })
}

fn v3_to_v2_aggregate(
    functions: &[FunctionInfo],
    aggregate: v3::Aggregate,
) -> Result<v2::Aggregate, ConversionError> {
    match aggregate {
        v3::Aggregate::ColumnCount { column, distinct } => {
            Ok(v2::Aggregate::ColumnCount { column, distinct })
        }
        v3::Aggregate::SingleColumn { column, function } => {
            let function_definition = functions
                .iter()
                .find(|f| f.name == function)
                .ok_or_else(|| ConversionError::UnspecifiedFunction(function.clone()))?;
            let result_type = type_to_type_name(&function_definition.result_type)?;
            Ok(v2::Aggregate::SingleColumn {
                column,
                function,
                result_type,
            })
        }
        v3::Aggregate::StarCount {} => Ok(v2::Aggregate::StarCount {}),
    }
}

fn type_to_type_name(t: &v3::Type) -> Result<String, ConversionError> {
    match t {
        v3::Type::Named { name } => Ok(name.clone()),
        v3::Type::Nullable { underlying_type } => type_to_type_name(underlying_type),
        v3::Type::Array { .. } => Err(ConversionError::TypeMismatch(format!(
            "Expected a named type, but got an array type: {t:?}"
        ))),
        v3::Type::Predicate { .. } => Err(ConversionError::TypeMismatch(format!(
            "Expected a named type, but got a predicate type: {t:?}"
        ))),
    }
}

fn v3_to_v2_fields(
    context: &QueryContext,
    v3_fields: Option<IndexMap<String, v3::Field>>,
) -> Result<Option<Option<HashMap<String, v2::Field>>>, ConversionError> {
    let v2_fields: Option<Option<HashMap<String, v2::Field>>> = v3_fields
        .map(|fields| {
            fields
                .into_iter()
                .map(|(name, field)| Ok((name, v3_to_v2_field(context, field)?)))
                .collect::<Result<_, ConversionError>>()
        })
        .transpose()?
        .map(Some);
    Ok(v2_fields)
}

fn v3_to_v2_field(context: &QueryContext, field: v3::Field) -> Result<v2::Field, ConversionError> {
    match field {
        v3::Field::Column { column, fields } => match fields {
            None => Ok(v2::Field::Column {
                column,
                column_type: UNKNOWN_TYPE.to_owned(), // TODO: is there a better option?
            }),
            Some(nested_field) => v3_to_v2_nested_field(context, column, nested_field),
        },
        v3::Field::Relationship {
            query,
            relationship,
            arguments: _,
        } => Ok(v2::Field::Relationship {
            query: Box::new(v3_to_v2_query(context, *query)?),
            relationship,
        }),
    }
}

fn v3_to_v2_nested_field(
    context: &QueryContext,
    column: String,
    nested_field: v3::NestedField,
) -> Result<v2::Field, ConversionError> {
    match nested_field {
        v3::NestedField::Object(nested_object) => {
            let mut query = v2::Query::new();
            query.fields = v3_to_v2_fields(context, Some(nested_object.fields))?;
            Ok(v2::Field::NestedObject {
                column,
                query: Box::new(query),
            })
        }
        v3::NestedField::Array(nested_array) => {
            let field =
                v3_to_v2_nested_field(context, column, nested_array.fields.deref().to_owned())?;
            Ok(v2::Field::NestedArray {
                field: Box::new(field),
                limit: None,
                offset: None,
                r#where: None,
            })
        }
    }
}

fn v3_to_v2_order_by_element(
    elem: v3::OrderByElement,
) -> Result<v2::OrderByElement, ConversionError> {
    let (target, target_path) = match elem.target {
        v3::OrderByTarget::Column { name, path } => (
            v2::OrderByTarget::Column {
                column: v2::ColumnSelector::Column(name),
            },
            path,
        ),
        v3::OrderByTarget::SingleColumnAggregate {
            column,
            function,
            path,
        } => (
            v2::OrderByTarget::SingleColumnAggregate {
                column,
                function,
                result_type: UNKNOWN_TYPE.to_owned(), // TODO: is there a better option?
            },
            path,
        ),
        v3::OrderByTarget::StarCountAggregate { path } => {
            (v2::OrderByTarget::StarCountAggregate {}, path)
        }
    };
    Ok(v2::OrderByElement {
        order_direction: match elem.order_direction {
            v3::OrderDirection::Asc => v2::OrderDirection::Asc,
            v3::OrderDirection::Desc => v2::OrderDirection::Desc,
        },
        target,
        target_path: v3_to_v2_target_path(target_path)?,
    })
}

// TODO: We should capture the predicate expression for each path element, and modify the agent to
// apply those predicates. This will involve modifying the dc_api_types to accept this data (even
// though the v2 API does not include this information - we will make sure serialization remains
// v2-compatible). This will be done in an upcoming PR.
fn v3_to_v2_target_path(path: Vec<v3::PathElement>) -> Result<Vec<String>, ConversionError> {
    fn is_expression_non_empty(expression: &v3::Expression) -> bool {
        match expression {
            v3::Expression::And { expressions } => !expressions.is_empty(),
            v3::Expression::Or { expressions } => !expressions.is_empty(),
            _ => true,
        }
    }
    if path
        .iter()
        .any(|path_element| match &path_element.predicate {
            Some(pred) => is_expression_non_empty(pred),
            None => false,
        })
    {
        Err(ConversionError::NotImplemented(
            "The MongoDB connector does not currently support predicates on references through relations",
        ))
    } else {
        Ok(path.into_iter().map(|elem| elem.relationship).collect())
    }
}

/// Like v2, a v3 QueryRequest has a map of Relationships. Unlike v2, v3 does not indicate the
/// source collection for each relationship. Instead we are supposed to keep track of the "current"
/// collection so that when we hit a Field that refers to a Relationship we infer that the source
/// is the "current" collection. This means that to produce a v2 Relationship mapping we need to
/// traverse the query here.
fn v3_to_v2_relationships(
    query_request: &v3::QueryRequest,
) -> Result<Vec<v2::TableRelationships>, ConversionError> {
    // This only captures relationships that are referenced by a Field or an OrderBy in the query.
    // We might record a relationship more than once, but we are recording to maps so that doesn't
    // matter. We might capture the same relationship multiple times with different source
    // collections, but that is by design.
    let relationships_by_source_and_name: Vec<(Vec<String>, (String, v2::Relationship))> =
        query_traversal(query_request)
            .filter_map_ok(|TraversalStep { collection, node }| match node {
                Node::Field {
                    field:
                        v3::Field::Relationship {
                            relationship,
                            arguments,
                            ..
                        },
                    ..
                } => Some((collection, relationship, arguments)),
                Node::ExistsInCollection(v3::ExistsInCollection::Related {
                    relationship,
                    arguments,
                }) => Some((collection, relationship, arguments)),
                Node::PathElement(v3::PathElement {
                    relationship,
                    arguments,
                    ..
                }) => Some((collection, relationship, arguments)),
                _ => None,
            })
            .map_ok(|(collection_name, relationship_name, _arguments)| {
                let v3_relationship = lookup_relationship(
                    &query_request.collection_relationships,
                    relationship_name,
                )?;

                // TODO: Add an `arguments` field to v2::Relationship and populate it here. (MVC-3)
                // I think it's possible that the same relationship might appear multiple time with
                // different arguments, so we may want to make some change to relationship names to
                // avoid overwriting in such a case. -Jesse
                let v2_relationship = v2::Relationship {
                    column_mapping: v2::ColumnMapping(
                        v3_relationship
                            .column_mapping
                            .iter()
                            .map(|(source_col, target_col)| {
                                (
                                    ColumnSelector::Column(source_col.clone()),
                                    ColumnSelector::Column(target_col.clone()),
                                )
                            })
                            .collect(),
                    ),
                    relationship_type: match v3_relationship.relationship_type {
                        v3::RelationshipType::Object => v2::RelationshipType::Object,
                        v3::RelationshipType::Array => v2::RelationshipType::Array,
                    },
                    target: v2::Target::TTable {
                        name: vec![v3_relationship.target_collection.clone()],
                    },
                };

                Ok((
                    vec![collection_name.to_owned()], // put in vec to match v2 namespaced format
                    (relationship_name.clone(), v2_relationship),
                )) as Result<_, ConversionError>
            })
            // The previous step produced Result<Result<_>,_> values. Flatten them to Result<_,_>.
            // We can't use the flatten() Iterator method because that loses the outer Result errors.
            .map(|result| match result {
                Ok(Ok(v)) => Ok(v),
                Ok(Err(e)) => Err(e),
                Err(e) => Err(e),
            })
            .collect::<Result<_, _>>()?;

    let grouped_by_source: HashMap<Vec<String>, Vec<(String, v2::Relationship)>> =
        relationships_by_source_and_name
            .into_iter()
            .into_group_map();

    let v2_relationships = grouped_by_source
        .into_iter()
        .map(|(source_table, relationships)| v2::TableRelationships {
            source_table,
            relationships: relationships.into_iter().collect(),
        })
        .collect();

    Ok(v2_relationships)
}

fn v3_to_v2_expression(
    scalar_types: &BTreeMap<String, ScalarType>,
    expression: v3::Expression,
) -> Result<v2::Expression, ConversionError> {
    match expression {
        v3::Expression::And { expressions } => Ok(v2::Expression::And {
            expressions: expressions
                .into_iter()
                .map(|expr| v3_to_v2_expression(scalar_types, expr))
                .collect::<Result<_, _>>()?,
        }),
        v3::Expression::Or { expressions } => Ok(v2::Expression::Or {
            expressions: expressions
                .into_iter()
                .map(|expr| v3_to_v2_expression(scalar_types, expr))
                .collect::<Result<_, _>>()?,
        }),
        v3::Expression::Not { expression } => Ok(v2::Expression::Not {
            expression: Box::new(v3_to_v2_expression(scalar_types, *expression)?),
        }),
        v3::Expression::UnaryComparisonOperator { column, operator } => {
            Ok(v2::Expression::ApplyUnaryComparison {
                column: v3_to_v2_comparison_target(column)?,
                operator: match operator {
                    v3::UnaryComparisonOperator::IsNull => v2::UnaryComparisonOperator::IsNull,
                },
            })
        }
        v3::Expression::BinaryComparisonOperator {
            column,
            operator,
            value,
        } => v3_to_v2_binary_comparison(scalar_types, column, operator, value),
        v3::Expression::Exists {
            in_collection,
            predicate,
        } => Ok(v2::Expression::Exists {
            in_table: match in_collection {
                v3::ExistsInCollection::Related {
                    relationship,
                    arguments: _,
                } => v2::ExistsInTable::RelatedTable { relationship },
                v3::ExistsInCollection::Unrelated {
                    collection,
                    arguments: _,
                } => v2::ExistsInTable::UnrelatedTable {
                    table: vec![collection],
                },
            },
            r#where: Box::new(if let Some(predicate) = predicate {
                v3_to_v2_expression(scalar_types, *predicate)?
            } else {
                // empty expression
                v2::Expression::Or {
                    expressions: vec![],
                }
            }),
        }),
    }
}

// TODO: NDC-393 - What do we need to do to handle array comparisons like `in`?. v3 now combines
// scalar and array comparisons, v2 separates them
fn v3_to_v2_binary_comparison(
    scalar_types: &BTreeMap<String, ScalarType>,
    column: v3::ComparisonTarget,
    operator: String,
    value: v3::ComparisonValue,
) -> Result<v2::Expression, ConversionError> {
    // TODO: NDC-310 look up real type here
    let fake_type = "String";
    let operator_definition = lookup_operator_definition(scalar_types, fake_type, &operator)?;
    let operator = match operator_definition {
        v3::ComparisonOperatorDefinition::Equal => v2::BinaryComparisonOperator::Equal,
        _ => v2::BinaryComparisonOperator::CustomBinaryComparisonOperator(operator),
    };
    Ok(v2::Expression::ApplyBinaryComparison {
        column: v3_to_v2_comparison_target(column)?,
        operator,
        value: v3_to_v2_comparison_value(value)?,
    })
}

fn v3_to_v2_comparison_target(
    target: v3::ComparisonTarget,
) -> Result<v2::ComparisonColumn, ConversionError> {
    match target {
        v3::ComparisonTarget::Column { name, path } => {
            let path = v3_to_v2_target_path(path)?;
            Ok(v2::ComparisonColumn {
                column_type: UNKNOWN_TYPE.to_owned(),
                name: ColumnSelector::Column(name),
                path: if path.is_empty() { None } else { Some(path) },
            })
        }
        v3::ComparisonTarget::RootCollectionColumn { name } => Ok(v2::ComparisonColumn {
            column_type: UNKNOWN_TYPE.to_owned(),
            name: ColumnSelector::Column(name),
            path: Some(vec!["$".to_owned()]),
        }),
    }
}

fn v3_to_v2_comparison_value(
    value: v3::ComparisonValue,
) -> Result<v2::ComparisonValue, ConversionError> {
    match value {
        v3::ComparisonValue::Column { column } => {
            Ok(v2::ComparisonValue::AnotherColumnComparison {
                column: v3_to_v2_comparison_target(column)?,
            })
        }
        v3::ComparisonValue::Scalar { value } => Ok(v2::ComparisonValue::ScalarValueComparison {
            value,
            value_type: UNKNOWN_TYPE.to_owned(),
        }),
        v3::ComparisonValue::Variable { name } => Ok(v2::ComparisonValue::Variable { name }),
    }
}

#[inline]
fn optional_32bit_number_to_64bit<A, B>(n: Option<A>) -> Option<Option<B>>
where
    B: From<A>,
{
    n.map(|input| Some(input.into()))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use dc_api_test_helpers::{self as v2, source, table_relationships, target};
    use ndc_sdk::models::{
        ComparisonOperatorDefinition, OrderByElement, OrderByTarget, OrderDirection, ScalarType,
        Type,
    };
    use ndc_test_helpers::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::{v3_to_v2_query_request, v3_to_v2_relationships, QueryContext};

    #[test]
    fn translates_query_request_relationships() -> Result<(), anyhow::Error> {
        let v3_query_request = query_request()
            .collection("schools")
            .relationships([
                (
                    "school_classes",
                    relationship("classes", [("_id", "school_id")]),
                ),
                (
                    "class_students",
                    relationship("students", [("_id", "class_id")]),
                ),
                (
                    "class_department",
                    relationship("departments", [("department_id", "_id")]).object_type(),
                ),
                (
                    "school_directory",
                    relationship("directory", [("_id", "school_id")]).object_type(),
                ),
                (
                    "student_advisor",
                    relationship("advisors", [("advisor_id", "_id")]).object_type(),
                ),
                (
                    "existence_check",
                    relationship("some_collection", [("some_id", "_id")]),
                ),
            ])
            .query(
                query()
                    .fields([relation_field!("school_classes" => "class_name", query()
                        .fields([
                            relation_field!("class_students" => "student_name")
                        ])
                    )])
                    .order_by(vec![OrderByElement {
                        order_direction: OrderDirection::Asc,
                        target: OrderByTarget::Column {
                            name: "advisor_name".to_owned(),
                            path: vec![
                                path_element("school_classes")
                                    .predicate(equal(
                                        target!(
                                            "department_id",
                                            [
                                                path_element("school_classes"),
                                                path_element("class_department"),
                                            ],
                                        ),
                                        column_value!(
                                            "math_department_id",
                                            [path_element("school_directory")],
                                        ),
                                    ))
                                    .into(),
                                path_element("class_students").into(),
                                path_element("student_advisor").into(),
                            ],
                        },
                    }])
                    // The `And` layer checks that we properly recursive into Expressions
                    .predicate(and([exists(
                        related!("existence_check"),
                        empty_expression(),
                    )])),
            )
            .into();

        let expected_relationships = vec![
            table_relationships(
                source("classes"),
                [
                    (
                        "class_department",
                        v2::relationship(
                            target("departments"),
                            [(v2::select!("department_id"), v2::select!("_id"))],
                        )
                        .object_type(),
                    ),
                    (
                        "class_students",
                        v2::relationship(
                            target("students"),
                            [(v2::select!("_id"), v2::select!("class_id"))],
                        ),
                    ),
                ],
            ),
            table_relationships(
                source("schools"),
                [
                    (
                        "school_classes",
                        v2::relationship(
                            target("classes"),
                            [(v2::select!("_id"), v2::select!("school_id"))],
                        ),
                    ),
                    (
                        "school_directory",
                        v2::relationship(
                            target("directory"),
                            [(v2::select!("_id"), v2::select!("school_id"))],
                        )
                        .object_type(),
                    ),
                    (
                        "existence_check",
                        v2::relationship(
                            target("some_collection"),
                            [(v2::select!("some_id"), v2::select!("_id"))],
                        ),
                    ),
                ],
            ),
            table_relationships(
                source("students"),
                [(
                    "student_advisor",
                    v2::relationship(
                        target("advisors"),
                        [(v2::select!("advisor_id"), v2::select!("_id"))],
                    )
                    .object_type(),
                )],
            ),
        ];

        let mut relationships = v3_to_v2_relationships(&v3_query_request)?;

        // Sort to match order of expected result
        relationships.sort_by_key(|rels| rels.source_table.clone());

        assert_eq!(relationships, expected_relationships);
        Ok(())
    }

    #[test]
    fn translates_root_column_references() -> Result<(), anyhow::Error> {
        let query = query_request()
            .collection("authors")
            .query(query().fields([field!("last_name")]).predicate(exists(
                unrelated!("articles"),
                and([
                    equal(target!("author_id"), column_value!(root("id"))),
                    binop("_regex", target!("title"), value!("Functional.*")),
                ]),
            )))
            .relationships([(
                "author_articles",
                relationship("articles", [("id", "author_id")]),
            )])
            .into();
        let v2_request = v3_to_v2_query_request(&query_context(), query)?;

        let expected = v2::query_request()
            .target(["authors"])
            .query(
                v2::query()
                    .fields([v2::column!("last_name": "unknown")])
                    .predicate(v2::exists_unrelated(
                        ["articles"],
                        v2::and([
                            v2::equal(
                                v2::compare!("author_id": "unknown"),
                                v2::column_value!(["$"], "id": "unknown"),
                            ),
                            v2::binop(
                                "_regex",
                                v2::compare!("title": "unknown"),
                                v2::value!(json!("Functional.*"), "unknown"),
                            ),
                        ]),
                    )),
            )
            .into();

        assert_eq!(v2_request, expected);
        Ok(())
    }

    #[test]
    fn translates_nested_fields() -> Result<(), anyhow::Error> {
        let query_request = query_request()
            .collection("authors")
            .query(query().fields([
                field!("author_address" => "address", object!([field!("address_country" => "country")])),
                field!("author_articles" => "articles", array!(object!([field!("article_title" => "title")]))),
                field!("author_array_of_arrays" => "array_of_arrays", array!(array!(object!([field!("article_title" => "title")]))))
            ]))
            .into();
        let v2_request = v3_to_v2_query_request(&query_context(), query_request)?;

        let expected = v2::query_request()
            .target(["authors"])
            .query(v2::query().fields([
                v2::nested_object!("author_address" => "address", v2::query().fields([v2::column!("address_country" => "country": "unknown")])),
                v2::nested_array!("author_articles", v2::nested_object_field!("articles", v2::query().fields([v2::column!("article_title" => "title": "unknown")]))),
                v2::nested_array!("author_array_of_arrays", v2::nested_array_field!(v2::nested_object_field!("array_of_arrays", v2::query().fields([v2::column!("article_title" => "title": "unknown")]))))
            ]))
            .into();

        assert_eq!(v2_request, expected);
        Ok(())
    }

    fn query_context() -> QueryContext {
        QueryContext {
            functions: vec![],
            scalar_types: BTreeMap::from([(
                "String".to_owned(),
                ScalarType {
                    aggregate_functions: Default::default(),
                    comparison_operators: BTreeMap::from([
                        ("_eq".to_owned(), ComparisonOperatorDefinition::Equal),
                        (
                            "_regex".to_owned(),
                            ComparisonOperatorDefinition::Custom {
                                argument_type: Type::Named {
                                    name: "String".to_owned(),
                                },
                            },
                        ),
                    ]),
                },
            )]),
        }
    }
}
