use std::collections::BTreeMap;

use itertools::Either;
use ndc_models::{
    ComparisonTarget, ComparisonValue, ExistsInCollection, Expression, Field, OrderByElement,
    OrderByTarget, PathElement, Query, QueryRequest, Relationship,
};

use super::{helpers::lookup_relationship, query_plan_error::QueryPlanError};

type Result<T> = std::result::Result<T, QueryPlanError>;

#[derive(Copy, Clone, Debug)]
pub enum Node<'a> {
    ComparisonTarget(&'a ComparisonTarget),
    ComparisonValue(&'a ComparisonValue),
    ExistsInCollection(&'a ExistsInCollection),
    Expression(&'a Expression),
    Field { name: &'a str, field: &'a Field },
    OrderByElement(&'a OrderByElement),
    PathElement(&'a PathElement),
}

#[derive(Clone, Debug)]
pub struct TraversalStep<'a, 'b> {
    pub collection: &'a str,
    pub node: Node<'b>,
}

#[derive(Copy, Clone, Debug)]
struct Context<'a> {
    collection: &'a str,
    relationships: &'a BTreeMap<String, Relationship>,
}

impl<'a> Context<'a> {
    fn set_collection<'b>(self, new_collection: &'b str) -> Context<'b>
    where
        'a: 'b,
    {
        Context {
            collection: new_collection,
            relationships: self.relationships,
        }
    }
}

/// Walk a v3 query producing an iterator that visits selected AST nodes. This is used to build up
/// maps of relationships, so the goal is to hit every instance of these node types:
///
/// - Field (referenced by Query, MutationOperation)
/// - ExistsInCollection (referenced by Expression which is referenced by Query, PathElement)
/// - PathElement (referenced by OrderByTarget<-OrderByElement<-OrderBy<-Query, ComparisonTarget<-Expression, ComparisonValue<-Expression)
///
/// This implementation does not guarantee an order.
pub fn query_traversal(
    query_request: &QueryRequest,
) -> impl Iterator<Item = Result<TraversalStep>> {
    let QueryRequest {
        collection,
        collection_relationships,
        query,
        ..
    } = query_request;
    query_traversal_helper(
        Context {
            relationships: collection_relationships,
            collection,
        },
        query,
    )
}

fn query_traversal_helper<'a>(
    context: Context<'a>,
    query: &'a Query,
) -> impl Iterator<Item = Result<TraversalStep<'a, 'a>>> {
    query_fields_traversal(context, query)
        .chain(traverse_collection(
            expression_traversal,
            context,
            &query.predicate,
        ))
        .chain(order_by_traversal(context, query))
}

/// Recursively walk each Field in a Query
fn query_fields_traversal<'a>(
    context: Context<'a>,
    query: &'a Query,
) -> impl Iterator<Item = Result<TraversalStep<'a, 'a>>> {
    query
        .fields
        .iter()
        .flatten()
        .flat_map(move |(name, field)| {
            let field_step = std::iter::once(Ok(TraversalStep {
                collection: context.collection,
                node: Node::Field { name, field },
            }));
            field_step.chain(field_relationship_traversal(context, field))
        })
}

/// If the given field is a Relationship, traverses the nested query
fn field_relationship_traversal<'a>(
    context: Context<'a>,
    field: &'a Field,
) -> Box<dyn Iterator<Item = Result<TraversalStep<'a, 'a>>> + 'a> {
    match field {
        Field::Column { .. } => Box::new(std::iter::empty()),
        Field::Relationship {
            query,
            relationship,
            ..
        } => match lookup_relationship(context.relationships, relationship) {
            Ok(rel) => Box::new(query_traversal_helper(
                context.set_collection(&rel.target_collection),
                query,
            )),
            Err(e) => Box::new(std::iter::once(Err(e))),
        },
    }
}

/// Traverse OrderByElements, including their PathElements.
fn order_by_traversal<'a>(
    context: Context<'a>,
    query: &'a Query,
) -> impl Iterator<Item = Result<TraversalStep<'a, 'a>>> {
    let order_by_elements = query.order_by.as_ref().map(|o| &o.elements);

    order_by_elements
        .into_iter()
        .flatten()
        .flat_map(move |order_by_element| {
            let order_by_element_step = std::iter::once(Ok(TraversalStep {
                collection: context.collection,
                node: Node::OrderByElement(order_by_element),
            }));
            let path = match &order_by_element.target {
                OrderByTarget::Column { path, .. } => path,
                OrderByTarget::SingleColumnAggregate { path, .. } => path,
                OrderByTarget::StarCountAggregate { path } => path,
            };
            order_by_element_step.chain(path_elements_traversal(context, path))
        })
}

