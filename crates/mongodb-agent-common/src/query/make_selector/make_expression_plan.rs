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

