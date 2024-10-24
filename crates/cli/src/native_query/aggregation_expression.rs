use std::collections::BTreeMap;

use itertools::Itertools as _;
use mongodb::bson::{Bson, Document};
use mongodb_support::BsonScalarType;
use nonempty::NonEmpty;

use super::pipeline_type_context::PipelineTypeContext;

use super::error::{Error, Result};
use super::reference_shorthand::{parse_reference_shorthand, Reference};
use super::type_constraint::{ObjectTypeConstraint, TypeConstraint};

pub fn infer_type_from_aggregation_expression(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    bson: Bson,
) -> Result<TypeConstraint> {
    let t = match bson {
        Bson::Double(_) => TypeConstraint::Scalar(BsonScalarType::Double),
        Bson::String(string) => infer_type_from_reference_shorthand(context, &string)?,
        Bson::Array(_) => todo!("array type"),
        Bson::Document(doc) => {
            infer_type_from_aggregation_expression_document(context, desired_object_type_name, doc)?
        }
        Bson::Boolean(_) => TypeConstraint::Scalar(BsonScalarType::Bool),
        Bson::Null | Bson::Undefined => {
            let type_variable = context.new_type_variable([]);
            TypeConstraint::Nullable(Box::new(TypeConstraint::Variable(type_variable)))
        }
        Bson::RegularExpression(_) => TypeConstraint::Scalar(BsonScalarType::Regex),
        Bson::JavaScriptCode(_) => TypeConstraint::Scalar(BsonScalarType::Javascript),
        Bson::JavaScriptCodeWithScope(_) => {
            TypeConstraint::Scalar(BsonScalarType::JavascriptWithScope)
        }
        Bson::Int32(_) => TypeConstraint::Scalar(BsonScalarType::Int),
        Bson::Int64(_) => TypeConstraint::Scalar(BsonScalarType::Long),
        Bson::Timestamp(_) => TypeConstraint::Scalar(BsonScalarType::Timestamp),
        Bson::Binary(_) => TypeConstraint::Scalar(BsonScalarType::BinData),
        Bson::ObjectId(_) => TypeConstraint::Scalar(BsonScalarType::ObjectId),
        Bson::DateTime(_) => TypeConstraint::Scalar(BsonScalarType::Date),
        Bson::Symbol(_) => TypeConstraint::Scalar(BsonScalarType::Symbol),
        Bson::Decimal128(_) => TypeConstraint::Scalar(BsonScalarType::Decimal),
        Bson::MaxKey => TypeConstraint::Scalar(BsonScalarType::MaxKey),
        Bson::MinKey => TypeConstraint::Scalar(BsonScalarType::MinKey),
        Bson::DbPointer(_) => TypeConstraint::Scalar(BsonScalarType::DbPointer),
    };
    Ok(t)
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

fn infer_type_from_operator_expression(
    _context: &mut PipelineTypeContext<'_>,
    _desired_object_type_name: &str,
    operator: &str,
    operands: Bson,
) -> Result<TypeConstraint> {
    let t = match (operator, operands) {
        ("$split", _) => {
            TypeConstraint::ArrayOf(Box::new(TypeConstraint::Scalar(BsonScalarType::String)))
        }
        (op, _) => Err(Error::UnknownAggregationOperator(op.to_string()))?,
    };
    Ok(t)
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
    Ok(TypeConstraint::Object(object_type_name))
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
        Reference::PipelineVariable { .. } => todo!(),
        Reference::InputDocumentField { name, nested_path } => {
            let doc_type = context.get_input_document_type()?;
            let path = NonEmpty {
                head: name,
                tail: nested_path,
            };
            TypeConstraint::FieldOf {
                target_type: Box::new(doc_type.clone()),
                path,
            }
        }
        Reference::String {
            native_query_variables,
        } => {
            for variable in native_query_variables {
                context.register_parameter(
                    variable.into(),
                    [TypeConstraint::Scalar(BsonScalarType::String)],
                );
            }
            TypeConstraint::Scalar(BsonScalarType::String)
        }
    };
    Ok(t)
}