fn path_elements_traversal<'a>(
    context: Context<'a>,
    path: &'a [PathElement],
) -> impl Iterator<Item = Result<TraversalStep<'a, 'a>>> {
    path.iter()
        .scan(
            context.collection,
            move |element_collection, path_element| -> Option<Box<dyn Iterator<Item = _>>> {
                match lookup_relationship(context.relationships, &path_element.relationship) {
                    Ok(rel) => {
                        let path_element_step = std::iter::once(Ok(TraversalStep {
                            collection: element_collection,
                            node: Node::PathElement(path_element),
                        }));

                        let expression_steps = match &path_element.predicate {
                            Some(expression) => Either::Right(expression_traversal(
                                context.set_collection(element_collection),
                                expression,
                            )),
                            None => Either::Left(std::iter::empty()),
                        };

                        *element_collection = &rel.target_collection;

                        Some(Box::new(path_element_step.chain(expression_steps)))
                    }
                    Err(e) => Some(Box::new(std::iter::once(Err(e)))),
                }
            },
        )
        .flatten()
}

fn expression_traversal<'a>(
    context: Context<'a>,
    expression: &'a Expression,
) -> impl Iterator<Item = Result<TraversalStep<'a, 'a>>> {
    let expression_step = std::iter::once(Ok(TraversalStep {
        collection: context.collection,
        node: Node::Expression(expression),
    }));

    let nested_expression_steps: Box<dyn Iterator<Item = _>> = match expression {
        Expression::And { expressions } => Box::new(traverse_collection(
            expression_traversal,
            context,
            expressions,
        )),
        Expression::Or { expressions } => Box::new(traverse_collection(
            expression_traversal,
            context,
            expressions,
        )),
        Expression::Not { expression } => Box::new(expression_traversal(context, expression)),
        Expression::UnaryComparisonOperator { column, .. } => {
            Box::new(comparison_target_traversal(context, column))
        }
        Expression::BinaryComparisonOperator { column, value, .. } => Box::new(
            comparison_target_traversal(context, column)
                .chain(comparison_value_traversal(context, value)),
        ),
        Expression::Exists {
            in_collection,
            predicate,
        } => {
            let in_collection_step = std::iter::once(Ok(TraversalStep {
                collection: context.collection,
                node: Node::ExistsInCollection(in_collection),
            }));
            match predicate {
                Some(predicate) => {
                    Box::new(in_collection_step.chain(expression_traversal(context, predicate)))
                }
                None => Box::new(std::iter::empty()),
            }
        }
    };

    expression_step.chain(nested_expression_steps)
}

fn comparison_target_traversal<'a>(
    context: Context<'a>,
    comparison_target: &'a ComparisonTarget,
) -> impl Iterator<Item = Result<TraversalStep<'a, 'a>>> {
    let this_step = std::iter::once(Ok(TraversalStep {
        collection: context.collection,
        node: Node::ComparisonTarget(comparison_target),
    }));

    let nested_steps: Box<dyn Iterator<Item = _>> = match comparison_target {
        ComparisonTarget::Column { path, .. } => Box::new(path_elements_traversal(context, path)),
        ComparisonTarget::RootCollectionColumn { .. } => Box::new(std::iter::empty()),
    };

    this_step.chain(nested_steps)
}

fn comparison_value_traversal<'a>(
    context: Context<'a>,
    comparison_value: &'a ComparisonValue,
) -> impl Iterator<Item = Result<TraversalStep<'a, 'a>>> {
    let this_step = std::iter::once(Ok(TraversalStep {
        collection: context.collection,
        node: Node::ComparisonValue(comparison_value),
    }));

    let nested_steps: Box<dyn Iterator<Item = _>> = match comparison_value {
        ComparisonValue::Column { column } => {
            Box::new(comparison_target_traversal(context, column))
        }
        ComparisonValue::Scalar { .. } => Box::new(std::iter::empty()),
        ComparisonValue::Variable { .. } => Box::new(std::iter::empty()),
    };

    this_step.chain(nested_steps)
}

fn traverse_collection<'a, Node, Nodes, I, F>(
    traverse: F,
    context: Context<'a>,
    ast_nodes: &'a Nodes,
) -> impl Iterator<Item = Result<TraversalStep<'a, 'a>>>
where
    &'a Nodes: IntoIterator<Item = Node>,
    F: Fn(Context<'a>, Node) -> I,
    I: Iterator<Item = Result<TraversalStep<'a, 'a>>>,
{
    ast_nodes
        .into_iter()
        .flat_map(move |node| traverse(context, node))
}

