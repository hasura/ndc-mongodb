mod helpers;
pub mod query_context;
pub mod query_plan_error;
mod query_plan_state;
mod query_traversal;
pub mod type_annotated_field;

use std::collections::BTreeMap;

use crate::{self as plan, ConnectorTypes, QueryPlan};
use itertools::Itertools as _;
use ndc::QueryRequest;
use ndc_models as ndc;

use self::{
    helpers::lookup_relationship,
    query_context::QueryContext,
    query_plan_error::QueryPlanError,
    query_traversal::{query_traversal, Node, TraversalStep},
};

type Result<T> = std::result::Result<T, QueryPlanError>;

fn find_object_field<'a, S>(
    object_type: &'a plan::ObjectType<S>,
    field_name: &str,
) -> Result<&'a plan::Type<S>> {
    object_type.fields.get(field_name).ok_or_else(|| {
        QueryPlanError::UnknownObjectTypeField {
            object_type: object_type.name.clone(),
            field_name: field_name.to_string(),
            path: Default::default(), // TODO: set a path for more helpful error reporting
        }
    })
}

pub fn plan_for_query_request<S: ConnectorTypes>(
    context: &QueryContext<S>,
    request: QueryRequest,
) -> Result<QueryPlan<S>> {
    todo!()
}

// pub fn plan_for_query_request<S: Clone>(
//     context: &QueryContext<S>,
//     request: QueryRequest,
// ) -> Result<QueryPlan<S>> {
//     let collection_object_type = context.find_collection_object_type(&request.collection)?;
//
//     Ok(QueryPlan {
//         // relationships: ndc_to_v2_relationships(&request)?,
//         collection: request.collection,
//         arguments: request.arguments,
//         query: Box::new(plan_for_query(
//             context,
//             &request.collection_relationships,
//             &collection_object_type,
//             request.query,
//             &collection_object_type,
//         )?),
//
//         // We are using v2 types that have been augmented with a `variables` field (even though
//         // that is not part of the v2 API). For queries translated from ndc we use `variables`
//         // instead of `foreach`.
//         foreach: None,
//         variables: request.variables,
//     })
// }
//
// fn plan_for_query<S: Clone>(
//     context: &QueryContext<S>,
//     collection_relationships: &BTreeMap<String, ndc::Relationship>,
//     root_collection_object_type: &WithNameRef<plan::ObjectType<S>>,
//     query: ndc::Query,
//     collection_object_type: &WithNameRef<plan::ObjectType<S>>,
// ) -> Result<plan::Query<S>> {
//     let aggregates: Option<HashMap<String, plan::Aggregate>> = query
//         .aggregates
//         .map(|aggregates| -> Result<_> {
//             aggregates
//                 .into_iter()
//                 .map(|(name, aggregate)| {
//                     Ok((
//                         name,
//                         plan_for_aggregate(context, collection_object_type, aggregate)?,
//                     ))
//                 })
//                 .collect()
//         })
//         .transpose()?;
//
//     let fields = ndc_to_v2_fields(
//         context,
//         collection_relationships,
//         root_collection_object_type,
//         collection_object_type,
//         query.fields,
//     )?;
//
//     let order_by: Option<v2::OrderBy> = query
//         .order_by
//         .map(|order_by| -> Result<_> {
//             let (elements, relations) = order_by
//                 .elements
//                 .into_iter()
//                 .map(|order_by_element| {
//                     ndc_to_v2_order_by_element(
//                         context,
//                         collection_relationships,
//                         root_collection_object_type,
//                         collection_object_type,
//                         order_by_element,
//                     )
//                 })
//                 .collect::<Result<Vec<(_, _)>>>()?
//                 .into_iter()
//                 .try_fold(
//                     (
//                         Vec::<v2::OrderByElement>::new(),
//                         HashMap::<String, v2::OrderByRelation>::new(),
//                     ),
//                     |(mut acc_elems, mut acc_rels), (elem, rels)| {
//                         acc_elems.push(elem);
//                         merge_order_by_relations(&mut acc_rels, rels)?;
//                         Ok((acc_elems, acc_rels))
//                     },
//                 )?;
//             Ok(v2::OrderBy {
//                 elements,
//                 relations,
//             })
//         })
//         .transpose()?;
//
//     let limit = optional_32bit_number_to_64bit(query.limit);
//     let offset = optional_32bit_number_to_64bit(query.offset);
//
//     Ok(v2::Query {
//         aggregates,
//         aggregates_limit: limit,
//         fields,
//         order_by,
//         limit,
//         offset,
//         r#where: query
//             .predicate
//             .map(|expr| {
//                 ndc_to_v2_expression(
//                     context,
//                     collection_relationships,
//                     root_collection_object_type,
//                     collection_object_type,
//                     expr,
//                 )
//             })
//             .transpose()?,
//     })
// }
//
// fn merge_order_by_relations(
//     rels1: &mut HashMap<String, v2::OrderByRelation>,
//     rels2: HashMap<String, v2::OrderByRelation>,
// ) -> Result<()> {
//     for (relationship_name, relation2) in rels2 {
//         if let Some(relation1) = rels1.get_mut(&relationship_name) {
//             if relation1.r#where != relation2.r#where {
//                 // v2 does not support navigating the same relationship more than once across multiple
//                 // order by elements and having different predicates used on the same relationship in
//                 // different order by elements. This appears to be technically supported by NDC.
//                 return Err(QueryPlanError::NotImplemented("Relationships used in order by elements cannot contain different predicates when used more than once"));
//             }
//             merge_order_by_relations(&mut relation1.subrelations, relation2.subrelations)?;
//         } else {
//             rels1.insert(relationship_name, relation2);
//         }
//     }
//     Ok(())
// }
//
// fn plan_for_aggregate(
//     context: &QueryContext,
//     collection_object_type: &WithNameRef<plan::ObjectType>,
//     aggregate: ndc::Aggregate,
// ) -> Result<v2::Aggregate> {
//     match aggregate {
//         ndc::Aggregate::ColumnCount { column, distinct } => {
//             Ok(v2::Aggregate::ColumnCount { column, distinct })
//         }
//         ndc::Aggregate::SingleColumn { column, function } => {
//             let object_type_field = find_object_field(collection_object_type, column.as_ref())?;
//             let column_scalar_type_name = get_scalar_type_name(&object_type_field.r#type)?;
//             let aggregate_function = context
//                 .find_aggregation_function_definition(&column_scalar_type_name, &function)?;
//             let result_type = type_to_type_name(&aggregate_function.result_type)?;
//             Ok(v2::Aggregate::SingleColumn {
//                 column,
//                 function,
//                 result_type,
//             })
//         }
//         ndc::Aggregate::StarCount {} => Ok(v2::Aggregate::StarCount {}),
//     }
// }
//
// fn type_to_type_name(t: &ndc::Type) -> Result<String> {
//     match t {
//         ndc::Type::Named { name } => Ok(name.clone()),
//         ndc::Type::Nullable { underlying_type } => type_to_type_name(underlying_type),
//         ndc::Type::Array { .. } => Err(QueryPlanError::TypeMismatch(format!(
//             "Expected a named type, but got an array type: {t:?}"
//         ))),
//         ndc::Type::Predicate { .. } => Err(QueryPlanError::TypeMismatch(format!(
//             "Expected a named type, but got a predicate type: {t:?}"
//         ))),
//     }
// }
//
// fn ndc_to_v2_fields(
//     context: &QueryContext,
//     collection_relationships: &BTreeMap<String, ndc::Relationship>,
//     root_collection_object_type: &WithNameRef<plan::ObjectType>,
//     object_type: &WithNameRef<plan::ObjectType>,
//     ndc_fields: Option<IndexMap<String, ndc::Field>>,
// ) -> Result<Option<HashMap<String, v2::Field>>> {
//     let v2_fields: Option<HashMap<String, v2::Field>> = ndc_fields
//         .map(|fields| {
//             fields
//                 .into_iter()
//                 .map(|(name, field)| {
//                     Ok((
//                         name,
//                         ndc_to_v2_field(
//                             context,
//                             collection_relationships,
//                             root_collection_object_type,
//                             object_type,
//                             field,
//                         )?,
//                     ))
//                 })
//                 .collect::<Result<_>>()
//         })
//         .transpose()?;
//     Ok(v2_fields)
// }
//
// fn ndc_to_v2_field(
//     context: &QueryContext,
//     collection_relationships: &BTreeMap<String, ndc::Relationship>,
//     root_collection_object_type: &WithNameRef<plan::ObjectType>,
//     object_type: &WithNameRef<plan::ObjectType>,
//     field: ndc::Field,
// ) -> Result<v2::Field> {
//     match field {
//         ndc::Field::Column { column, fields } => {
//             let object_type_field = find_object_field(object_type, column.as_ref())?;
//             ndc_to_v2_nested_field(
//                 context,
//                 collection_relationships,
//                 root_collection_object_type,
//                 column,
//                 &object_type_field.r#type,
//                 fields,
//             )
//         }
//         ndc::Field::Relationship {
//             query,
//             relationship,
//             arguments: _,
//         } => {
//             let ndc_relationship = lookup_relationship(collection_relationships, &relationship)?;
//             let collection_object_type =
//                 context.find_collection_object_type(&ndc_relationship.target_collection)?;
//             Ok(v2::Field::Relationship {
//                 query: Box::new(plan_for_query(
//                     context,
//                     collection_relationships,
//                     root_collection_object_type,
//                     *query,
//                     &collection_object_type,
//                 )?),
//                 relationship,
//             })
//         }
//     }
// }
//
// fn ndc_to_v2_nested_field(
//     context: &QueryContext,
//     collection_relationships: &BTreeMap<String, ndc::Relationship>,
//     root_collection_object_type: &WithNameRef<plan::ObjectType>,
//     column: String,
//     schema_type: &schema::Type,
//     nested_field: Option<ndc::NestedField>,
// ) -> Result<v2::Field> {
//     match schema_type {
//         schema::Type::ExtendedJSON => {
//             Ok(v2::Field::Column {
//                 column,
//                 column_type: mongodb_support::EXTENDED_JSON_TYPE_NAME.to_string(),
//             })
//         }
//         schema::Type::Scalar(bson_scalar_type) => {
//             Ok(v2::Field::Column {
//                 column,
//                 column_type: bson_scalar_type.graphql_name(),
//             })
//         },
//         schema::Type::Nullable(underlying_type) => ndc_to_v2_nested_field(context, collection_relationships, root_collection_object_type, column, underlying_type, nested_field),
//         schema::Type::ArrayOf(element_type) => {
//             let inner_nested_field = match nested_field {
//                 None => Ok(None),
//                 Some(ndc::NestedField::Object(_nested_object)) => Err(QueryPlanError::TypeMismatch("Expected an array nested field selection, but got an object nested field selection instead".into())),
//                 Some(ndc::NestedField::Array(nested_array)) => Ok(Some(*nested_array.fields)),
//             }?;
//             let nested_v2_field = ndc_to_v2_nested_field(context, collection_relationships, root_collection_object_type, column, element_type, inner_nested_field)?;
//             Ok(v2::Field::NestedArray {
//                 field: Box::new(nested_v2_field),
//                 limit: None,
//                 offset: None,
//                 r#where: None,
//             })
//         },
//         schema::Type::Object(object_type_name) => {
//             match nested_field {
//                 None => {
//                     Ok(v2::Field::Column {
//                         column,
//                         column_type: object_type_name.clone(),
//                     })
//                 },
//                 Some(ndc::NestedField::Object(nested_object)) => {
//                     let object_type = context.find_object_type(object_type_name.as_ref())?;
//                     let mut query = v2::Query::new();
//                     query.fields = ndc_to_v2_fields(context, collection_relationships, root_collection_object_type, &object_type, Some(nested_object.fields))?;
//                     Ok(v2::Field::NestedObject {
//                         column,
//                         query: Box::new(query),
//                     })
//                 },
//                 Some(ndc::NestedField::Array(_nested_array)) =>
//                     Err(QueryPlanError::TypeMismatch("Expected an array nested field selection, but got an object nested field selection instead".into())),
//             }
//         },
//     }
// }
//
// fn ndc_to_v2_order_by_element(
//     context: &QueryContext,
//     collection_relationships: &BTreeMap<String, ndc::Relationship>,
//     root_collection_object_type: &WithNameRef<plan::ObjectType>,
//     object_type: &WithNameRef<plan::ObjectType>,
//     elem: ndc::OrderByElement,
// ) -> Result<(v2::OrderByElement, HashMap<String, v2::OrderByRelation>)> {
//     let (target, target_path) = match elem.target {
//         ndc::OrderByTarget::Column { name, path } => (
//             v2::OrderByTarget::Column {
//                 column: v2::ColumnSelector::Column(name),
//             },
//             path,
//         ),
//         ndc::OrderByTarget::SingleColumnAggregate {
//             column,
//             function,
//             path,
//         } => {
//             let end_of_relationship_path_object_type = path
//                 .last()
//                 .map(|last_path_element| {
//                     let relationship = lookup_relationship(
//                         collection_relationships,
//                         &last_path_element.relationship,
//                     )?;
//                     context.find_collection_object_type(&relationship.target_collection)
//                 })
//                 .transpose()?;
//             let target_object_type = end_of_relationship_path_object_type
//                 .as_ref()
//                 .unwrap_or(object_type);
//             let object_field = find_object_field(target_object_type, &column)?;
//             let scalar_type_name = get_scalar_type_name(&object_field.r#type)?;
//             let aggregate_function =
//                 context.find_aggregation_function_definition(&scalar_type_name, &function)?;
//             let result_type = type_to_type_name(&aggregate_function.result_type)?;
//             let target = v2::OrderByTarget::SingleColumnAggregate {
//                 column,
//                 function,
//                 result_type,
//             };
//             (target, path)
//         }
//         ndc::OrderByTarget::StarCountAggregate { path } => {
//             (v2::OrderByTarget::StarCountAggregate {}, path)
//         }
//     };
//     let (target_path, relations) = ndc_to_v2_target_path(
//         context,
//         collection_relationships,
//         root_collection_object_type,
//         target_path,
//     )?;
//     let order_by_element = v2::OrderByElement {
//         order_direction: match elem.order_direction {
//             ndc::OrderDirection::Asc => v2::OrderDirection::Asc,
//             ndc::OrderDirection::Desc => v2::OrderDirection::Desc,
//         },
//         target,
//         target_path,
//     };
//     Ok((order_by_element, relations))
// }
//
// fn ndc_to_v2_target_path(
//     context: &QueryContext,
//     collection_relationships: &BTreeMap<String, ndc::Relationship>,
//     root_collection_object_type: &WithNameRef<plan::ObjectType>,
//     path: Vec<ndc::PathElement>,
// ) -> Result<(Vec<String>, HashMap<String, v2::OrderByRelation>)> {
//     let mut v2_path = vec![];
//     let v2_relations = ndc_to_v2_target_path_step::<Vec<_>>(
//         context,
//         collection_relationships,
//         root_collection_object_type,
//         path.into_iter(),
//         &mut v2_path,
//     )?;
//     Ok((v2_path, v2_relations))
// }
//
// fn ndc_to_v2_target_path_step<T: IntoIterator<Item = ndc::PathElement>>(
//     context: &QueryContext,
//     collection_relationships: &BTreeMap<String, ndc::Relationship>,
//     root_collection_object_type: &WithNameRef<plan::ObjectType>,
//     mut path_iter: T::IntoIter,
//     v2_path: &mut Vec<String>,
// ) -> Result<HashMap<String, v2::OrderByRelation>> {
//     let mut v2_relations = HashMap::new();
//
//     if let Some(path_element) = path_iter.next() {
//         v2_path.push(path_element.relationship.clone());
//
//         let where_expr = path_element
//             .predicate
//             .map(|expression| {
//                 let ndc_relationship =
//                     lookup_relationship(collection_relationships, &path_element.relationship)?;
//                 let target_object_type =
//                     context.find_collection_object_type(&ndc_relationship.target_collection)?;
//                 let v2_expression = ndc_to_v2_expression(
//                     context,
//                     collection_relationships,
//                     root_collection_object_type,
//                     &target_object_type,
//                     *expression,
//                 )?;
//                 Ok(Box::new(v2_expression))
//             })
//             .transpose()?;
//
//         let subrelations = ndc_to_v2_target_path_step::<T>(
//             context,
//             collection_relationships,
//             root_collection_object_type,
//             path_iter,
//             v2_path,
//         )?;
//
//         v2_relations.insert(
//             path_element.relationship,
//             v2::OrderByRelation {
//                 r#where: where_expr,
//                 subrelations,
//             },
//         );
//     }
//
//     Ok(v2_relations)
// }
//
// /// Like v2, a ndc QueryRequest has a map of Relationships. Unlike v2, ndc does not indicate the
// /// source collection for each relationship. Instead we are supposed to keep track of the "current"
// /// collection so that when we hit a Field that refers to a Relationship we infer that the source
// /// is the "current" collection. This means that to produce a v2 Relationship mapping we need to
// /// traverse the query here.
// fn ndc_to_v2_relationships(
//     query_request: &ndc::QueryRequest,
// ) -> Result<Vec<v2::TableRelationships>> {
//     // This only captures relationships that are referenced by a Field or an OrderBy in the query.
//     // We might record a relationship more than once, but we are recording to maps so that doesn't
//     // matter. We might capture the same relationship multiple times with different source
//     // collections, but that is by design.
//     let relationships_by_source_and_name: Vec<(Vec<String>, (String, v2::Relationship))> =
//         query_traversal(query_request)
//             .filter_map_ok(|TraversalStep { collection, node }| match node {
//                 Node::Field {
//                     field:
//                         ndc::Field::Relationship {
//                             relationship,
//                             arguments,
//                             ..
//                         },
//                     ..
//                 } => Some((collection, relationship, arguments)),
//                 Node::ExistsInCollection(ndc::ExistsInCollection::Related {
//                     relationship,
//                     arguments,
//                 }) => Some((collection, relationship, arguments)),
//                 Node::PathElement(ndc::PathElement {
//                     relationship,
//                     arguments,
//                     ..
//                 }) => Some((collection, relationship, arguments)),
//                 _ => None,
//             })
//             .map_ok(|(collection_name, relationship_name, arguments)| {
//                 let ndc_relationship = lookup_relationship(
//                     &query_request.collection_relationships,
//                     relationship_name,
//                 )?;
//
//                 // TODO: Functions (native queries) may be referenced multiple times in a query
//                 // request with different arguments. To accommodate that we will need to record
//                 // separate v2 relations for each reference with different names. In the current
//                 // implementation one set of arguments will override arguments to all occurrences of
//                 // a given function. MDB-106
//                 let v2_relationship = v2::Relationship {
//                     column_mapping: v2::ColumnMapping(
//                         ndc_relationship
//                             .column_mapping
//                             .iter()
//                             .map(|(source_col, target_col)| {
//                                 (
//                                     ColumnSelector::Column(source_col.clone()),
//                                     ColumnSelector::Column(target_col.clone()),
//                                 )
//                             })
//                             .collect(),
//                     ),
//                     relationship_type: match ndc_relationship.relationship_type {
//                         ndc::RelationshipType::Object => v2::RelationshipType::Object,
//                         ndc::RelationshipType::Array => v2::RelationshipType::Array,
//                     },
//                     target: v2::Target::TTable {
//                         name: vec![ndc_relationship.target_collection.clone()],
//                         arguments: ndc_to_v2_relationship_arguments(arguments.clone()),
//                     },
//                 };
//
//                 Ok((
//                     vec![collection_name.to_owned()], // put in vec to match v2 namespaced format
//                     (relationship_name.clone(), v2_relationship),
//                 )) as Result<_>
//             })
//             // The previous step produced Result<Result<_>,_> values. Flatten them to Result<_,_>.
//             // We can't use the flatten() Iterator method because that loses the outer Result errors.
//             .map(|result| match result {
//                 Ok(Ok(v)) => Ok(v),
//                 Ok(Err(e)) => Err(e),
//                 Err(e) => Err(e),
//             })
//             .collect::<Result<_, _>>()?;
//
//     let grouped_by_source: HashMap<Vec<String>, Vec<(String, v2::Relationship)>> =
//         relationships_by_source_and_name
//             .into_iter()
//             .into_group_map();
//
//     let v2_relationships = grouped_by_source
//         .into_iter()
//         .map(|(source_table, relationships)| v2::TableRelationships {
//             source_table,
//             relationships: relationships.into_iter().collect(),
//         })
//         .collect();
//
//     Ok(v2_relationships)
// }
//
// fn ndc_to_v2_expression(
//     context: &QueryContext,
//     collection_relationships: &BTreeMap<String, ndc::Relationship>,
//     root_collection_object_type: &WithNameRef<plan::ObjectType>,
//     object_type: &WithNameRef<plan::ObjectType>,
//     expression: ndc::Expression,
// ) -> Result<v2::Expression> {
//     match expression {
//         ndc::Expression::And { expressions } => Ok(v2::Expression::And {
//             expressions: expressions
//                 .into_iter()
//                 .map(|expr| {
//                     ndc_to_v2_expression(
//                         context,
//                         collection_relationships,
//                         root_collection_object_type,
//                         object_type,
//                         expr,
//                     )
//                 })
//                 .collect::<Result<_, _>>()?,
//         }),
//         ndc::Expression::Or { expressions } => Ok(v2::Expression::Or {
//             expressions: expressions
//                 .into_iter()
//                 .map(|expr| {
//                     ndc_to_v2_expression(
//                         context,
//                         collection_relationships,
//                         root_collection_object_type,
//                         object_type,
//                         expr,
//                     )
//                 })
//                 .collect::<Result<_, _>>()?,
//         }),
//         ndc::Expression::Not { expression } => Ok(v2::Expression::Not {
//             expression: Box::new(ndc_to_v2_expression(
//                 context,
//                 collection_relationships,
//                 root_collection_object_type,
//                 object_type,
//                 *expression,
//             )?),
//         }),
//         ndc::Expression::UnaryComparisonOperator { column, operator } => {
//             Ok(v2::Expression::ApplyUnaryComparison {
//                 column: ndc_to_v2_comparison_target(
//                     root_collection_object_type,
//                     object_type,
//                     column,
//                 )?,
//                 operator: match operator {
//                     ndc::UnaryComparisonOperator::IsNull => v2::UnaryComparisonOperator::IsNull,
//                 },
//             })
//         }
//         ndc::Expression::BinaryComparisonOperator {
//             column,
//             operator,
//             value,
//         } => ndc_to_v2_binary_comparison(
//             context,
//             root_collection_object_type,
//             object_type,
//             column,
//             operator,
//             value,
//         ),
//         ndc::Expression::Exists {
//             in_collection,
//             predicate,
//         } => {
//             let (in_table, collection_object_type) = match in_collection {
//                 ndc::ExistsInCollection::Related {
//                     relationship,
//                     arguments: _,
//                 } => {
//                     let ndc_relationship =
//                         lookup_relationship(collection_relationships, &relationship)?;
//                     let collection_object_type =
//                         context.find_collection_object_type(&ndc_relationship.target_collection)?;
//                     let in_table = v2::ExistsInTable::RelatedTable { relationship };
//                     Ok((in_table, collection_object_type))
//                 }
//                 ndc::ExistsInCollection::Unrelated {
//                     collection,
//                     arguments: _,
//                 } => {
//                     let collection_object_type =
//                         context.find_collection_object_type(&collection)?;
//                     let in_table = v2::ExistsInTable::UnrelatedTable {
//                         table: vec![collection],
//                     };
//                     Ok((in_table, collection_object_type))
//                 }
//             }?;
//             Ok(v2::Expression::Exists {
//                 in_table,
//                 r#where: Box::new(if let Some(predicate) = predicate {
//                     ndc_to_v2_expression(
//                         context,
//                         collection_relationships,
//                         root_collection_object_type,
//                         &collection_object_type,
//                         *predicate,
//                     )?
//                 } else {
//                     // empty expression
//                     v2::Expression::Or {
//                         expressions: vec![],
//                     }
//                 }),
//             })
//         }
//     }
// }
//
// // TODO: NDC-393 - What do we need to do to handle array comparisons like `in`?. ndc now combines
// // scalar and array comparisons, v2 separates them
// fn ndc_to_v2_binary_comparison(
//     context: &QueryContext,
//     root_collection_object_type: &WithNameRef<plan::ObjectType>,
//     object_type: &WithNameRef<plan::ObjectType>,
//     column: ndc::ComparisonTarget,
//     operator: String,
//     value: ndc::ComparisonValue,
// ) -> Result<v2::Expression> {
//     let comparison_column =
//         ndc_to_v2_comparison_target(root_collection_object_type, object_type, column)?;
//     let operator_definition =
//         context.find_comparison_operator_definition(&comparison_column.column_type, &operator)?;
//     let operator = match operator_definition {
//         ndc::ComparisonOperatorDefinition::Equal => v2::BinaryComparisonOperator::Equal,
//         _ => v2::BinaryComparisonOperator::CustomBinaryComparisonOperator(operator),
//     };
//     Ok(v2::Expression::ApplyBinaryComparison {
//         value: ndc_to_v2_comparison_value(
//             root_collection_object_type,
//             object_type,
//             comparison_column.column_type.clone(),
//             value,
//         )?,
//         column: comparison_column,
//         operator,
//     })
// }
//
// fn get_scalar_type_name(schema_type: &schema::Type) -> Result<String> {
//     match schema_type {
//         schema::Type::ExtendedJSON => Ok(mongodb_support::EXTENDED_JSON_TYPE_NAME.to_string()),
//         schema::Type::Scalar(scalar_type_name) => Ok(scalar_type_name.graphql_name()),
//         schema::Type::Object(object_name_name) => Err(QueryPlanError::TypeMismatch(format!(
//             "Expected a scalar type, got the object type {object_name_name}"
//         ))),
//         schema::Type::ArrayOf(element_type) => Err(QueryPlanError::TypeMismatch(format!(
//             "Expected a scalar type, got an array of {element_type:?}"
//         ))),
//         schema::Type::Nullable(underlying_type) => get_scalar_type_name(underlying_type),
//     }
// }
//
// fn ndc_to_v2_comparison_target(
//     root_collection_object_type: &WithNameRef<plan::ObjectType>,
//     object_type: &WithNameRef<plan::ObjectType>,
//     target: ndc::ComparisonTarget,
// ) -> Result<v2::ComparisonColumn> {
//     match target {
//         ndc::ComparisonTarget::Column { name, path } => {
//             let object_field = find_object_field(object_type, &name)?;
//             let scalar_type_name = get_scalar_type_name(&object_field.r#type)?;
//             if !path.is_empty() {
//                 // This is not supported in the v2 model. ComparisonColumn.path accepts only two values:
//                 // []/None for the current table, and ["$"] for the RootCollectionColumn (handled below)
//                 Err(QueryPlanError::NotImplemented(
//                     "The MongoDB connector does not currently support comparisons against columns from related tables",
//                 ))
//             } else {
//                 Ok(v2::ComparisonColumn {
//                     column_type: scalar_type_name,
//                     name: ColumnSelector::Column(name),
//                     path: None,
//                 })
//             }
//         }
//         ndc::ComparisonTarget::RootCollectionColumn { name } => {
//             let object_field = find_object_field(root_collection_object_type, &name)?;
//             let scalar_type_name = get_scalar_type_name(&object_field.r#type)?;
//             Ok(v2::ComparisonColumn {
//                 column_type: scalar_type_name,
//                 name: ColumnSelector::Column(name),
//                 path: Some(vec!["$".to_owned()]),
//             })
//         }
//     }
// }
//
// fn ndc_to_v2_comparison_value(
//     root_collection_object_type: &WithNameRef<plan::ObjectType>,
//     object_type: &WithNameRef<plan::ObjectType>,
//     comparison_column_scalar_type: String,
//     value: ndc::ComparisonValue,
// ) -> Result<v2::ComparisonValue> {
//     match value {
//         ndc::ComparisonValue::Column { column } => {
//             Ok(v2::ComparisonValue::AnotherColumnComparison {
//                 column: ndc_to_v2_comparison_target(
//                     root_collection_object_type,
//                     object_type,
//                     column,
//                 )?,
//             })
//         }
//         ndc::ComparisonValue::Scalar { value } => Ok(v2::ComparisonValue::ScalarValueComparison {
//             value,
//             value_type: comparison_column_scalar_type,
//         }),
//         ndc::ComparisonValue::Variable { name } => Ok(v2::ComparisonValue::Variable { name }),
//     }
// }
//
// #[inline]
// fn optional_32bit_number_to_64bit<A, B>(n: Option<A>) -> Option<B>
// where
//     B: From<A>,
// {
//     n.map(|input| input.into())
// }
//
// fn ndc_to_v2_arguments(
//     arguments: BTreeMap<String, ndc::Argument>,
// ) -> HashMap<String, v2::Argument> {
//     arguments
//         .into_iter()
//         .map(|(argument_name, argument)| match argument {
//             ndc::Argument::Variable { name } => (argument_name, v2::Argument::Variable { name }),
//             ndc::Argument::Literal { value } => (argument_name, v2::Argument::Literal { value }),
//         })
//         .collect()
// }
//
// fn ndc_to_v2_relationship_arguments(
//     arguments: BTreeMap<String, ndc::RelationshipArgument>,
// ) -> HashMap<String, v2::Argument> {
//     arguments
//         .into_iter()
//         .map(|(argument_name, argument)| match argument {
//             ndc::RelationshipArgument::Variable { name } => {
//                 (argument_name, v2::Argument::Variable { name })
//             }
//             ndc::RelationshipArgument::Literal { value } => {
//                 (argument_name, v2::Argument::Literal { value })
//             }
//             ndc::RelationshipArgument::Column { name } => {
//                 (argument_name, v2::Argument::Column { name })
//             }
//         })
//         .collect()
// }
//
// #[cfg(test)]
// mod tests {
//     use std::collections::HashMap;
//
//     use dc_api_test_helpers::{self as v2, source, table_relationships, target};
//     use ndc_sdk::models::{OrderByElement, OrderByTarget, OrderDirection};
//     use ndc_test_helpers::*;
//     use pretty_assertions::assert_eq;
//     use serde_json::json;
//
//     use crate::test_helpers::{make_flat_schema, make_nested_schema};
//
//     use super::{ndc_to_v2_relationships, plan_for_query_request};
//
//     #[test]
//     fn translates_query_request_relationships() -> Result<(), anyhow::Error> {
//         let ndc_query_request = query_request()
//             .collection("schools")
//             .relationships([
//                 (
//                     "school_classes",
//                     relationship("classes", [("_id", "school_id")]),
//                 ),
//                 (
//                     "class_students",
//                     relationship("students", [("_id", "class_id")]),
//                 ),
//                 (
//                     "class_department",
//                     relationship("departments", [("department_id", "_id")]).object_type(),
//                 ),
//                 (
//                     "school_directory",
//                     relationship("directory", [("_id", "school_id")]).object_type(),
//                 ),
//                 (
//                     "student_advisor",
//                     relationship("advisors", [("advisor_id", "_id")]).object_type(),
//                 ),
//                 (
//                     "existence_check",
//                     relationship("some_collection", [("some_id", "_id")]),
//                 ),
//             ])
//             .query(
//                 query()
//                     .fields([relation_field!("school_classes" => "class_name", query()
//                         .fields([
//                             relation_field!("class_students" => "student_name")
//                         ])
//                     )])
//                     .order_by(vec![OrderByElement {
//                         order_direction: OrderDirection::Asc,
//                         target: OrderByTarget::Column {
//                             name: "advisor_name".to_owned(),
//                             path: vec![
//                                 path_element("school_classes")
//                                     .predicate(equal(
//                                         target!(
//                                             "department_id",
//                                             [
//                                                 path_element("school_classes"),
//                                                 path_element("class_department"),
//                                             ],
//                                         ),
//                                         column_value!(
//                                             "math_department_id",
//                                             [path_element("school_directory")],
//                                         ),
//                                     ))
//                                     .into(),
//                                 path_element("class_students").into(),
//                                 path_element("student_advisor").into(),
//                             ],
//                         },
//                     }])
//                     // The `And` layer checks that we properly recursive into Expressions
//                     .predicate(and([exists(
//                         related!("existence_check"),
//                         empty_expression(),
//                     )])),
//             )
//             .into();
//
//         let expected_relationships = vec![
//             table_relationships(
//                 source("classes"),
//                 [
//                     (
//                         "class_department",
//                         v2::relationship(
//                             target("departments"),
//                             [(v2::select!("department_id"), v2::select!("_id"))],
//                         )
//                         .object_type(),
//                     ),
//                     (
//                         "class_students",
//                         v2::relationship(
//                             target("students"),
//                             [(v2::select!("_id"), v2::select!("class_id"))],
//                         ),
//                     ),
//                 ],
//             ),
//             table_relationships(
//                 source("schools"),
//                 [
//                     (
//                         "school_classes",
//                         v2::relationship(
//                             target("classes"),
//                             [(v2::select!("_id"), v2::select!("school_id"))],
//                         ),
//                     ),
//                     (
//                         "school_directory",
//                         v2::relationship(
//                             target("directory"),
//                             [(v2::select!("_id"), v2::select!("school_id"))],
//                         )
//                         .object_type(),
//                     ),
//                     (
//                         "existence_check",
//                         v2::relationship(
//                             target("some_collection"),
//                             [(v2::select!("some_id"), v2::select!("_id"))],
//                         ),
//                     ),
//                 ],
//             ),
//             table_relationships(
//                 source("students"),
//                 [(
//                     "student_advisor",
//                     v2::relationship(
//                         target("advisors"),
//                         [(v2::select!("advisor_id"), v2::select!("_id"))],
//                     )
//                     .object_type(),
//                 )],
//             ),
//         ];
//
//         let mut relationships = ndc_to_v2_relationships(&ndc_query_request)?;
//
//         // Sort to match order of expected result
//         relationships.sort_by_key(|rels| rels.source_table.clone());
//
//         assert_eq!(relationships, expected_relationships);
//         Ok(())
//     }
//
//     #[test]
//     fn translates_root_column_references() -> Result<(), anyhow::Error> {
//         let query_context = make_flat_schema();
//         let query = query_request()
//             .collection("authors")
//             .query(query().fields([field!("last_name")]).predicate(exists(
//                 unrelated!("articles"),
//                 and([
//                     equal(target!("author_id"), column_value!(root("id"))),
//                     binop("_regex", target!("title"), value!("Functional.*")),
//                 ]),
//             )))
//             .into();
//         let v2_request = plan_for_query_request(&query_context, query)?;
//
//         let expected = v2::query_request()
//             .target(["authors"])
//             .query(
//                 v2::query()
//                     .fields([v2::column!("last_name": "String")])
//                     .predicate(v2::exists_unrelated(
//                         ["articles"],
//                         v2::and([
//                             v2::equal(
//                                 v2::compare!("author_id": "Int"),
//                                 v2::column_value!(["$"], "id": "Int"),
//                             ),
//                             v2::binop(
//                                 "_regex",
//                                 v2::compare!("title": "String"),
//                                 v2::value!(json!("Functional.*"), "String"),
//                             ),
//                         ]),
//                     )),
//             )
//             .into();
//
//         assert_eq!(v2_request, expected);
//         Ok(())
//     }
//
//     #[test]
//     fn translates_aggregate_selections() -> Result<(), anyhow::Error> {
//         let query_context = make_flat_schema();
//         let query = query_request()
//             .collection("authors")
//             .query(query().aggregates([
//                 star_count_aggregate!("count_star"),
//                 column_count_aggregate!("count_id" => "last_name", distinct: true),
//                 column_aggregate!("avg_id" => "id", "avg"),
//             ]))
//             .into();
//         let v2_request = plan_for_query_request(&query_context, query)?;
//
//         let expected = v2::query_request()
//             .target(["authors"])
//             .query(v2::query().aggregates([
//                 v2::star_count_aggregate!("count_star"),
//                 v2::column_count_aggregate!("count_id" => "last_name", distinct: true),
//                 v2::column_aggregate!("avg_id" => "id", "avg": "Float"),
//             ]))
//             .into();
//
//         assert_eq!(v2_request, expected);
//         Ok(())
//     }
//
//     #[test]
//     fn translates_relationships_in_fields_predicates_and_orderings() -> Result<(), anyhow::Error> {
//         let query_context = make_flat_schema();
//         let query = query_request()
//             .collection("authors")
//             .query(
//                 query()
//                     .fields([
//                         field!("last_name"),
//                         relation_field!(
//                             "author_articles" => "articles",
//                             query().fields([field!("title"), field!("year")])
//                         ),
//                     ])
//                     .predicate(exists(
//                         related!("author_articles"),
//                         binop("_regex", target!("title"), value!("Functional.*")),
//                     ))
//                     .order_by(vec![
//                         OrderByElement {
//                             order_direction: OrderDirection::Asc,
//                             target: OrderByTarget::SingleColumnAggregate {
//                                 column: "year".into(),
//                                 function: "avg".into(),
//                                 path: vec![path_element("author_articles").into()],
//                             },
//                         },
//                         OrderByElement {
//                             order_direction: OrderDirection::Desc,
//                             target: OrderByTarget::Column {
//                                 name: "id".into(),
//                                 path: vec![],
//                             },
//                         },
//                     ]),
//             )
//             .relationships([(
//                 "author_articles",
//                 relationship("articles", [("id", "author_id")]),
//             )])
//             .into();
//         let v2_request = plan_for_query_request(&query_context, query)?;
//
//         let expected = v2::query_request()
//             .target(["authors"])
//             .query(
//                 v2::query()
//                     .fields([
//                         v2::column!("last_name": "String"),
//                         v2::relation_field!(
//                             "author_articles" => "articles",
//                             v2::query()
//                                 .fields([
//                                     v2::column!("title": "String"),
//                                     v2::column!("year": "Int")]
//                                 )
//                         ),
//                     ])
//                     .predicate(v2::exists(
//                         "author_articles",
//                         v2::binop(
//                             "_regex",
//                             v2::compare!("title": "String"),
//                             v2::value!(json!("Functional.*"), "String"),
//                         ),
//                     ))
//                     .order_by(dc_api_types::OrderBy {
//                         elements: vec![
//                             dc_api_types::OrderByElement {
//                                 order_direction: dc_api_types::OrderDirection::Asc,
//                                 target: dc_api_types::OrderByTarget::SingleColumnAggregate {
//                                     column: "year".into(),
//                                     function: "avg".into(),
//                                     result_type: "Float".into(),
//                                 },
//                                 target_path: vec!["author_articles".into()],
//                             },
//                             dc_api_types::OrderByElement {
//                                 order_direction: dc_api_types::OrderDirection::Desc,
//                                 target: dc_api_types::OrderByTarget::Column {
//                                     column: v2::select!("id"),
//                                 },
//                                 target_path: vec![],
//                             },
//                         ],
//                         relations: HashMap::from([(
//                             "author_articles".into(),
//                             dc_api_types::OrderByRelation {
//                                 r#where: None,
//                                 subrelations: HashMap::new(),
//                             },
//                         )]),
//                     }),
//             )
//             .relationships(vec![table_relationships(
//                 source("authors"),
//                 [(
//                     "author_articles",
//                     v2::relationship(
//                         target("articles"),
//                         [(v2::select!("id"), v2::select!("author_id"))],
//                     ),
//                 )],
//             )])
//             .into();
//
//         assert_eq!(v2_request, expected);
//         Ok(())
//     }
//
//     #[test]
//     fn translates_nested_fields() -> Result<(), anyhow::Error> {
//         let query_context = make_nested_schema();
//         let query_request = query_request()
//             .collection("authors")
//             .query(query().fields([
//                 field!("author_address" => "address", object!([field!("address_country" => "country")])),
//                 field!("author_articles" => "articles", array!(object!([field!("article_title" => "title")]))),
//                 field!("author_array_of_arrays" => "array_of_arrays", array!(array!(object!([field!("article_title" => "title")]))))
//             ]))
//             .into();
//         let v2_request = plan_for_query_request(&query_context, query_request)?;
//
//         let expected = v2::query_request()
//             .target(["authors"])
//             .query(v2::query().fields([
//                 v2::nested_object!("author_address" => "address", v2::query().fields([v2::column!("address_country" => "country": "String")])),
//                 v2::nested_array!("author_articles", v2::nested_object_field!("articles", v2::query().fields([v2::column!("article_title" => "title": "String")]))),
//                 v2::nested_array!("author_array_of_arrays", v2::nested_array_field!(v2::nested_object_field!("array_of_arrays", v2::query().fields([v2::column!("article_title" => "title": "String")]))))
//             ]))
//             .into();
//
//         assert_eq!(v2_request, expected);
//         Ok(())
//     }
//
//     #[test]
//     fn translates_predicate_referencing_field_of_related_collection() -> anyhow::Result<()> {
//         let query_context = make_nested_schema();
//         let request = query_request()
//             .collection("appearances")
//             .relationships([("author", relationship("authors", [("authorId", "id")]))])
//             .query(
//                 query()
//                     .fields([relation_field!("author" => "presenter", query().fields([
//                         field!("name"),
//                     ]))])
//                     .predicate(not(is_null(target!("name", [path_element("author")])))),
//             )
//             .into();
//         let v2_request = plan_for_query_request(&query_context, request)?;
//
//         let expected = v2::query_request()
//             .target(["appearances"])
//             .relationships([v2::table_relationships(
//                 vec!["appearances".into()],
//                 [(
//                     "author",
//                     v2::relationship(
//                         v2::target("author"),
//                         [(
//                             dc_api_types::ColumnSelector::Column("authorId".into()),
//                             dc_api_types::ColumnSelector::Column("id".into()),
//                         )],
//                     ),
//                 )],
//             )])
//             .query(v2::query().fields([
//                 v2::relation_field!("author" => "presenter", v2::query().fields([
//                     v2::column!("name": "String")
//                 ])
//                 .predicate(v2::exists("author", v2::not(v2::is_null(v2::compare!("name": "String")))))),
//             ])).into();
//
//         assert_eq!(v2_request, expected);
//         Ok(())
//     }
// }
