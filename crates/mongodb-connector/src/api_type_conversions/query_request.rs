use std::collections::{BTreeMap, HashMap};

use configuration::{schema, Schema, WithNameRef};
use dc_api_types::{self as v2, ColumnSelector, Target};
use indexmap::IndexMap;
use itertools::Itertools;
use ndc_sdk::models::{self as v3};

use super::{
    helpers::lookup_relationship,
    query_traversal::{query_traversal, Node, TraversalStep},
    ConversionError,
};

#[derive(Clone, Debug)]
pub struct QueryContext<'a> {
    pub functions: Vec<v3::FunctionInfo>,
    pub scalar_types: &'a BTreeMap<String, v3::ScalarType>,
    pub schema: &'a Schema,
}

impl QueryContext<'_> {
    fn find_collection(self: &Self, collection_name: &str) -> Result<&schema::Collection, ConversionError> {
        self
            .schema
            .collections
            .get(collection_name)
            .ok_or_else(|| ConversionError::UnknownCollection(collection_name.to_string()))
    }

    fn find_object_type<'a>(self: &'a Self, object_type_name: &'a str) -> Result<WithNameRef<schema::ObjectType>, ConversionError> {
        let object_type = self
            .schema
            .object_types
            .get(object_type_name)
            .ok_or_else(|| ConversionError::UnknownObjectType(object_type_name.to_string()))?;
    
        Ok(WithNameRef { name: object_type_name, value: object_type })
    }

    fn find_scalar_type(self: &Self, scalar_type_name: &str) -> Result<&v3::ScalarType, ConversionError> {
        self.scalar_types
            .get(scalar_type_name)
            .ok_or_else(|| ConversionError::UnknownScalarType(scalar_type_name.to_owned()))
    }

    fn find_comparison_operator_definition(self: &Self, scalar_type_name: &str, operator: &str) -> Result<&v3::ComparisonOperatorDefinition, ConversionError> {
        let scalar_type = self.find_scalar_type(scalar_type_name)?;
        let operator = scalar_type
            .comparison_operators
            .get(operator)
            .ok_or_else(|| ConversionError::UnknownComparisonOperator(operator.to_owned()))?;
        Ok(operator)
    }
}

fn find_object_field<'a>(object_type: &'a WithNameRef<schema::ObjectType>, field_name: &str) -> Result<&'a schema::ObjectField, ConversionError> {
    object_type
        .value
        .fields
        .get(field_name)
        .ok_or_else(|| ConversionError::UnknownObjectTypeField {
            object_type: object_type.name.to_string(),
            field_name: field_name.to_string(),
        })
}

