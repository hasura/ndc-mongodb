use std::collections::BTreeMap;

use itertools::Itertools as _;
use mongodb::bson::{Bson, Document};
use mongodb_support::BsonScalarType;
use nonempty::NonEmpty;

use super::pipeline_type_context::PipelineTypeContext;

use super::error::{Error, Result};
use super::reference_shorthand::{parse_reference_shorthand, Reference};
use super::type_constraint::{ObjectTypeConstraint, TypeConstraint, Variance};

use TypeConstraint as C;

pub fn infer_type_from_aggregation_expression(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    type_hint: Option<&TypeConstraint>,
    expression: Bson,
) -> Result<TypeConstraint> {
    let t = match expression {
        Bson::Double(_) => C::Scalar(BsonScalarType::Double),
        Bson::String(string) => infer_type_from_reference_shorthand(context, type_hint, &string)?,
        Bson::Array(elems) => {
            infer_type_from_array(context, desired_object_type_name, type_hint, elems)?
        }
        Bson::Document(doc) => infer_type_from_aggregation_expression_document(
            context,
            desired_object_type_name,
            type_hint,
            doc,
        )?,
        Bson::Boolean(_) => C::Scalar(BsonScalarType::Bool),
        Bson::Null | Bson::Undefined => C::Scalar(BsonScalarType::Null),
        Bson::RegularExpression(_) => C::Scalar(BsonScalarType::Regex),
        Bson::JavaScriptCode(_) => C::Scalar(BsonScalarType::Javascript),
        Bson::JavaScriptCodeWithScope(_) => C::Scalar(BsonScalarType::JavascriptWithScope),
        Bson::Int32(_) => C::Scalar(BsonScalarType::Int),
        Bson::Int64(_) => C::Scalar(BsonScalarType::Long),
        Bson::Timestamp(_) => C::Scalar(BsonScalarType::Timestamp),
        Bson::Binary(_) => C::Scalar(BsonScalarType::BinData),
        Bson::ObjectId(_) => C::Scalar(BsonScalarType::ObjectId),
        Bson::DateTime(_) => C::Scalar(BsonScalarType::Date),
        Bson::Symbol(_) => C::Scalar(BsonScalarType::Symbol),
        Bson::Decimal128(_) => C::Scalar(BsonScalarType::Decimal),
        Bson::MaxKey => C::Scalar(BsonScalarType::MaxKey),
        Bson::MinKey => C::Scalar(BsonScalarType::MinKey),
        Bson::DbPointer(_) => C::Scalar(BsonScalarType::DbPointer),
    };
    Ok(t)
}

pub fn infer_types_from_aggregation_expression_tuple(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    type_hint_for_elements: Option<&TypeConstraint>,
    bson: Bson,
) -> Result<Vec<TypeConstraint>> {
    let tuple = match bson {
        Bson::Array(exprs) => exprs
            .into_iter()
            .map(|expr| {
                infer_type_from_aggregation_expression(
                    context,
                    desired_object_type_name,
                    type_hint_for_elements,
                    expr,
                )
            })
            .collect::<Result<Vec<_>>>()?,
        expr => {
            let t = infer_type_from_aggregation_expression(
                context,
                desired_object_type_name,
                None,
                expr,
            )?;
            vec![t]
        }
    };
    Ok(tuple)
}

fn infer_type_from_array(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    type_hint_for_entire_array: Option<&TypeConstraint>,
    elements: Vec<Bson>,
) -> Result<TypeConstraint> {
    let elem_type_hint = type_hint_for_entire_array.map(|hint| match hint {
        C::ArrayOf(t) => *t.clone(),
        t => C::ElementOf(Box::new(t.clone())),
    });
    Ok(C::Union(
        elements
            .into_iter()
            .map(|elem| {
                infer_type_from_aggregation_expression(
                    context,
                    desired_object_type_name,
                    elem_type_hint.as_ref(),
                    elem,
                )
            })
            .collect::<Result<_>>()?,
    ))
}

