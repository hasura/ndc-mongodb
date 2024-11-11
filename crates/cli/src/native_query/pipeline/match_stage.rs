use mongodb::bson::{Bson, Document};
use mongodb_support::BsonScalarType;
use nonempty::NonEmpty;

use crate::native_query::{
    aggregation_expression::infer_type_from_aggregation_expression,
    error::{Error, Result},
    pipeline_type_context::PipelineTypeContext,
    reference_shorthand::{parse_reference_shorthand, Reference},
    type_constraint::TypeConstraint,
};

pub fn check_match_doc_for_parameters(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    mut match_doc: Document,
) -> Result<()> {
    let input_document_type = context.get_input_document_type()?;
    if let Some(expression) = match_doc.remove("$expr") {
        let type_hint = TypeConstraint::Scalar(BsonScalarType::Bool);
        infer_type_from_aggregation_expression(
            context,
            desired_object_type_name,
            Some(&type_hint),
            expression,
        )?;
        Ok(())
    } else {
        check_match_doc_for_parameters_helper(
            context,
            desired_object_type_name,
            &input_document_type,
            match_doc,
        )
    }
}

fn check_match_doc_for_parameters_helper(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    input_document_type: &TypeConstraint,
    match_doc: Document,
) -> Result<()> {
    for (key, value) in match_doc {
        if key.starts_with("$") {
            analyze_match_operator(
                context,
                desired_object_type_name,
                input_document_type,
                key,
                value,
            )?;
        } else {
            analyze_input_doc_field(
                context,
                desired_object_type_name,
                input_document_type,
                key,
                value,
            )?;
        }
    }
    Ok(())
}

fn analyze_input_doc_field(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    input_document_type: &TypeConstraint,
    field_name: String,
    match_expression: Bson,
) -> Result<()> {
    let field_type = TypeConstraint::FieldOf {
        target_type: Box::new(input_document_type.clone()),
        path: NonEmpty::from_vec(field_name.split(".").map(Into::into).collect())
            .ok_or_else(|| Error::Other("object field reference is an empty string".to_string()))?,
    };
    analyze_match_expression(
        context,
        desired_object_type_name,
        &field_type,
        match_expression,
    )
}