pub fn v3_to_v2_query_request(
    context: &QueryContext,
    request: v3::QueryRequest,
) -> Result<v2::QueryRequest, ConversionError> {
    let collection = context.find_collection(&request.collection)?;
    let collection_object_type = context.find_object_type(&collection.r#type)?;
        
    Ok(v2::QueryRequest {
        relationships: v3_to_v2_relationships(&request)?,
        target: Target::TTable {
            name: vec![request.collection],
        },
        query: Box::new(v3_to_v2_query(
            context,
            &request.collection_relationships,
            &collection_object_type,
            request.query,
            &collection_object_type,
        )?),

        // We are using v2 types that have been augmented with a `variables` field (even though
        // that is not part of the v2 API). For queries translated from v3 we use `variables`
        // instead of `foreach`.
        foreach: None,
        variables: request.variables,
    })
}

fn v3_to_v2_query(
    context: &QueryContext,
    collection_relationships: &BTreeMap<String, v3::Relationship>,
    root_collection_object_type: &WithNameRef<schema::ObjectType>,
    query: v3::Query,
    collection_object_type: &WithNameRef<schema::ObjectType>,
) -> Result<v2::Query, ConversionError> {
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

    let fields = v3_to_v2_fields(
        context,
        collection_relationships,
        root_collection_object_type,
        collection_object_type,
        query.fields,
    )?;

    let order_by: Option<Option<v2::OrderBy>> = query
        .order_by
        .map(|order_by| -> Result<_, ConversionError> {
            let (elements, relations) = 
                order_by
                    .elements
                    .into_iter()
                    .map(|order_by_element| v3_to_v2_order_by_element(context, collection_relationships, root_collection_object_type, collection_object_type, order_by_element))
                    .collect::<Result<Vec::<(_,_)>, ConversionError>>()?
                    .into_iter()
                    .fold(
                        Ok((Vec::<v2::OrderByElement>::new(), HashMap::<String, v2::OrderByRelation>::new())), 
                        |acc, (elem, rels)| {
                            acc.and_then(|(mut acc_elems, mut acc_rels)| {
                                acc_elems.push(elem);
                                 merge_order_by_relations(&mut acc_rels, rels)?;
                                Ok((acc_elems, acc_rels))
                            })
                        }
                    )?;
            Ok(v2::OrderBy { elements, relations })
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
            .map(|expr| v3_to_v2_expression(&context, collection_relationships, root_collection_object_type, collection_object_type, expr))
            .transpose()?,
    })
}

fn merge_order_by_relations(rels1: &mut HashMap<String, v2::OrderByRelation>, rels2: HashMap<String, v2::OrderByRelation>) -> Result<(), ConversionError> {
    for (relationship_name, relation2) in rels2 {
        if let Some(relation1) = rels1.get_mut(&relationship_name) {
            if relation1.r#where != relation2.r#where {
                // v2 does not support navigating the same relationship more than once across multiple
                // order by elements and having different predicates used on the same relationship in 
                // different order by elements. This appears to be technically supported by NDC.
                return Err(ConversionError::NotImplemented("Relationships used in order by elements cannot contain different predicates when used more than once"))
            }
            merge_order_by_relations(&mut relation1.subrelations, relation2.subrelations)?;
        } else {
            rels1.insert(relationship_name, relation2);
        }
    }
    Ok(())
}

fn v3_to_v2_aggregate(
    functions: &[v3::FunctionInfo],
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
    collection_relationships: &BTreeMap<String, v3::Relationship>,
    root_collection_object_type: &WithNameRef<schema::ObjectType>,
    object_type: &WithNameRef<schema::ObjectType>,
    v3_fields: Option<IndexMap<String, v3::Field>>,
) -> Result<Option<Option<HashMap<String, v2::Field>>>, ConversionError> {
    let v2_fields: Option<Option<HashMap<String, v2::Field>>> = v3_fields
        .map(|fields| {
            fields
                .into_iter()
                .map(|(name, field)| {
                    Ok((
                        name,
                        v3_to_v2_field(context, collection_relationships, root_collection_object_type, object_type, field)?,
                    ))
                })
                .collect::<Result<_, ConversionError>>()
        })
        .transpose()?
        .map(Some);
    Ok(v2_fields)
}

