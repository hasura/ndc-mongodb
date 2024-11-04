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
        Bson::String(string) => infer_type_from_reference_shorthand(context, &string)?,
        Bson::Array(_) => todo!("array type"),
        Bson::Document(doc) => {
            infer_type_from_aggregation_expression_document(context, desired_object_type_name, doc)?
        }
        Bson::Boolean(_) => C::Scalar(BsonScalarType::Bool),
        Bson::Null | Bson::Undefined => {
            let type_variable = context.new_type_variable(Variance::Covariant, []);
            C::Nullable(Box::new(C::Variable(type_variable)))
        }
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
    type_hint: Option<&TypeConstraint>,
    bson: Bson,
) -> Result<Vec<TypeConstraint>> {
    let tuple = match bson {
        Bson::Array(exprs) => exprs
            .into_iter()
            .map(|expr| {
                infer_type_from_aggregation_expression(
                    context,
                    desired_object_type_name,
                    type_hint,
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

fn infer_type_from_aggregation_expression_document(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
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
            Some(&C::Numeric),
            operand,
        )?,
        "$acos" => type_for_trig_operator(infer_type_from_aggregation_expression(
            context,
            desired_object_type_name,
            Some(&C::Numeric),
            operand,
        )?)?,
        "$acosh" => type_for_trig_operator(infer_type_from_aggregation_expression(
            context,
            desired_object_type_name,
            Some(&C::Numeric),
            operand,
        )?)?,
        "$add" => {
            let operand_types = infer_types_from_aggregation_expression_tuple(
                context,
                desired_object_type_name,
                Some(&C::Union(vec![C::Numeric, C::Scalar(BsonScalarType::Date)])),
                operand,
            )?;
            if operand_types
                .iter()
                .any(|t| matches!(t, &C::Scalar(BsonScalarType::Date)))
            {
                C::Scalar(BsonScalarType::Date)
            } else {
                operand_types.into_iter().next().unwrap()
            }
        }
        // "$addToSet" => todo!(),
        "$allElementsTrue" => {
            infer_type_from_aggregation_expression(
                context,
                desired_object_type_name,
                Some(&C::ArrayOf(Box::new(C::Variable(
                    context.new_type_variable(Variance::Covariant, []),
                )))),
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
        "$anyElementsTrue" => {
            infer_type_from_aggregation_expression(
                context,
                desired_object_type_name,
                Some(&C::ArrayOf(Box::new(C::Variable(
                    context.new_type_variable(Variance::Covariant, []),
                )))),
                operand,
            )?;
            C::Scalar(BsonScalarType::Bool)
        }
        "$arrayElemAt" => {
            let array_type = match operand {
                Bson::Array(operands) => {
                    let constraint =
                        C::Variable(context.new_type_variable(Variance::Covariant, []));
                    for operand in operands {
                        infer_types_from_aggregation_expression_tuple(
                            context,
                            desired_object_type_name,
                            Some(&constraint),
                            operand,
                        )?;
                    }
                }
                _ => Err(Error::ExpectedArray { actual_type: () })
            };
            let array_type =
                infer_type_from_aggregation_expression(context, desired_object_type_name, operand)?;
            C::ElementOf(Box::new(array_type))
        }
        // "$arrayToObject" => todo!(),
        "$asin" => type_for_trig_operator(infer_type_from_aggregation_expression(
            context,
            desired_object_type_name,
            Some(&C::Numeric),
            operand,
        )?)?,
        "$eq" => {
            match operand {
                Bson::Array(operands) => {
                    let constraint =
                        C::Variable(context.new_type_variable(Variance::Covariant, []));
                    for operand in operands {
                        infer_types_from_aggregation_expression_tuple(
                            context,
                            desired_object_type_name,
                            Some(&constraint),
                            operand,
                        )?;
                    }
                }
                expression => {
                    infer_type_from_aggregation_expression(
                        context,
                        desired_object_type_name,
                        None,
                        expression,
                    )?;
                }
            };
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

fn type_for_trig_operator(operand_type: TypeConstraint) -> Result<TypeConstraint> {
    Ok(map_nullable(operand_type, |t| match t {
        t @ C::Scalar(BsonScalarType::Decimal) => t,
        _ => C::Scalar(BsonScalarType::Double),
    }))
}

fn map_nullable<F>(constraint: TypeConstraint, callback: F) -> TypeConstraint
where
    F: FnOnce(TypeConstraint) -> TypeConstraint,
{
    match constraint {
        C::Nullable(t) => C::Nullable(Box::new(callback(*t))),
        t => callback(t),
    }
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
            let object_field_type =
                infer_type_from_aggregation_expression(context, &field_object_type_name, bson)?;
            Ok((field_name.into(), object_field_type))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let object_type = ObjectTypeConstraint { fields };
    context.insert_object_type(object_type_name.clone(), object_type);
    Ok(C::Object(object_type_name))
}

pub fn infer_type_from_reference_shorthand(
    context: &mut PipelineTypeContext<'_>,
    input: &str,
) -> Result<TypeConstraint> {
    let reference = parse_reference_shorthand(input)?;
    let t = match reference {
        Reference::NativeQueryVariable {
            name,
            type_annotation: _,
        } => {
            // TODO: read type annotation ENG-1249
            // TODO: set constraint based on expected type here like we do in match_stage.rs NDC-1251
            context.register_parameter(name.into(), [])
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
