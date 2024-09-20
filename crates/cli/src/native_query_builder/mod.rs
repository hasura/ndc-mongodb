pub mod error;
mod helpers;
mod infer_result_type;

use configuration::{
    native_query::NativeQueryRepresentation::Collection, serialized::NativeQuery, Configuration,
};
use mongodb_support::aggregate::Pipeline;
use ndc_models::{CollectionName, ObjectTypeName};

use self::error::Result;
use self::infer_result_type::{infer_result_type, Constraint};

pub fn native_query_from_pipeline(
    configuration: &Configuration,
    name: &str,
    input_collection: Option<CollectionName>,
    pipeline: Pipeline,
) -> Result<NativeQuery> {
    let pipeline_types =
        infer_result_type(configuration, name, input_collection.as_ref(), &pipeline)?;
    for warning in pipeline_types.warnings() {
        println!("warning: {warning}");
    }
    let result_document_type = match pipeline_types.result_doc_type()? {
        Constraint::Type(t) => t,
        Constraint::InsufficientContext => todo!("could not infer result type"),
    };
    Ok(NativeQuery {
        representation: Collection,
        input_collection,
        arguments: Default::default(), // TODO: infer arguments
        result_document_type,
        object_types: pipeline_types.object_types(),
        pipeline: pipeline.into(),
        description: None,
    })
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use configuration::{
        native_query::NativeQueryRepresentation::Collection,
        read_directory,
        schema::{ObjectField, ObjectType, Type},
        serialized::NativeQuery,
        Configuration,
    };
    use mongodb::bson::doc;
    use mongodb_support::{
        aggregate::{Pipeline, Stage},
        BsonScalarType,
    };
    use ndc_models::ObjectTypeName;
    use pretty_assertions::assert_eq;

    use super::native_query_from_pipeline;

    #[tokio::test]
    async fn infers_native_query_from_pipeline() -> Result<()> {
        let config = read_configuration().await?;
        let pipeline = Pipeline::new(vec![Stage::Documents(vec![
            doc! { "foo": 1 },
            doc! { "bar": 2 },
        ])]);
        let native_query = native_query_from_pipeline(
            &config,
            "selected_title",
            Some("movies".into()),
            pipeline.clone(),
        )?;

        let expected_document_type_name: ObjectTypeName = "selected_title".into();

        let expected_object_types = [(
            expected_document_type_name.clone(),
            ObjectType {
                fields: [
                    (
                        "foo".into(),
                        ObjectField {
                            r#type: Type::Nullable(Box::new(Type::Scalar(BsonScalarType::Int))),
                            description: None,
                        },
                    ),
                    (
                        "bar".into(),
                        ObjectField {
                            r#type: Type::Nullable(Box::new(Type::Scalar(BsonScalarType::Int))),
                            description: None,
                        },
                    ),
                ]
                .into(),
                description: None,
            },
        )]
        .into();

        let expected = NativeQuery {
            representation: Collection,
            input_collection: Some("movies".into()),
            arguments: Default::default(),
            result_document_type: expected_document_type_name,
            object_types: expected_object_types,
            pipeline: pipeline.into(),
            description: None,
        };

        assert_eq!(native_query, expected);
        Ok(())
    }

    async fn read_configuration() -> Result<Configuration> {
        read_directory("../../fixtures/hasura/sample_mflix/connector").await
    }
}