fn infer_type_from_aggregation_expression_document(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    type_hint_for_entire_object: Option<&TypeConstraint>,
    mut document: Document,
) -> Result<TypeConstraint> {
    let mut expression_operators = document
        .keys()
        .filter(|key| key.starts_with("$"))
        .collect_vec();
    let expression_operator = expression_operators.pop().map(ToString::to_string);
    let is_empty = expression_operators.is_empty();
    match (expression_operator, is_empty) {
        (_, false) => Err(Error::MultipleExpressionOperators(document)),
        (Some(operator), _) => {
            let operands = document.remove(&operator).unwrap();
            infer_type_from_operator_expression(
                context,
                desired_object_type_name,
                type_hint_for_entire_object,
                &operator,
                operands,
            )
        }
        (None, _) => infer_type_from_document(context, desired_object_type_name, document),
    }
}

// TODO: propagate expected type based on operator used
fn infer_type_from_operator_expression(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    type_hint: Option<&TypeConstraint>,
    operator: &str,
    operand: Bson,
) -> Result<TypeConstraint> {
    // NOTE: It is important to run inference on `operand` in every match arm even if we don't read
    // the result because we need to check for uses of parameters.
    let t = match operator {
        // technically $abs returns the same *numeric* type as its input, and fails on other types
        "$abs" => infer_type_from_aggregation_expression(
            context,
            desired_object_type_name,
            type_hint.or(Some(&C::numeric())),
            operand,
        )?,
        "$sin" | "$cos" | "$tan" | "$asin" | "$acos" | "$atan" | "$asinh" | "$acosh" | "$atanh"
        | "$sinh" | "$cosh" | "$tanh" => {
            type_for_trig_operator(infer_type_from_aggregation_expression(
                context,
                desired_object_type_name,
                Some(&C::numeric()),
                operand,
            )?)
        }
        "$add" => {
            let operand_types = infer_types_from_aggregation_expression_tuple(
                context,
                desired_object_type_name,
                Some(&C::numeric()),
                operand,
            )?;
            operand_types.into_iter().next().unwrap()
        }
        // "$addToSet" => todo!(),
        "$allElementsTrue" => {
            infer_type_from_aggregation_expression(
                context,
                desired_object_type_name,
                Some(&C::ArrayOf(Box::new(C::Scalar(BsonScalarType::Bool)))),
                operand,
            )?;
            C::Scalar(BsonScalarType::Bool)
        }
        "$and" => {
            infer_type_from_aggregation_expression(
                context,
                desired_object_type_name,
                None,
                operand,
            )?;
            C::Scalar(BsonScalarType::Bool)
        }
        "$anyElementTrue" => {
            infer_type_from_aggregation_expression(
                context,
                desired_object_type_name,
                Some(&C::ArrayOf(Box::new(C::Scalar(BsonScalarType::Bool)))),
                operand,
            )?;
            C::Scalar(BsonScalarType::Bool)
        }
        "$arrayElemAt" => {
            let (array_ref, idx) = two_paramater_operand(operator, operand)?;
            let array_type = infer_type_from_aggregation_expression(
                context,
                &format!("{desired_object_type_name}_arrayElemAt_array"),
                type_hint.map(|t| C::ArrayOf(Box::new(t.clone()))).as_ref(),
                array_ref,
            )?;
            infer_type_from_aggregation_expression(
                context,
                &format!("{desired_object_type_name}_arrayElemAt_idx"),
                Some(&C::Scalar(BsonScalarType::Int)),
                idx,
            )?;
            type_hint
                .cloned()
                .unwrap_or_else(|| C::ElementOf(Box::new(array_type)))
        }
        // "$arrayToObject" => todo!(),
        "$asin" => type_for_trig_operator(infer_type_from_aggregation_expression(
            context,
            desired_object_type_name,
            Some(&C::numeric()),
            operand,
        )?),
        "$eq" => {
            let (a, b) = two_paramater_operand(operator, operand)?;
            let variable = context.new_type_variable(Variance::Covariant, []);
            let type_a = infer_type_from_aggregation_expression(
                context,
                desired_object_type_name,
                Some(&C::Variable(variable)),
                a,
            )?;
            let type_b = infer_type_from_aggregation_expression(
                context,
                desired_object_type_name,
                Some(&C::Variable(variable)),
                b,
            )?;
            // Avoid cycles of type variable references
            if !context.constraint_references_variable(&type_a, variable) {
                context.set_type_variable_constraint(variable, type_a);
            }
            if !context.constraint_references_variable(&type_b, variable) {
                context.set_type_variable_constraint(variable, type_b);
            }
            C::Scalar(BsonScalarType::Bool)
        }
        "$split" => {
            infer_types_from_aggregation_expression_tuple(
                context,
                desired_object_type_name,
                Some(&C::Scalar(BsonScalarType::String)),
                operand,
            )?;
            C::ArrayOf(Box::new(C::Scalar(BsonScalarType::String)))
        }
        op => Err(Error::UnknownAggregationOperator(op.to_string()))?,
    };
    Ok(t)
}

