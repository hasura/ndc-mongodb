use dc_api_types::{
    ArrayComparisonValue, BinaryArrayComparisonOperator, BinaryComparisonOperator,
    ComparisonColumn, ComparisonValue, ExistsInTable, Expression,
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

pub fn equal(op1: ComparisonColumn, op2: ComparisonValue) -> Expression {
    Expression::ApplyBinaryComparison {
        column: op1,
        operator: BinaryComparisonOperator::Equal,
        value: op2,
    }
}

pub fn binop<S>(oper: S, op1: ComparisonColumn, op2: ComparisonValue) -> Expression
where
    S: ToString,
{
    Expression::ApplyBinaryComparison {
        column: op1,
        operator: BinaryComparisonOperator::CustomBinaryComparisonOperator(oper.to_string()),
        value: op2,
    }
}

pub fn is_in<I>(op1: ComparisonColumn, value_type: &str, values: I) -> Expression
where
    I: IntoIterator<Item = ArrayComparisonValue>,
{
    Expression::ApplyBinaryArrayComparison {
        column: op1,
        operator: BinaryArrayComparisonOperator::In,
        value_type: value_type.to_owned(),
        values: values.into_iter().collect(),
    }
}

pub fn exists(relationship: &str, predicate: Expression) -> Expression {
    Expression::Exists {
        in_table: ExistsInTable::RelatedTable {
            relationship: relationship.to_owned(),
        },
        r#where: Box::new(predicate),
    }
}

pub fn exists_unrelated(
    table: impl IntoIterator<Item = impl ToString>,
    predicate: Expression,
) -> Expression {
    Expression::Exists {
        in_table: ExistsInTable::UnrelatedTable {
            table: table.into_iter().map(|v| v.to_string()).collect(),
        },
        r#where: Box::new(predicate),
    }
}
