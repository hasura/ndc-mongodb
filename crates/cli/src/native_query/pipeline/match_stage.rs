use mongodb::bson::{Bson, Document};
use nonempty::nonempty;

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
        infer_type_from_aggregation_expression(context, desired_object_type_name, expression)?;
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
    if match_doc.keys().any(|key| key.starts_with("$")) {
        analyze_document_with_match_operators(
            context,
            desired_object_type_name,
            input_document_type,
            match_doc,
        )
    } else {
        analyze_document_with_field_name_keys(
            context,
            desired_object_type_name,
            input_document_type,
            match_doc,
        )
    }
}

fn analyze_document_with_field_name_keys(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    input_document_type: &TypeConstraint,
    match_doc: Document,
) -> Result<()> {
    for (field_name, match_expression) in match_doc {
        let field_type = TypeConstraint::FieldOf {
            target_type: Box::new(input_document_type.clone()),
            path: nonempty![field_name.into()],
        };
        analyze_match_expression(
            context,
            desired_object_type_name,
            &field_type,
            match_expression,
        )?;
    }
    Ok(())
}

fn analyze_document_with_match_operators(
    context: &mut PipelineTypeContext<'_>,
    desired_object_type_name: &str,
    field_type: &TypeConstraint,
    match_doc: Document,
) -> Result<()> {
    for (operator, match_expression) in match_doc {
        match operator.as_ref() {
            "$eq" => analyze_match_expression(
                context,
                desired_object_type_name,
                field_type,
                match_expression,
            )?,
            // TODO: more operators! ENG-1248
            _ => Err(Error::UnknownMatchDocumentOperator(operator))?,
        }
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
        Bson::Array(_) => todo!(),
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