fn v3_to_v2_field(
    context: &QueryContext,
    collection_relationships: &BTreeMap<String, v3::Relationship>,
    root_collection_object_type: &WithNameRef<schema::ObjectType>,
    object_type: &WithNameRef<schema::ObjectType>,
    field: v3::Field,
) -> Result<v2::Field, ConversionError> {
    match field {
        v3::Field::Column { column, fields } => {
            let object_type_field = find_object_field(object_type, column.as_ref())?;
            v3_to_v2_nested_field(
                context,
                collection_relationships,
                root_collection_object_type,
                column,
                &object_type_field.r#type,
                fields,
            )
        }
        v3::Field::Relationship {
            query,
            relationship,
            arguments: _,
        } => {
            let v3_relationship = lookup_relationship(collection_relationships, &relationship)?;
            let collection = context.find_collection(&v3_relationship.target_collection)?;
            let collection_object_type = context.find_object_type(&collection.r#type)?;
            Ok(v2::Field::Relationship {
                query: Box::new(v3_to_v2_query(
                    context,
                    collection_relationships,
                    root_collection_object_type,
                    *query,
                    &collection_object_type,
                )?),
                relationship,
            })
        }
    }
}

fn v3_to_v2_nested_field(
    context: &QueryContext,
    collection_relationships: &BTreeMap<String, v3::Relationship>,
    root_collection_object_type: &WithNameRef<schema::ObjectType>,
    column: String,
    schema_type: &schema::Type,
    nested_field: Option<v3::NestedField>,
) -> Result<v2::Field, ConversionError> {
    match schema_type {
        schema::Type::Any => {
            Ok(v2::Field::Column {
                column,
                column_type: mongodb_support::ANY_TYPE_NAME.to_string(),
            })
        }
        schema::Type::Scalar(bson_scalar_type) => {
            Ok(v2::Field::Column {
                column,
                column_type: bson_scalar_type.graphql_name(),
            })
        },
        schema::Type::Nullable(underlying_type) => v3_to_v2_nested_field(context, collection_relationships, root_collection_object_type, column, underlying_type, nested_field),
        schema::Type::ArrayOf(element_type) => {
            let inner_nested_field = match nested_field {
                None => Ok(None),
                Some(v3::NestedField::Object(_nested_object)) => Err(ConversionError::TypeMismatch(format!("Expected an array nested field selection, but got an object nested field selection instead"))),
                Some(v3::NestedField::Array(nested_array)) => Ok(Some(*nested_array.fields)),
            }?;
            let nested_v2_field = v3_to_v2_nested_field(context, collection_relationships, root_collection_object_type, column, element_type, inner_nested_field)?;
            Ok(v2::Field::NestedArray {
                field: Box::new(nested_v2_field),
                limit: None,
                offset: None,
                r#where: None,
            })
        },
        schema::Type::Object(object_type_name) => {
            match nested_field {
                None => {
                    Ok(v2::Field::Column {
                        column,
                        column_type: object_type_name.clone(),
                    })
                },
                Some(v3::NestedField::Object(nested_object)) => {
                    let object_type = context.find_object_type(object_type_name.as_ref())?;
                    let mut query = v2::Query::new();
                    query.fields = v3_to_v2_fields(context, collection_relationships, root_collection_object_type, &object_type, Some(nested_object.fields))?;
                    Ok(v2::Field::NestedObject {
                        column,
                        query: Box::new(query),
                    })
                },
                Some(v3::NestedField::Array(_nested_array)) => 
                    Err(ConversionError::TypeMismatch(format!("Expected an array nested field selection, but got an object nested field selection instead"))),
            }
        },
    }
}

fn v3_to_v2_order_by_element(
    context: &QueryContext,
    collection_relationships: &BTreeMap<String, v3::Relationship>,
    root_collection_object_type: &WithNameRef<schema::ObjectType>,
    object_type: &WithNameRef<schema::ObjectType>,
    elem: v3::OrderByElement,
) -> Result<(v2::OrderByElement, HashMap<String, v2::OrderByRelation>), ConversionError> {
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
        } => {
            let end_of_relationship_path_object_type = path
                .last()
                .map(|last_path_element| {
                    let relationship = lookup_relationship(collection_relationships, &last_path_element.relationship)?;
                    let target_collection = context.find_collection(&relationship.target_collection)?;
                    context.find_object_type(&target_collection.r#type)
                })
                .transpose()?;
            let target_object_type = end_of_relationship_path_object_type.as_ref().unwrap_or(object_type);
            let object_field = find_object_field(target_object_type, &column)?;
            let scalar_type_name = get_scalar_type_name(&object_field.r#type)?;
            let scalar_type = context.find_scalar_type(&scalar_type_name)?;
            let aggregate_function = scalar_type.aggregate_functions.get(&function).ok_or_else(|| ConversionError::UnknownAggregateFunction { scalar_type: scalar_type_name, aggregate_function: function.clone() })?;
            let result_type = type_to_type_name(&aggregate_function.result_type)?;
            let target = v2::OrderByTarget::SingleColumnAggregate {
                column,
                function,
                result_type,
            };
            (target, path)
        },
        v3::OrderByTarget::StarCountAggregate { path } => {
            (v2::OrderByTarget::StarCountAggregate {}, path)
        }
    };
    let (target_path, relations) = v3_to_v2_target_path(context, collection_relationships, root_collection_object_type, target_path)?;
    let order_by_element = v2::OrderByElement {
        order_direction: match elem.order_direction {
            v3::OrderDirection::Asc => v2::OrderDirection::Asc,
            v3::OrderDirection::Desc => v2::OrderDirection::Desc,
        },
        target,
        target_path,
    };
    Ok((order_by_element, relations))
}

fn v3_to_v2_target_path(
    context: &QueryContext,
    collection_relationships: &BTreeMap<String, v3::Relationship>,
    root_collection_object_type: &WithNameRef<schema::ObjectType>,
    path: Vec<v3::PathElement>
) -> Result<(Vec<String>, HashMap<String, v2::OrderByRelation>), ConversionError> {
    let mut v2_path = vec![];
    let v2_relations = v3_to_v2_target_path_step::<Vec<_>>(context, collection_relationships, root_collection_object_type, path.into_iter(), &mut v2_path)?;
    Ok((v2_path, v2_relations))
}

fn v3_to_v2_target_path_step<T : IntoIterator<Item = v3::PathElement>>(
    context: &QueryContext,
    collection_relationships: &BTreeMap<String, v3::Relationship>,
    root_collection_object_type: &WithNameRef<schema::ObjectType>,
    mut path_iter: T::IntoIter, 
    v2_path: &mut Vec<String>
) -> Result<HashMap<String, v2::OrderByRelation>, ConversionError> {
    let mut v2_relations = HashMap::new();

    if let Some(path_element) = path_iter.next() {
        v2_path.push(path_element.relationship.clone());

        let where_expr = path_element
            .predicate
            .map(|expression| {
                let v3_relationship = lookup_relationship(collection_relationships, &path_element.relationship)?;
                let target_collection = context.find_collection(&v3_relationship.target_collection)?;
                let target_object_type = context.find_object_type(&target_collection.r#type)?;
                let v2_expression = v3_to_v2_expression(context, collection_relationships, root_collection_object_type, &target_object_type, *expression)?;
                Ok(Box::new(v2_expression))
            })
            .transpose()?;

        let subrelations = v3_to_v2_target_path_step::<T>(context, collection_relationships, root_collection_object_type, path_iter, v2_path)?;
        
        v2_relations.insert(
            path_element.relationship, 
            v2::OrderByRelation {
                r#where: where_expr,
                subrelations,
            }
        );
    }

    Ok(v2_relations)
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
    context: &QueryContext,
    collection_relationships: &BTreeMap<String, v3::Relationship>,
    root_collection_object_type: &WithNameRef<schema::ObjectType>,
    object_type: &WithNameRef<schema::ObjectType>,
    expression: v3::Expression,
) -> Result<v2::Expression, ConversionError> {
    match expression {
        v3::Expression::And { expressions } => Ok(v2::Expression::And {
            expressions: expressions
                .into_iter()
                .map(|expr| v3_to_v2_expression(context, collection_relationships, root_collection_object_type, object_type, expr))
                .collect::<Result<_, _>>()?,
        }),
        v3::Expression::Or { expressions } => Ok(v2::Expression::Or {
            expressions: expressions
                .into_iter()
                .map(|expr| v3_to_v2_expression(context, collection_relationships, root_collection_object_type, object_type, expr))
                .collect::<Result<_, _>>()?,
        }),
        v3::Expression::Not { expression } => Ok(v2::Expression::Not {
            expression: Box::new(v3_to_v2_expression(context, collection_relationships, root_collection_object_type, object_type, *expression)?),
        }),
        v3::Expression::UnaryComparisonOperator { column, operator } => {
            Ok(v2::Expression::ApplyUnaryComparison {
                column: v3_to_v2_comparison_target(root_collection_object_type, object_type, column)?,
                operator: match operator {
                    v3::UnaryComparisonOperator::IsNull => v2::UnaryComparisonOperator::IsNull,
                },
            })
        }
        v3::Expression::BinaryComparisonOperator {
            column,
            operator,
            value,
        } => v3_to_v2_binary_comparison(context, root_collection_object_type, object_type, column, operator, value),
        v3::Expression::Exists { in_collection, predicate, } => {
            let (in_table, collection_object_type) = match in_collection {
                v3::ExistsInCollection::Related { relationship, arguments: _ } => {
                    let v3_relationship = lookup_relationship(collection_relationships, &relationship)?;
                    let v3_collection = context.find_collection(&v3_relationship.target_collection)?;
                    let collection_object_type = context.find_object_type(&v3_collection.r#type)?;
                    let in_table = v2::ExistsInTable::RelatedTable { relationship };
                    Ok((in_table, collection_object_type))
                },
                v3::ExistsInCollection::Unrelated { collection, arguments: _ } => {
                    let v3_collection = context.find_collection(&collection)?;
                    let collection_object_type = context.find_object_type(&v3_collection.r#type)?;
                    let in_table = v2::ExistsInTable::UnrelatedTable { table: vec![collection] };
                    Ok((in_table, collection_object_type))
                },
            }?;
            Ok(v2::Expression::Exists {
                in_table,
                r#where: Box::new(if let Some(predicate) = predicate {
                    v3_to_v2_expression(context, collection_relationships, root_collection_object_type, &collection_object_type, *predicate)?
                } else {
                    // empty expression
                    v2::Expression::Or {
                        expressions: vec![],
                    }
                }),
            })
        },
    }
}

// TODO: NDC-393 - What do we need to do to handle array comparisons like `in`?. v3 now combines
// scalar and array comparisons, v2 separates them
fn v3_to_v2_binary_comparison(
    context: &QueryContext,
    root_collection_object_type: &WithNameRef<schema::ObjectType>,
    object_type: &WithNameRef<schema::ObjectType>,
    column: v3::ComparisonTarget,
    operator: String,
    value: v3::ComparisonValue,
) -> Result<v2::Expression, ConversionError> {
    let comparison_column = v3_to_v2_comparison_target(root_collection_object_type, object_type, column)?;
    let operator_definition = context.find_comparison_operator_definition(&comparison_column.column_type, &operator)?;
    let operator = match operator_definition {
        v3::ComparisonOperatorDefinition::Equal => v2::BinaryComparisonOperator::Equal,
        _ => v2::BinaryComparisonOperator::CustomBinaryComparisonOperator(operator),
    };
    Ok(v2::Expression::ApplyBinaryComparison {
        value: v3_to_v2_comparison_value(root_collection_object_type, object_type, comparison_column.column_type.clone(), value)?,
        column: comparison_column,
        operator,
    })
}

fn get_scalar_type_name(schema_type: &schema::Type) -> Result<String, ConversionError> {
    match schema_type {
        schema::Type::Any => Ok(mongodb_support::ANY_TYPE_NAME.to_string()),
        schema::Type::Scalar(scalar_type_name) => Ok(scalar_type_name.graphql_name()),
        schema::Type::Object(object_name_name) => Err(ConversionError::TypeMismatch(format!("Expected a scalar type, got the object type {object_name_name}"))),
        schema::Type::ArrayOf(element_type) => Err(ConversionError::TypeMismatch(format!("Expected a scalar type, got an array of {element_type:?}"))),
        schema::Type::Nullable(underlying_type) => get_scalar_type_name(&underlying_type),
    }
}

fn v3_to_v2_comparison_target(
    root_collection_object_type: &WithNameRef<schema::ObjectType>,
    object_type: &WithNameRef<schema::ObjectType>,
    target: v3::ComparisonTarget,
) -> Result<v2::ComparisonColumn, ConversionError> {
    match target {
        v3::ComparisonTarget::Column { name, path } => {
            let object_field = find_object_field(object_type, &name)?;
            let scalar_type_name = get_scalar_type_name(&object_field.r#type)?;
            if !path.is_empty() {
                // This is not supported in the v2 model. ComparisonColumn.path accepts only two values: 
                // []/None for the current table, and ["*"] for the RootCollectionColumn (handled below)
                Err(ConversionError::NotImplemented(
                    "The MongoDB connector does not currently support comparisons against columns from related tables",
                ))
            } else {
                Ok(v2::ComparisonColumn {
                    column_type: scalar_type_name,
                    name: ColumnSelector::Column(name),
                    path: None,
                })
            }
        }
        v3::ComparisonTarget::RootCollectionColumn { name } => {
            let object_field = find_object_field(root_collection_object_type, &name)?;
            let scalar_type_name = get_scalar_type_name(&object_field.r#type)?;
            Ok(v2::ComparisonColumn {
                column_type: scalar_type_name,
                name: ColumnSelector::Column(name),
                path: Some(vec!["$".to_owned()]),
            })
        },
    }
}

fn v3_to_v2_comparison_value(
    root_collection_object_type: &WithNameRef<schema::ObjectType>,
    object_type: &WithNameRef<schema::ObjectType>,
    comparison_column_scalar_type: String,
    value: v3::ComparisonValue,
) -> Result<v2::ComparisonValue, ConversionError> {
    match value {
        v3::ComparisonValue::Column { column } => {
            Ok(v2::ComparisonValue::AnotherColumnComparison {
                column: v3_to_v2_comparison_target(root_collection_object_type, object_type, column)?,
            })
        }
        v3::ComparisonValue::Scalar { value } => Ok(v2::ComparisonValue::ScalarValueComparison {
            value,
            value_type: comparison_column_scalar_type,
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
    use std::collections::{BTreeMap, HashMap};

    use configuration::{schema, Schema};
    use dc_api_test_helpers::{self as v2, source, table_relationships, target};
    use mongodb_support::BsonScalarType;
    use ndc_sdk::models::{
        AggregateFunctionDefinition, ComparisonOperatorDefinition, OrderByElement, OrderByTarget, OrderDirection, ScalarType, Type
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
        let scalar_types = make_scalar_types();
        let schema = make_flat_schema();
        let query_context = QueryContext {
            functions: vec![],
            scalar_types: &scalar_types,
            schema: &schema,
        };
        let query = query_request()
            .collection("authors")
            .query(query().fields([field!("last_name")]).predicate(exists(
                unrelated!("articles"),
                and([
                    equal(target!("author_id"), column_value!(root("id"))),
                    binop("_regex", target!("title"), value!("Functional.*")),
                ]),
            )))
            .into();
        let v2_request = v3_to_v2_query_request(&query_context, query)?;

        let expected = v2::query_request()
            .target(["authors"])
            .query(
                v2::query()
                    .fields([v2::column!("last_name": "String")])
                    .predicate(v2::exists_unrelated(
                        ["articles"],
                        v2::and([
                            v2::equal(
                                v2::compare!("author_id": "Int"),
                                v2::column_value!(["$"], "id": "Int"),
                            ),
                            v2::binop(
                                "_regex",
                                v2::compare!("title": "String"),
                                v2::value!(json!("Functional.*"), "String"),
                            ),
                        ]),
                    )),
            )
            .into();

        assert_eq!(v2_request, expected);
        Ok(())
    }

    #[test]
    fn translates_relationships_in_fields_predicates_and_orderings() -> Result<(), anyhow::Error> {
        let scalar_types = make_scalar_types();
        let schema = make_flat_schema();
        let query_context = QueryContext {
            functions: vec![],
            scalar_types: &scalar_types,
            schema: &schema,
        };
        let query = query_request()
            .collection("authors")
            .query(
                query()
                    .fields([
                        field!("last_name"),
                        relation_field!(
                            "author_articles" => "articles", 
                            query().fields([field!("title"), field!("year")])
                        )
                    ])
                    .predicate(exists(
                        related!("author_articles"),
                        binop("_regex", target!("title"), value!("Functional.*")),
                    ))
                    .order_by(vec![
                        OrderByElement {
                            order_direction: OrderDirection::Asc,
                            target: OrderByTarget::SingleColumnAggregate {
                                column: "year".into(),
                                function: "avg".into(),
                                path: vec![
                                    path_element("author_articles").into()
                                ],
                            },
                        },
                        OrderByElement {
                            order_direction: OrderDirection::Desc,
                            target: OrderByTarget::Column {
                                name: "id".into(),
                                path: vec![],
                            },
                        }
                    ])
            )
            .relationships([(
                "author_articles",
                relationship("articles", [("id", "author_id")]),
            )])
            .into();
        let v2_request = v3_to_v2_query_request(&query_context, query)?;

        let expected = v2::query_request()
            .target(["authors"])
            .query(
                v2::query()
                    .fields([
                        v2::column!("last_name": "String"),
                        v2::relation_field!(
                            "author_articles" => "articles", 
                            v2::query()
                                .fields([
                                    v2::column!("title": "String"), 
                                    v2::column!("year": "Int")]
                                )
                        )
                    ])
                    .predicate(v2::exists(
                        "author_articles",
                        v2::binop(
                            "_regex",
                            v2::compare!("title": "String"),
                            v2::value!(json!("Functional.*"), "String"),
                        ),
                    ))
                    .order_by(
                        dc_api_types::OrderBy { 
                            elements: vec![
                                dc_api_types::OrderByElement { 
                                    order_direction: dc_api_types::OrderDirection::Asc, 
                                    target: dc_api_types::OrderByTarget::SingleColumnAggregate { 
                                        column: "year".into(), 
                                        function: "avg".into(), 
                                        result_type: "Float".into() 
                                    }, 
                                    target_path: vec!["author_articles".into()],
                                },
                                dc_api_types::OrderByElement { 
                                    order_direction: dc_api_types::OrderDirection::Desc, 
                                    target: dc_api_types::OrderByTarget::Column { column: v2::select!("id") }, 
                                    target_path: vec![],
                                }
                            ], 
                            relations: HashMap::from([(
                                "author_articles".into(),
                                dc_api_types::OrderByRelation {
                                    r#where: None,
                                    subrelations: HashMap::new(),
                                }
                            )])
                        }
                    ),
            )
            .relationships(vec![
                table_relationships(
                    source("authors"),
                    [
                        (
                            "author_articles",
                            v2::relationship(
                                target("articles"),
                                [(v2::select!("id"), v2::select!("author_id"))],
                            )
                        ),
                    ],
                )
            ])
            .into();

        assert_eq!(v2_request, expected);
        Ok(())
    }

    #[test]
    fn translates_nested_fields() -> Result<(), anyhow::Error> {
        let scalar_types = make_scalar_types();
        let schema = make_nested_schema();
        let query_context = QueryContext {
            functions: vec![],
            scalar_types: &scalar_types,
            schema: &schema,
        };
        let query_request = query_request()
            .collection("authors")
            .query(query().fields([
                field!("author_address" => "address", object!([field!("address_country" => "country")])),
                field!("author_articles" => "articles", array!(object!([field!("article_title" => "title")]))),
                field!("author_array_of_arrays" => "array_of_arrays", array!(array!(object!([field!("article_title" => "title")]))))
            ]))
            .into();
        let v2_request = v3_to_v2_query_request(&query_context, query_request)?;

        let expected = v2::query_request()
            .target(["authors"])
            .query(v2::query().fields([
                v2::nested_object!("author_address" => "address", v2::query().fields([v2::column!("address_country" => "country": "String")])),
                v2::nested_array!("author_articles", v2::nested_object_field!("articles", v2::query().fields([v2::column!("article_title" => "title": "String")]))),
                v2::nested_array!("author_array_of_arrays", v2::nested_array_field!(v2::nested_object_field!("array_of_arrays", v2::query().fields([v2::column!("article_title" => "title": "String")]))))
            ]))
            .into();

        assert_eq!(v2_request, expected);
        Ok(())
    }

    fn make_scalar_types() -> BTreeMap<String, ScalarType> {
        BTreeMap::from([
            (
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
            ),
            (
                "Int".to_owned(),
                ScalarType {
                    aggregate_functions: BTreeMap::from([
                        (
                            "avg".into(),
                            AggregateFunctionDefinition {
                                result_type: Type::Named {
                                    name: "Float".into() // Different result type to the input scalar type
                                }
                            }
                        )
                    ]),
                    comparison_operators: BTreeMap::from([
                        ("_eq".to_owned(), ComparisonOperatorDefinition::Equal),
                    ]),
                },
            )
        ])
    }

    fn make_flat_schema() -> Schema {
        Schema {
            collections: BTreeMap::from([
                (
                    "authors".into(),
                    schema::Collection {
                        description: None,
                        r#type: "Author".into()
                    }
                ),
                (
                    "articles".into(),
                    schema::Collection {
                        description: None,
                        r#type: "Article".into()
                    }
                ),
            ]),
            object_types: BTreeMap::from([
                (
                    "Author".into(),
                    schema::ObjectType {
                        description: None,
                        fields: BTreeMap::from([
                            (
                                "id".into(),
                                schema::ObjectField {
                                    description: None,
                                    r#type: schema::Type::Scalar(BsonScalarType::Int)
                                }
                            ),
                            (
                                "last_name".into(),
                                schema::ObjectField {
                                    description: None,
                                    r#type: schema::Type::Scalar(BsonScalarType::String)
                                }
                            ),
                        ]),
                    }
                ),
                (
                    "Article".into(),
                    schema::ObjectType {
                        description: None,
                        fields: BTreeMap::from([
                            (
                                "author_id".into(),
                                schema::ObjectField {
                                    description: None,
                                    r#type: schema::Type::Scalar(BsonScalarType::Int)
                                }
                            ),
                            (
                                "title".into(),
                                schema::ObjectField {
                                    description: None,
                                    r#type: schema::Type::Scalar(BsonScalarType::String)
                                }
                            ),
                            (
                                "year".into(),
                                schema::ObjectField {
                                    description: None,
                                    r#type: schema::Type::Nullable(Box::new(schema::Type::Scalar(BsonScalarType::Int)))
                                }
                            ),
                        ]),
                    }
                ),
            ]),
        }
    }

    fn make_nested_schema() -> Schema {
        Schema {
            collections: BTreeMap::from([
                (
                    "authors".into(),
                    schema::Collection {
                        description: None,
                        r#type: "Author".into()
                    }
                )
            ]),
            object_types: BTreeMap::from([
                (
                    "Author".into(),
                    schema::ObjectType {
                        description: None,
                        fields: BTreeMap::from([
                            (
                                "address".into(),
                                schema::ObjectField {
                                    description: None,
                                    r#type: schema::Type::Object("Address".into())
                                }
                            ),
                            (
                                "articles".into(),
                                schema::ObjectField {
                                    description: None,
                                    r#type: schema::Type::ArrayOf(Box::new(schema::Type::Object("Article".into())))
                                }
                            ),
                            (
                                "array_of_arrays".into(),
                                schema::ObjectField {
                                    description: None,
                                    r#type: schema::Type::ArrayOf(Box::new(schema::Type::ArrayOf(Box::new(schema::Type::Object("Article".into())))))
                                }
                            ),
                        ]),
                    }
                ),
                (
                    "Address".into(),
                    schema::ObjectType {
                        description: None,
                        fields: BTreeMap::from([
                            (
                                "country".into(),
                                schema::ObjectField {
                                    description: None,
                                    r#type: schema::Type::Scalar(BsonScalarType::String)
                                }
                            ),
                        ]),
                    }
                ),
                (
                    "Article".into(),
                    schema::ObjectType {
                        description: None,
                        fields: BTreeMap::from([
                            (
                                "title".into(),
                                schema::ObjectField {
                                    description: None,
                                    r#type: schema::Type::Scalar(BsonScalarType::String)
                                }
                            ),
                        ]),
                    }
                ),
            ]),
        }
    }
}
