use std::iter::once;

use mongodb::bson::{self, doc, Bson};

use crate::{
    mongo_query_plan::{ComparisonValue, ExistsInCollection, Expression},
    query::column_ref::{column_expression, ColumnRef},
};

use super::{
    make_aggregation_expression::{
        make_aggregation_expression, make_aggregation_expression_for_exists,
        make_aggregation_expression_for_exists_in_related, AggregationExpression,
    },
    Result,
};

pub struct QueryDocument(pub bson::Document);

/// Represents the body of a `$match` stage which may use a special shorthand syntax (query
/// document) where document keys are interpreted as field references, or if the entire match
/// document is enclosed in an object with an `$expr` property then it is interpreted as an
/// aggregation expression.
pub enum ExpressionPlan {
    QueryDocument(QueryDocument),
    AggregationExpression(AggregationExpression),
}

impl ExpressionPlan {
    fn query_document(doc: bson::Document) -> Self {
        Self::QueryDocument(QueryDocument(doc))
    }

    fn aggregation_expression(expr: impl Into<Bson>) -> Self {
        Self::AggregationExpression(AggregationExpression(expr.into()))
    }

    fn is_aggregation_expression(&self) -> bool {
        match self {
            ExpressionPlan::QueryDocument(_) => false,
            ExpressionPlan::AggregationExpression(_) => true,
        }
    }

    // /// Force a list of expression plans into either [QueryDocument] or [AggregationExpression].
    // fn with_shorthands_or_expressions<T>(
    //     expressions: impl IntoIterator<Item = ExpressionPlan>,
    //     from_query_documents: impl FnOnce(Vec<QueryDocument>) -> T,
    // ) -> Either<Vec<QueryDocument>, Vec<AggregationExpression>> {
    // }

    /// Convert a list of [ExpressionPlan] into a list of [QueryDocument] if every expression in
    /// the input is a [QueryDocument]. Otherwise returns None.
    fn as_query_documents(
        expressions: impl IntoIterator<Item = ExpressionPlan>,
    ) -> Option<Vec<bson::Document>> {
        let mut docs = vec![];
        for expr in expressions {
            match expr {
                ExpressionPlan::QueryDocument(QueryDocument(document)) => docs.push(document),
                ExpressionPlan::AggregationExpression(_) => return None,
            }
        }
        Some(docs)
    }

    fn bimap<F, G>(self, map_query_document: F, map_aggregation_expression: G) -> Self
    where
        F: Fn(QueryDocument) -> ExpressionPlan,
        G: Fn(AggregationExpression) -> AggregationExpression,
    {
        match self {
            Self::QueryDocument(d) => map_query_document(d),
            Self::AggregationExpression(e) => {
                Self::AggregationExpression(map_aggregation_expression(e))
            }
        }
    }
}

pub fn make_expression_plan(expr: &Expression) -> Result<ExpressionPlan> {
    match expr {
        Expression::And { expressions } => {
            let sub_exps: Vec<ExpressionPlan> = expressions
                .clone()
                .iter()
                .map(make_expression_plan)
                .collect::<Result<_>>()?;
            let plan = match ExpressionPlan::as_query_documents(sub_exps) {
                Some(sub_exps) => ExpressionPlan::query_document(doc! { "$and": sub_exps }),

                // If any of the sub expressions are not query documents then we have to back-track
                // and map everything to aggregation expressions.
                None => ExpressionPlan::AggregationExpression(make_aggregation_expression(expr)?),
            };
            Ok(plan)
        }
        Expression::Or { expressions } => {
            let sub_exps: Vec<ExpressionPlan> = expressions
                .clone()
                .iter()
                .map(make_expression_plan)
                .collect::<Result<_>>()?;
            let plan = match ExpressionPlan::as_query_documents(sub_exps) {
                Some(sub_exps) => ExpressionPlan::query_document(doc! { "$or": sub_exps }),

                // If any of the sub expressions are not query documents then we have to back-track
                // and map everything to aggregation expressions.
                None => ExpressionPlan::AggregationExpression(make_aggregation_expression(expr)?),
            };
            Ok(plan)
        }
        Expression::Not { expression } => {
            let sub_expression = make_expression_plan(expression)?;
            let plan = sub_expression.bimap(
                |QueryDocument(d)| ExpressionPlan::query_document(doc! { "$nor": [d] }),
                |AggregationExpression(e)| AggregationExpression(doc! { "$nor": [e] }.into()),
            );
            Ok(plan)
        }
        Expression::Exists {
            in_collection,
            predicate,
        } => make_expression_plan_for_exists(in_collection, predicate.as_deref()),
        Expression::BinaryComparisonOperator {
            column,
            operator,
            value,
        } => make_binary_comparison_selector(column, operator, value),
        Expression::UnaryComparisonOperator { column, operator } => {
            make_unary_comparison_selector(column, operator)
        }
    }
}