fn two_paramater_operand(operator: &str, operand: Bson) -> Result<(Bson, Bson)> {
    match operand {
        Bson::Array(operands) => {
            if operands.len() != 2 {
                return Err(Error::Other(format!(
                    "argument to {operator} must be a two-element array"
                )));
            }
            let mut operands = operands.into_iter();
            let a = operands.next().unwrap();
            let b = operands.next().unwrap();
            Ok((a, b))
        }
        other_bson => Err(Error::ExpectedArrayExpressionArgument {
            actual_argument: other_bson,
        })?,
    }
}

pub fn type_for_trig_operator(operand_type: TypeConstraint) -> TypeConstraint {
    operand_type.map_nullable(|t| match t {
        t @ C::Scalar(BsonScalarType::Decimal) => t,
        _ => C::Scalar(BsonScalarType::Double),
    })
}

/// This is a document that is not evaluated as a plain value, not as an aggregation expression.
fn infer_type_from_document(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    document: Document,
) -> Result<TypeConstraint> {
    let object_type_name = context.unique_type_name(desired_object_type_name);
    let fields = document
        .into_iter()
        .map(|(field_name, bson)| {
            let field_object_type_name = format!("{desired_object_type_name}_{field_name}");
            let object_field_type = infer_type_from_aggregation_expression(
                context,
                &field_object_type_name,
                None,
                bson,
            )?;
            Ok((field_name.into(), object_field_type))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let object_type = ObjectTypeConstraint { fields };
    context.insert_object_type(object_type_name.clone(), object_type);
    Ok(C::Object(object_type_name))
}

pub fn infer_type_from_reference_shorthand(
    context: &mut PipelineTypeContext<'_>,
    type_hint: Option<&TypeConstraint>,
    input: &str,
) -> Result<TypeConstraint> {
    let reference = parse_reference_shorthand(input)?;
    let t = match reference {
        Reference::NativeQueryVariable {
            name,
            type_annotation: _,
        } => {
            // TODO: read type annotation ENG-1249
            context.register_parameter(name.into(), type_hint.into_iter().cloned())
        }
        Reference::PipelineVariable { .. } => todo!("pipeline variable"),
        Reference::InputDocumentField { name, nested_path } => {
            let doc_type = context.get_input_document_type()?;
            let path = NonEmpty {
                head: name,
                tail: nested_path,
            };
            C::FieldOf {
                target_type: Box::new(doc_type.clone()),
                path,
            }
        }
        Reference::String {
            native_query_variables,
        } => {
            for variable in native_query_variables {
                context.register_parameter(variable.into(), [C::Scalar(BsonScalarType::String)]);
            }
            C::Scalar(BsonScalarType::String)
        }
    };
    Ok(t)
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use mongodb::bson::bson;
    use mongodb_support::BsonScalarType;
    use test_helpers::configuration::mflix_config;

    use crate::native_query::{
        pipeline_type_context::PipelineTypeContext,
        type_constraint::{TypeConstraint, TypeVariable},
    };

    use super::infer_type_from_operator_expression;

    use TypeConstraint as C;

    #[googletest::test]
    fn infers_constrants_on_equality() -> Result<()> {
        let config = mflix_config();
        let mut context = PipelineTypeContext::new(&config, None);

        let (var0, var1) = (
            TypeVariable::new(0, crate::native_query::type_constraint::Variance::Covariant),
            TypeVariable::new(
                1,
                crate::native_query::type_constraint::Variance::Contravariant,
            ),
        );

        infer_type_from_operator_expression(
            &mut context,
            "test",
            None,
            "$eq",
            bson!(["{{ parameter }}", 1]),
        )?;

        expect_eq!(
            context.type_variables(),
            &[
                (var0, [C::Scalar(BsonScalarType::Int)].into()),
                (var1, [C::Variable(var0)].into())
            ]
            .into()
        );

        Ok(())
    }
}
