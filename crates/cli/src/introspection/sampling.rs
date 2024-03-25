use std::collections::BTreeMap;

use super::type_unification::{
    unify_object_types, unify_schema, unify_type, TypeUnificationContext, TypeUnificationResult,
};
use configuration::{
    schema::{self, Type},
    Schema, WithName,
};
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};
use mongodb_agent_common::interface_types::MongoConfig;
use mongodb_support::BsonScalarType::{self, *};

type ObjectField = WithName<schema::ObjectField>;
type ObjectType = WithName<schema::ObjectType>;

/// Sample from all collections in the database and return a Schema.
/// Return an error if there are any errors accessing the database
/// or if the types derived from the sample documents for a collection
/// are not unifiable.
pub async fn sample_schema_from_db(
    sample_size: u32,
    config: &MongoConfig,
) -> anyhow::Result<Schema> {
    let mut schema = Schema {
        collections: BTreeMap::new(),
        object_types: BTreeMap::new(),
    };
    let db = config.client.database(&config.database);
    let mut collections_cursor = db.list_collections(None, None).await?;

    while let Some(collection_spec) = collections_cursor.try_next().await? {
        let collection_name = collection_spec.name;
        let collection_schema =
            sample_schema_from_collection(&collection_name, sample_size, config).await?;
        schema = unify_schema(schema, collection_schema);
    }
    Ok(schema)
}

async fn sample_schema_from_collection(
    collection_name: &str,
    sample_size: u32,
    config: &MongoConfig,
) -> anyhow::Result<Schema> {
    let db = config.client.database(&config.database);
    let options = None;
    let mut cursor = db
        .collection::<Document>(collection_name)
        .aggregate(vec![doc! {"$sample": { "size": sample_size }}], options)
        .await?;
    let mut collected_object_types = vec![];
    while let Some(document) = cursor.try_next().await? {
        let object_types = make_object_type(collection_name, &document)?;
        collected_object_types = if collected_object_types.is_empty() {
            object_types
        } else {
            unify_object_types(collected_object_types, object_types)?
        };
    }
    let collection_info = WithName::named(
        collection_name.to_string(),
        schema::Collection {
            description: None,
            r#type: collection_name.to_string(),
        },
    );

    Ok(Schema {
        collections: WithName::into_map([collection_info]),
        object_types: WithName::into_map(collected_object_types),
    })
}

fn make_object_type(
    object_type_name: &str,
    document: &Document,
) -> TypeUnificationResult<Vec<ObjectType>> {
    let (mut object_type_defs, object_fields) = {
        let type_prefix = format!("{object_type_name}_");
        let (object_type_defs, object_fields): (Vec<Vec<ObjectType>>, Vec<ObjectField>) = document
            .iter()
            .map(|(field_name, field_value)| {
                make_object_field(&type_prefix, field_name, field_value)
            })
            .collect::<TypeUnificationResult<Vec<(Vec<ObjectType>, ObjectField)>>>()?
            .into_iter()
            .unzip();
        (object_type_defs.concat(), object_fields)
    };

    let object_type = WithName::named(
        object_type_name.to_string(),
        schema::ObjectType {
            description: None,
            fields: WithName::into_map(object_fields),
        },
    );

    object_type_defs.push(object_type);
    Ok(object_type_defs)
}

fn make_object_field(
    type_prefix: &str,
    field_name: &str,
    field_value: &Bson,
) -> TypeUnificationResult<(Vec<ObjectType>, ObjectField)> {
    let object_type_name = format!("{type_prefix}{field_name}");
    let (collected_otds, field_type) = make_field_type(&object_type_name, field_name, field_value)?;

    let object_field = WithName::named(
        field_name.to_owned(),
        schema::ObjectField {
            description: None,
            r#type: field_type,
        },
    );

    Ok((collected_otds, object_field))
}