fn make_expression_plan_for_exists(
    in_collection: &ExistsInCollection,
    predicate: Option<&Expression>,
) -> Result<ExpressionPlan> {
    let plan = match (in_collection, predicate) {
        (ExistsInCollection::Related { relationship }, Some(predicate)) => {
            let relationship_ref = ColumnRef::from_relationship(relationship);
            let sub_expression = make_expression_plan(predicate)?;
            match (relationship_ref, sub_expression) {
                (
                    ColumnRef::MatchKey(key),
                    ExpressionPlan::QueryDocument(QueryDocument(query_doc)),
                ) => ExpressionPlan::query_document(doc! {
                    key: { "$elemMatch": query_doc }
                }),
                (_, _) => ExpressionPlan::AggregationExpression(
                    make_aggregation_expression_for_exists(in_collection, Some(predicate))?,
                ),
            }
        }
        (ExistsInCollection::Related { relationship }, None) => {
            let relationship_ref = ColumnRef::from_relationship(relationship);
            match relationship_ref {
                ColumnRef::MatchKey(key) => {
                    ExpressionPlan::query_document(doc! { format!("{key}.0"): { "$exists": true } })
                }
                _ => ExpressionPlan::AggregationExpression(make_aggregation_expression_for_exists(
                    in_collection,
                    predicate,
                )?),
            }
        }
        (ExistsInCollection::Unrelated { .. }, _) => ExpressionPlan::AggregationExpression(
            make_aggregation_expression_for_exists(in_collection, predicate)?,
        ),
        (
            ExistsInCollection::NestedCollection {
                column_name,
                field_path,
                ..
            },
            Some(predicate),
        ) => {
            let column_ref = ColumnRef::from_field_path(field_path.iter().chain(once(column_name)));
            let sub_expression = make_expression_plan(predicate)?;
            match (column_ref, sub_expression) {
                (
                    ColumnRef::MatchKey(key),
                    ExpressionPlan::QueryDocument(QueryDocument(query_doc)),
                ) => ExpressionPlan::query_document(doc! {
                    key: {
                        "$elemMatch": query_doc
                    }
                }),
                (_, _) => ExpressionPlan::AggregationExpression(
                    make_aggregation_expression_for_exists(in_collection, Some(predicate))?,
                ), // (
                   //     column_expr @ (ColumnRef::ExpressionStringShorthand(_)
                   //     | ColumnRef::Expression(_)),
                   //     Some(predicate),
                   // ) => {
                   //     // TODO: NDC-436 We need to be able to create a plan for `predicate` that
                   //     // evaluates with the variable `$$this` as document root since that
                   //     // references each array element. With reference to the plan in the
                   //     // TODO comment above, this scoped predicate plan needs to be created
                   //     // with `make_aggregation_expression` since we are in an aggregate
                   //     // expression context at this point.
                   //     let predicate_scoped_to_nested_document: Document =
                   //             Err(MongoAgentError::NotImplemented(format!("currently evaluating the predicate, {predicate:?}, in a nested collection context is not implemented").into()))?;
                   //     doc! {
                   //         "$expr": {
                   //            "$anyElementTrue": {
                   //                 "$map": {
                   //                     "input": column_expr.into_aggregate_expression(),
                   //                     "in": predicate_scoped_to_nested_document,
                   //                 }
                   //             }
                   //         }
                   //     }
                   // }
                   // (
                   //     column_expr @ (ColumnRef::ExpressionStringShorthand(_)
                   //     | ColumnRef::Expression(_)),
                   //     None,
                   // ) => {
                   //     doc! {
                   //         "$expr": {
                   //             "$gt": [{ "$size": column_expr.into_aggregate_expression() }, 0]
                   //         }
                   //     }
                   // }
            }
        }
        (
            ExistsInCollection::NestedCollection {
                column_name,
                field_path,
                ..
            },
            None,
        ) => {
            let column_ref = ColumnRef::from_field_path(field_path.iter().chain(once(column_name)));
            match column_ref {
                ColumnRef::MatchKey(key) => ExpressionPlan::query_document(doc! {
                    key: {
                        "$exists": true,
                        "$not": { "$size": 0 },
                    }
                }),
                _ => ExpressionPlan::AggregationExpression(make_aggregation_expression_for_exists(
                    in_collection,
                    predicate,
                )?),
            }
        }
    };
    Ok(plan)
}

