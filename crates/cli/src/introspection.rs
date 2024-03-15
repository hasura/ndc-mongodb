use configuration::{
    metadata::{Collection, ObjectField, ObjectType, Type},
    Metadata,
};
use futures_util::{StreamExt, TryStreamExt};
use indexmap::IndexMap;
use mongodb::bson::from_bson;
use mongodb_agent_common::schema::{get_property_description, Property, ValidatorSchema};
use mongodb_support::{BsonScalarType, BsonType};

use mongodb_agent_common::interface_types::{MongoAgentError, MongoConfig};

pub async fn get_metadata_from_validation_schema(
    config: &MongoConfig,
) -> Result<Metadata, MongoAgentError> {
    let db = config.client.database(&config.database);
    let collections_cursor = db.list_collections(None, None).await?;

    let (object_types, collections) = collections_cursor
        .into_stream()
        .map(
            |collection_spec| -> Result<(Vec<ObjectType>, Collection), MongoAgentError> {
                let collection_spec_value = collection_spec?;
                let name = &collection_spec_value.name;
                let schema_bson_option = collection_spec_value
                    .options
                    .validator
                    .as_ref()
                    .and_then(|x| x.get("$jsonSchema"));

                match schema_bson_option {
                    Some(schema_bson) => {
                        from_bson::<ValidatorSchema>(schema_bson.clone()).map_err(|err| {
                            MongoAgentError::BadCollectionSchema(
                                name.to_owned(),
                                schema_bson.clone(),
                                err,
                            )
                        })
                    }
                    None => Ok(ValidatorSchema {
                        bson_type: BsonType::Object,
                        description: None,
                        required: Vec::new(),
                        properties: IndexMap::new(),
                    }),
                }
                .map(|validator_schema| make_collection(name, &validator_schema))
            },
        )
        .try_collect::<(Vec<Vec<ObjectType>>, Vec<Collection>)>()
        .await?;

    Ok(Metadata {
        collections,
        object_types: object_types.concat(),
    })
}

fn make_collection(
    collection_name: &str,
    validator_schema: &ValidatorSchema,
) -> (Vec<ObjectType>, Collection) {
    let properties = &validator_schema.properties;
    let required_labels = &validator_schema.required;

    let (mut object_type_defs, object_fields) = {
        let type_prefix = format!("{collection_name}_");
        let id_field = ObjectField {
            name: "_id".to_string(),
            description: Some("primary key _id".to_string()),
            r#type: Type::Scalar(BsonScalarType::ObjectId),
        };
        let (object_type_defs, mut object_fields): (Vec<Vec<ObjectType>>, Vec<ObjectField>) =
            properties
                .iter()
                .map(|prop| make_object_field(&type_prefix, required_labels, prop))
                .unzip();
        if !object_fields.iter().any(|info| info.name == "_id") {
            // There should always be an _id field, so add it unless it was already specified in
            // the validator.
            object_fields.push(id_field);
        }
        (object_type_defs.concat(), object_fields)
    };

    let collection_type = ObjectType {
        name: collection_name.to_string(),
        description: Some(format!("Object type for collection {collection_name}")),
        fields: object_fields,
    };

    object_type_defs.push(collection_type);

    let collection_info = Collection {
        name: collection_name.to_string(),
        description: validator_schema.description.clone(),
        r#type: collection_name.to_string(),
    };

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

    let object_field = ObjectField {
        name: prop_name.clone(),
        description,
        r#type: maybe_nullable(field_type, !required_labels.contains(prop_name)),
    };

    (collected_otds, object_field)
}

fn maybe_nullable(
    t: configuration::metadata::Type,
    is_nullable: bool,
) -> configuration::metadata::Type {
    if is_nullable {
        configuration::metadata::Type::Nullable(Box::new(t))
    } else {
        t
    }
}

fn make_field_type(object_type_name: &str, prop_schema: &Property) -> (Vec<ObjectType>, Type) {
    let mut collected_otds: Vec<ObjectType> = vec![];

    match prop_schema {
        Property::Object {
            bson_type: _,
            description: _,
            required,
            properties,
        } => {
            let type_prefix = format!("{object_type_name}_");
            let (otds, otd_fields): (Vec<Vec<ObjectType>>, Vec<ObjectField>) = properties
                .iter()
                .map(|prop| make_object_field(&type_prefix, required, prop))
                .unzip();

            let object_type_definition = ObjectType {
                name: object_type_name.to_string(),
                description: Some("generated from MongoDB validation schema".to_string()),
                fields: otd_fields,
            };

            collected_otds.append(&mut otds.concat());
            collected_otds.push(object_type_definition);

            (collected_otds, Type::Object(object_type_name.to_string()))
        }
        Property::Array {
            bson_type: _,
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
