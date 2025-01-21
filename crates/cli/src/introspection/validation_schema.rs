use std::collections::BTreeMap;

use configuration::{
    schema::{self, Type},
    Schema, WithName,
};
use futures_util::TryStreamExt;
use mongodb::bson::from_bson;
use mongodb_agent_common::{
    mongodb::DatabaseTrait,
    schema::{get_property_description, Property, ValidatorSchema},
};
use mongodb_support::BsonScalarType;

use mongodb_agent_common::interface_types::MongoAgentError;

type Collection = WithName<ndc_models::CollectionName, schema::Collection>;
type ObjectType = WithName<ndc_models::ObjectTypeName, schema::ObjectType>;
type ObjectField = WithName<ndc_models::FieldName, schema::ObjectField>;

pub async fn get_metadata_from_validation_schema(
    db: &impl DatabaseTrait,
) -> Result<BTreeMap<String, Schema>, MongoAgentError> {
    let mut collections_cursor = db.list_collections().await?;

    let mut schemas: Vec<WithName<String, Schema>> = vec![];

    while let Some(collection_spec) = collections_cursor.try_next().await? {
        let name = &collection_spec.name;
        let schema_bson_option = collection_spec
            .options
            .validator
            .as_ref()
            .and_then(|x| x.get("$jsonSchema"));

        if let Some(schema_bson) = schema_bson_option {
            let validator_schema =
                from_bson::<ValidatorSchema>(schema_bson.clone()).map_err(|err| {
                    MongoAgentError::BadCollectionSchema(Box::new((
                        name.to_owned(),
                        schema_bson.clone(),
                        err,
                    )))
                })?;
            let collection_schema = make_collection_schema(name, &validator_schema);
            schemas.push(collection_schema);
        }
    }

    Ok(WithName::into_map(schemas))
}

fn make_collection_schema(
    collection_name: &str,
    validator_schema: &ValidatorSchema,
) -> WithName<String, Schema> {
    let (object_types, collection) = make_collection(collection_name, validator_schema);
    WithName::named(
        collection.name.to_string(),
        Schema {
            collections: WithName::into_map(vec![collection]),
            object_types: WithName::into_map(object_types),
        },
    )
}

fn make_collection(
    collection_name: &str,
    validator_schema: &ValidatorSchema,
) -> (Vec<ObjectType>, Collection) {
    let properties = &validator_schema.properties;
    let required_labels = &validator_schema.required;

    let (mut object_type_defs, object_fields) = {
        let type_prefix = format!("{collection_name}_");
        let id_field = WithName::named(
            "_id".into(),
            schema::ObjectField {
                description: Some("primary key _id".to_string()),
                r#type: Type::Scalar(BsonScalarType::ObjectId),
            },
        );
        let (object_type_defs, mut object_fields): (Vec<Vec<ObjectType>>, Vec<ObjectField>) =
            properties
                .iter()
                .map(|prop| make_object_field(&type_prefix, required_labels, prop))
                .unzip();
        if !object_fields.iter().any(|info| info.name == "_id".into()) {
            // There should always be an _id field, so add it unless it was already specified in
            // the validator.
            object_fields.push(id_field);
        }
        (object_type_defs.concat(), object_fields)
    };

    let collection_type = WithName::named(
        collection_name.into(),
        schema::ObjectType {
            description: Some(format!("Object type for collection {collection_name}")),
            fields: WithName::into_map(object_fields),
        },
    );

    object_type_defs.push(collection_type);

    let collection_info = WithName::named(
        collection_name.into(),
        schema::Collection {
            description: validator_schema.description.clone(),
            r#type: collection_name.into(),
        },
    );

    (object_type_defs, collection_info)
}

fn make_object_field(
    type_prefix: &str,
    required_labels: &[String],
    (prop_name, prop_schema): (&String, &Property),
) -> (Vec<ObjectType>, ObjectField) {
    let description = get_property_description(prop_schema);

    let object_type_name = format!("{type_prefix}{prop_name}");
    let (collected_otds, field_type) = make_field_type(&object_type_name, prop_schema);

    let object_field = WithName::named(
        prop_name.to_owned().into(),
        schema::ObjectField {
            description,
            r#type: maybe_nullable(field_type, !required_labels.contains(prop_name)),
        },
    );

    (collected_otds, object_field)
}

fn maybe_nullable(
    t: configuration::schema::Type,
    is_nullable: bool,
) -> configuration::schema::Type {
    if is_nullable {
        configuration::schema::Type::Nullable(Box::new(t))
    } else {
        t
    }
}

fn make_field_type(object_type_name: &str, prop_schema: &Property) -> (Vec<ObjectType>, Type) {
    let mut collected_otds: Vec<ObjectType> = vec![];

    match prop_schema {
        Property::Object {
            properties: None, ..
        } => (vec![], Type::ExtendedJSON),
        Property::Object {
            description: _,
            required,
            properties: Some(properties),
        } => {
            let type_prefix = format!("{object_type_name}_");
            let (otds, otd_fields): (Vec<Vec<ObjectType>>, Vec<ObjectField>) = properties
                .iter()
                .map(|prop| make_object_field(&type_prefix, required, prop))
                .unzip();

            let object_type_definition = WithName::named(
                object_type_name.into(),
                schema::ObjectType {
                    description: Some("generated from MongoDB validation schema".to_string()),
                    fields: WithName::into_map(otd_fields),
                },
            );

            collected_otds.append(&mut otds.concat());
            collected_otds.push(object_type_definition);

            (collected_otds, Type::Object(object_type_name.to_string()))
        }
        Property::Array {
            description: _,
            items,
        } => {
            let item_schemas = *items.clone();

            let (mut otds, element_type) = make_field_type(object_type_name, &item_schemas);
            let field_type = Type::ArrayOf(Box::new(element_type));

            collected_otds.append(&mut otds);

            (collected_otds, field_type)
        }
        Property::Scalar {
            bson_type,
            description: _,
        } => (collected_otds, Type::Scalar(bson_type.to_owned())),
    }
}
