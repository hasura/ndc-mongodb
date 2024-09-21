use std::collections::BTreeMap;

use configuration::schema::{ObjectField, ObjectType, Type};
use mongodb::bson::{Bson, Document};
use ndc_models::ObjectTypeName;

use super::pipeline_type_context::PipelineTypeContext;

use super::error::Result;

pub fn infer_type_from_replacement_doc(
    mut context: PipelineTypeContext<'_>,
    object_type_name: ObjectTypeName,
    replacement_doc: Document,
) -> Result<PipelineTypeContext<'_>> {
    let fields = replacement_doc
        .into_iter()
        .map(|(field_name, bson)| {
            let field_object_type_name = format!("{object_type_name}_{field_name}").into();
            let object_field_type =
                infer_type_from_aggregation_expression(&mut context, field_object_type_name, bson)?;
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
    context.insert_object_type(object_type_name, object_type);
    Ok(context)
}

fn infer_type_from_aggregation_expression(
    context: &mut PipelineTypeContext<'_>,
    type_name: ObjectTypeName,
    bson: Bson,
) -> Result<Type> {
    match bson {
        Bson::Double(_) => todo!(),
        Bson::String(string) => infer_type_from_reference_shorthand(context, type_name, string),
        Bson::Array(_) => todo!(),
        Bson::Document(_) => todo!(),
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
    }
}

fn infer_type_from_reference_shorthand<'a>(
    context: &mut PipelineTypeContext<'a>,
    type_name: ObjectTypeName,
    input: String,
) -> Result<Type> {
    todo!()
}