fn analyze_match_operator(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    field_type: &TypeConstraint,
    operator: String,
    match_expression: Bson,
) -> Result<()> {
    match operator.as_ref() {
        "$and" | "$or" | "$nor" => {
            if let Bson::Array(array) = match_expression {
                for expression in array {
                    check_match_doc_for_parameters_helper(
                        context,
                        desired_object_type_name,
                        field_type,
                        expression
                            .as_document()
                            .ok_or_else(|| {
                                Error::Other(format!(
                                    "expected argument to {operator} to be an array of objects"
                                ))
                            })?
                            .clone(),
                    )?;
                }
            } else {
                Err(Error::Other(format!(
                    "expected argument to {operator} to be an array of objects"
                )))?;
            }
        }
        "$not" => {
            match match_expression {
                Bson::Document(match_doc) => check_match_doc_for_parameters_helper(
                    context,
                    desired_object_type_name,
                    field_type,
                    match_doc,
                )?,
                _ => Err(Error::Other(format!(
                    "{operator} operator requires a document",
                )))?,
            };
        }
        "$elemMatch" => {
            let element_type = field_type.clone().map_nullable(|ft| match ft {
                TypeConstraint::ArrayOf(t) => *t,
                other => TypeConstraint::ElementOf(Box::new(other)),
            });
            match match_expression {
                Bson::Document(match_doc) => check_match_doc_for_parameters_helper(
                    context,
                    desired_object_type_name,
                    &element_type,
                    match_doc,
                )?,
                _ => Err(Error::Other(format!(
                    "{operator} operator requires a document",
                )))?,
            };
        }
        "$eq" | "$ne" | "$gt" | "$lt" | "$gte" | "$lte" => analyze_match_expression(
            context,
            desired_object_type_name,
            field_type,
            match_expression,
        )?,
        "$in" | "$nin" => analyze_match_expression(
            context,
            desired_object_type_name,
            &TypeConstraint::ArrayOf(Box::new(field_type.clone())),
            match_expression,
        )?,
        "$exists" => analyze_match_expression(
            context,
            desired_object_type_name,
            &TypeConstraint::Scalar(BsonScalarType::Bool),
            match_expression,
        )?,
        // In MongoDB $type accepts either a number, a string, an array of numbers, or an array of
        // strings - for simplicity we're only accepting an array of strings since this form can
        // express all comparisons that can be expressed with the other forms.
        "$type" => analyze_match_expression(
            context,
            desired_object_type_name,
            &TypeConstraint::ArrayOf(Box::new(TypeConstraint::Scalar(BsonScalarType::String))),
            match_expression,
        )?,
        "$mod" => match match_expression {
            Bson::Array(xs) => {
                if xs.len() != 2 {
                    Err(Error::Other(format!(
                        "{operator} operator requires exactly two arguments",
                        operator = operator
                    )))?;
                }
                for divisor_or_remainder in xs {
                    analyze_match_expression(
                        context,
                        desired_object_type_name,
                        &TypeConstraint::Scalar(BsonScalarType::Int),
                        divisor_or_remainder,
                    )?;
                }
            }
            _ => Err(Error::Other(format!(
                "{operator} operator requires an array of two elements",
            )))?,
        },
        "$regex" => analyze_match_expression(
            context,
            desired_object_type_name,
            &TypeConstraint::Scalar(BsonScalarType::Regex),
            match_expression,
        )?,
        "$all" => {
            let element_type = field_type.clone().map_nullable(|ft| match ft {
                TypeConstraint::ArrayOf(t) => *t,
                other => TypeConstraint::ElementOf(Box::new(other)),
            });
            // It's like passing field_type through directly, except that we move out of
            // a possible nullable type, and we enforce an array type.
            let argument_type = TypeConstraint::ArrayOf(Box::new(element_type));
            analyze_match_expression(
                context,
                desired_object_type_name,
                &argument_type,
                match_expression,
            )?;
        }
        "$size" => analyze_match_expression(
            context,
            desired_object_type_name,
            &TypeConstraint::Scalar(BsonScalarType::Int),
            match_expression,
        )?,
        _ => Err(Error::UnknownMatchDocumentOperator(operator))?,
    }
    Ok(())
}

fn analyze_match_expression(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    field_type: &TypeConstraint,
    match_expression: Bson,
) -> Result<()> {
    match match_expression {
        Bson::String(s) => analyze_match_expression_string(context, field_type, s),
        Bson::Document(match_doc) => check_match_doc_for_parameters_helper(
            context,
            desired_object_type_name,
            field_type,
            match_doc,
        ),
        Bson::Array(xs) => {
            let element_type = field_type.clone().map_nullable(|ft| match ft {
                TypeConstraint::ArrayOf(t) => *t,
                other => TypeConstraint::ElementOf(Box::new(other)),
            });
            for x in xs {
                analyze_match_expression(context, desired_object_type_name, &element_type, x)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn analyze_match_expression_string(
    context: &mut PipelineTypeContext<'_>,
    field_type: &TypeConstraint,
    match_expression: String,
) -> Result<()> {
    // A match expression is not an aggregation expression shorthand string. But we only care about
    // variable references, and the shorthand parser gets those for us.
    match parse_reference_shorthand(&match_expression)? {
        Reference::NativeQueryVariable {
            name,
            type_annotation: _, // TODO: parse type annotation ENG-1249
        } => {
            context.register_parameter(name.into(), [field_type.clone()]);
        }
        Reference::String {
            native_query_variables,
        } => {
            for variable in native_query_variables {
                context.register_parameter(
                    variable.into(),
                    [TypeConstraint::Scalar(
                        mongodb_support::BsonScalarType::String,
                    )],
                );
            }
        }
        Reference::PipelineVariable { .. } => (),
        Reference::InputDocumentField { .. } => (),
    };
    Ok(())
}