fn make_field_type(
    object_type_name: &str,
    field_name: &str,
    field_value: &Bson,
) -> TypeUnificationResult<(Vec<ObjectType>, Type)> {
    fn scalar(t: BsonScalarType) -> TypeUnificationResult<(Vec<ObjectType>, Type)> {
        Ok((vec![], Type::Scalar(t)))
    }
    match field_value {
        Bson::Double(_) => scalar(Double),
        Bson::String(_) => scalar(String),
        Bson::Array(arr) => {
            // Examine all elements of the array and take the union of the resulting types.
            let mut collected_otds = vec![];
            let mut result_type = Type::Scalar(Undefined);
            for elem in arr {
                let (elem_collected_otds, elem_type) =
                    make_field_type(object_type_name, field_name, elem)?;
                collected_otds = if collected_otds.is_empty() {
                    elem_collected_otds
                } else {
                    unify_object_types(collected_otds, elem_collected_otds)?
                };
                let context = TypeUnificationContext::new(object_type_name, field_name);
                result_type = unify_type(context, result_type, elem_type)?;
            }
            Ok((collected_otds, Type::ArrayOf(Box::new(result_type))))
        }
        Bson::Document(document) => {
            let collected_otds = make_object_type(object_type_name, document)?;
            Ok((collected_otds, Type::Object(object_type_name.to_owned())))
        }
        Bson::Boolean(_) => scalar(Bool),
        Bson::Null => scalar(Null),
        Bson::RegularExpression(_) => scalar(Regex),
        Bson::JavaScriptCode(_) => scalar(Javascript),
        Bson::JavaScriptCodeWithScope(_) => scalar(JavascriptWithScope),
        Bson::Int32(_) => scalar(Int),
        Bson::Int64(_) => scalar(Long),
        Bson::Timestamp(_) => scalar(Timestamp),
        Bson::Binary(_) => scalar(BinData),
        Bson::ObjectId(_) => scalar(ObjectId),
        Bson::DateTime(_) => scalar(Date),
        Bson::Symbol(_) => scalar(Symbol),
        Bson::Decimal128(_) => scalar(Decimal),
        Bson::Undefined => scalar(Undefined),
        Bson::MaxKey => scalar(MaxKey),
        Bson::MinKey => scalar(MinKey),
        Bson::DbPointer(_) => scalar(DbPointer),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use configuration::{
        schema::{ObjectField, ObjectType, Type},
        WithName,
    };
    use mongodb::bson::doc;
    use mongodb_support::BsonScalarType;

    use crate::introspection::type_unification::{TypeUnificationContext, TypeUnificationError};

    use super::make_object_type;

    #[test]
    fn simple_doc() -> Result<(), anyhow::Error> {
        let object_name = "foo";
        let doc = doc! {"my_int": 1, "my_string": "two"};
        let result = make_object_type(object_name, &doc).map(WithName::into_map::<BTreeMap<_, _>>);

        let expected = Ok(BTreeMap::from([(
            object_name.to_owned(),
            ObjectType {
                fields: BTreeMap::from([
                    (
                        "my_int".to_owned(),
                        ObjectField {
                            r#type: Type::Scalar(BsonScalarType::Int),
                            description: None,
                        },
                    ),
                    (
                        "my_string".to_owned(),
                        ObjectField {
                            r#type: Type::Scalar(BsonScalarType::String),
                            description: None,
                        },
                    ),
                ]),
                description: None,
            },
        )]));

        assert_eq!(expected, result);

        Ok(())
    }

    #[test]
    fn array_of_objects() -> Result<(), anyhow::Error> {
        let object_name = "foo";
        let doc = doc! {"my_array": [{"foo": 42, "bar": ""}, {"bar": "wut", "baz": 3.77}]};
        let result = make_object_type(object_name, &doc).map(WithName::into_map::<BTreeMap<_, _>>);

        let expected = Ok(BTreeMap::from([
            (
                "foo_my_array".to_owned(),
                ObjectType {
                    fields: BTreeMap::from([
                        (
                            "foo".to_owned(),
                            ObjectField {
                                r#type: Type::Nullable(Box::new(Type::Scalar(BsonScalarType::Int))),
                                description: None,
                            },
                        ),
                        (
                            "bar".to_owned(),
                            ObjectField {
                                r#type: Type::Scalar(BsonScalarType::String),
                                description: None,
                            },
                        ),
                        (
                            "baz".to_owned(),
                            ObjectField {
                                r#type: Type::Nullable(Box::new(Type::Scalar(
                                    BsonScalarType::Double,
                                ))),
                                description: None,
                            },
                        ),
                    ]),
                    description: None,
                },
            ),
            (
                object_name.to_owned(),
                ObjectType {
                    fields: BTreeMap::from([(
                        "my_array".to_owned(),
                        ObjectField {
                            r#type: Type::ArrayOf(Box::new(Type::Object(
                                "foo_my_array".to_owned(),
                            ))),
                            description: None,
                        },
                    )]),
                    description: None,
                },
            ),
        ]));

        assert_eq!(expected, result);

        Ok(())
    }

    #[test]
    fn non_unifiable_array_of_objects() -> Result<(), anyhow::Error> {
        let object_name = "foo";
        let doc = doc! {"my_array": [{"foo": 42, "bar": ""}, {"bar": 17, "baz": 3.77}]};
        let result = make_object_type(object_name, &doc);

        let expected = Err(TypeUnificationError::ScalarType(
            TypeUnificationContext::new("foo_my_array", "bar"),
            BsonScalarType::String,
            BsonScalarType::Int,
        ));
        assert_eq!(expected, result);

        Ok(())
    }
}
