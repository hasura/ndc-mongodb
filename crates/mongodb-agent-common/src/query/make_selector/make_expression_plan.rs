use crate::mongo_query_plan::Expression;

use super::{
    make_aggregation_expression::{make_aggregation_expression, AggregationExpression},
    make_query_document::{make_query_document, QueryDocument},
    Result,
};

/// Represents the body of a `$match` stage which may use a special shorthand syntax (query
/// document) where document keys are interpreted as field references, or if the entire match
/// document is enclosed in an object with an `$expr` property then it is interpreted as an
/// aggregation expression.
#[derive(Clone, Debug)]
pub enum ExpressionPlan {
    QueryDocument(QueryDocument),
    AggregationExpression(AggregationExpression),
}

pub fn make_expression_plan(expression: &Expression) -> Result<ExpressionPlan> {
    if let Some(query_doc) = make_query_document(expression)? {
        Ok(ExpressionPlan::QueryDocument(query_doc))
    } else {
        let aggregation_expression = make_aggregation_expression(expression)?;
        Ok(ExpressionPlan::AggregationExpression(
            aggregation_expression,
        ))
    }
}
