use std::collections::BTreeMap;
use std::iter::once;

use configuration::schema::{ObjectField, ObjectType, Type};
use itertools::Itertools as _;
use mongodb::bson::{Bson, Document};
use mongodb_support::BsonScalarType;

use super::helpers::nested_field_type;
use super::pipeline_type_context::PipelineTypeContext;

use super::error::{Error, Result};
use super::reference_shorthand::{parse_reference_shorthand, Reference};

pub fn infer_type_from_aggregation_expression(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    bson: Bson,
) -> Result<Type> {
    let t = match bson {
        Bson::Double(_) => Type::Scalar(BsonScalarType::Double),
        Bson::String(string) => infer_type_from_reference_shorthand(context, &string)?,
        Bson::Array(_) => todo!("array type"),
        Bson::Document(doc) => {
            infer_type_from_aggregation_expression_document(context, desired_object_type_name, doc)?
        }
        Bson::Boolean(_) => todo!(),
        Bson::Null => todo!(),
        Bson::RegularExpression(_) => todo!(),
        Bson::JavaScriptCode(_) => todo!(),
        Bson::JavaScriptCodeWithScope(_) => todo!(),
        Bson::Int32(_) => todo!(),
        Bson::Int64(_) => todo!(),
        Bson::Timestamp(_) => todo!(),
        Bson::Binary(_) => todo!(),
        Bson::ObjectId(_) => todo!(),
        Bson::DateTime(_) => todo!(),
        Bson::Symbol(_) => todo!(),
        Bson::Decimal128(_) => todo!(),
        Bson::Undefined => todo!(),
        Bson::MaxKey => todo!(),
        Bson::MinKey => todo!(),
        Bson::DbPointer(_) => todo!(),
    };
    Ok(t)
}

fn infer_type_from_aggregation_expression_document(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    mut document: Document,
) -> Result<Type> {
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
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    operator: &str,
    operands: Bson,
) -> Result<Type> {
    let t = match (operator, operands) {
        ("$split", _) => Type::ArrayOf(Box::new(Type::Scalar(BsonScalarType::String))),
        (op, _) => Err(Error::UnknownAggregationOperator(op.to_string()))?,
    };
    Ok(t)
}

/// This is a document that is not evaluated as a plain value, not as an aggregation expression.
fn infer_type_from_document(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    document: Document,
) -> Result<Type> {
    let object_type_name = context.unique_type_name(desired_object_type_name);
    let fields = document
        .into_iter()
        .map(|(field_name, bson)| {
            let field_object_type_name = format!("{desired_object_type_name}_{field_name}");
            let object_field_type =
                infer_type_from_aggregation_expression(context, &field_object_type_name, bson)?;
            let object_field = ObjectField {
                r#type: object_field_type,
                description: None,
            };
            Ok((field_name.into(), object_field))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let object_type = ObjectType {
        fields,
        description: None,
    };
    context.insert_object_type(object_type_name.clone(), object_type);
    Ok(Type::Object(object_type_name.into()))
}

pub fn infer_type_from_reference_shorthand(
    context: &mut PipelineTypeContext<'_>,
    input: &str,
) -> Result<Type> {
    let reference = parse_reference_shorthand(input)?;
    let t = match reference {
        Reference::NativeQueryVariable {
            name,
            type_annotation,
        } => todo!(),
        Reference::PipelineVariable { name, nested_path } => todo!(),
        Reference::InputDocumentField { name, nested_path } => {
            let doc_type = context.get_input_document_type_name()?;
            let path = once(&name).chain(&nested_path);
            nested_field_type(context, doc_type.to_string(), path)?
        }
        Reference::String => Type::Scalar(BsonScalarType::String),
    };
    Ok(t)
}