fn make_binary_comparison_selector(
    target_column: &ComparisonTarget,
    operator: &ComparisonFunction,
    value: &ComparisonValue,
) -> Result<ExpressionPlan> {
    let selector = match value {
        ComparisonValue::Column {
            column: value_column,
        } => {
            if !target_column.relationship_path().is_empty()
                || !value_column.relationship_path().is_empty()
            {
                return Err(MongoAgentError::NotImplemented(
                    "binary comparisons between two fields where either field is in a related collection".into(),
                ));
            }
            doc! {
                "$expr": operator.mongodb_aggregation_expression(
                    column_expression(target_column),
                    column_expression(value_column)
                )
            }
        }
        ComparisonValue::Scalar { value, value_type } => {
            let comparison_value = bson_from_scalar_value(value, value_type)?;
            let match_doc = match ColumnRef::from_comparison_target(target_column) {
                ColumnRef::MatchKey(key) => operator.mongodb_match_query(key, comparison_value),
                expr => {
                    // Special case for array-to-scalar comparisons - this is required because implicit
                    // existential quantification over arrays for scalar comparisons does not work in
                    // aggregation expressions.
                    if target_column.get_field_type().is_array() && !value_type.is_array() {
                        doc! {
                            "$expr": {
                                "$reduce": {
                                    "input": expr.into_aggregate_expression(),
                                    "initialValue": false,
                                    "in": operator.mongodb_aggregation_expression("$$this", comparison_value)
                                },
                            },
                        }
                    } else {
                        doc! {
                            "$expr": operator.mongodb_aggregation_expression(expr.into_aggregate_expression(), comparison_value)
                        }
                    }
                }
            };
            traverse_relationship_path(target_column.relationship_path(), match_doc)
        }
        ComparisonValue::Variable {
            name,
            variable_type,
        } => {
            let comparison_value = variable_to_mongo_expression(name, variable_type);
            let match_doc =
                // Special case for array-to-scalar comparisons - this is required because implicit
                // existential quantification over arrays for scalar comparisons does not work in
                // aggregation expressions.
                if target_column.get_field_type().is_array() && !variable_type.is_array() {
                    doc! {
                        "$expr": {
                            "$reduce": {
                                "input": column_expression(target_column),
                                "initialValue": false,
                                "in": operator.mongodb_aggregation_expression("$$this", comparison_value)
                            },
                        },
                    }
                } else {
                    doc! {
                        "$expr": operator.mongodb_aggregation_expression(
                            column_expression(target_column),
                            comparison_value
                        )
                    }
                };
            traverse_relationship_path(target_column.relationship_path(), match_doc)
        }
    };
    Ok(selector)
}

fn make_unary_comparison_selector(
    target_column: &ComparisonTarget,
    operator: &UnaryComparisonOperator,
) -> Result<ExpressionPlan> {
    match operator {
        UnaryComparisonOperator::IsNull => {
            let match_doc = match ColumnRef::from_comparison_target(column) {
                ColumnRef::MatchKey(key) => doc! {
                    key: { "$eq": null }
                },
                expr => {
                    // Special case for array-to-scalar comparisons - this is required because implicit
                    // existential quantification over arrays for scalar comparisons does not work in
                    // aggregation expressions.
                    if column.get_field_type().is_array() {
                        doc! {
                            "$expr": {
                                "$reduce": {
                                    "input": expr.into_aggregate_expression(),
                                    "initialValue": false,
                                    "in": { "$eq": ["$$this", null] }
                                },
                            },
                        }
                    } else {
                        doc! {
                            "$expr": {
                                "$eq": [expr.into_aggregate_expression(), null]
                            }
                        }
                    }
                }
            };
            Ok(traverse_relationship_path(
                column.relationship_path(),
                match_doc,
            ))
        }
    }
}

/// For simple cases the target of an expression is a field reference. But if the target is
/// a column of a related collection then we're implicitly making an array comparison (because
/// related documents always come as an array, even for object relationships), so we have to wrap
/// the starting expression with an `$elemMatch` for each relationship that is traversed to reach
/// the target column.
fn traverse_relationship_path(
    path: &[ndc_models::RelationshipName],
    mut expression: Document,
) -> Document {
    for path_element in path.iter().rev() {
        expression = doc! {
            path_element.to_string(): {
                "$elemMatch": expression
            }
        }
    }
    expression
}

fn variable_to_mongo_expression(
    variable: &ndc_models::VariableName,
    value_type: &Type,
) -> bson::Bson {
    let mongodb_var_name = query_variable_name(variable, value_type);
    format!("$${mongodb_var_name}").into()
}

/// Convert a JSON Value into BSON using the provided type information.
/// For example, parses values of type "Date" into BSON DateTime.
fn bson_from_scalar_value(value: &serde_json::Value, value_type: &Type) -> Result<bson::Bson> {
    json_to_bson(value_type, value.clone()).map_err(|e| MongoAgentError::BadQuery(anyhow!(e)))
}
