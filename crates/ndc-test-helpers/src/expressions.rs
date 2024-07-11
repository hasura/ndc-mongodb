use ndc_models::{
    ComparisonTarget, ComparisonValue, ExistsInCollection, Expression, UnaryComparisonOperator,
};

pub fn and<I>(operands: I) -> Expression
where
    I: IntoIterator<Item = Expression>,
{
    Expression::And {
        expressions: operands.into_iter().collect(),
    }
}

pub fn or<I>(operands: I) -> Expression
where
    I: IntoIterator<Item = Expression>,
{
    Expression::Or {
        expressions: operands.into_iter().collect(),
    }
}

pub fn not(operand: Expression) -> Expression {
    Expression::Not {
        expression: Box::new(operand),
    }
}

pub fn is_null(target: ComparisonTarget) -> Expression {
    Expression::UnaryComparisonOperator {
        column: target,
        operator: UnaryComparisonOperator::IsNull,
    }
}

pub fn binop<S>(oper: S, op1: ComparisonTarget, op2: ComparisonValue) -> Expression
where
    S: ToString,
{
    Expression::BinaryComparisonOperator {
        column: op1,
        operator: oper.to_string().into(),
        value: op2,
    }
}

pub fn is_in<I>(op1: ComparisonTarget, values: I) -> Expression
where
    I: IntoIterator<Item = serde_json::Value>,
{
    Expression::BinaryComparisonOperator {
        column: op1,
        operator: "_in".into(),
        value: ComparisonValue::Scalar {
            value: values.into_iter().collect(),
        },
    }
}

pub fn exists(in_collection: ExistsInCollection, predicate: Expression) -> Expression {
    Expression::Exists {
        in_collection,
        predicate: Some(Box::new(predicate)),
    }
}
