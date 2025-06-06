mod keep_backward_compatible_changes;

use std::collections::BTreeMap;

use crate::log_warning;

use super::type_unification::{make_nullable_field, unify_object_types, unify_type};
use configuration::{
    schema::{self, Collection, CollectionSchema, ObjectTypes, Type},
    Schema, WithName,
};
use futures_util::TryStreamExt;
use json_structural_diff::JsonDiff;
use mongodb::bson::{doc, spec::BinarySubtype, Binary, Bson, Document};
use mongodb_agent_common::mongodb::{CollectionTrait as _, DatabaseTrait};
use mongodb_support::{
    aggregate::{Pipeline, Stage},
    BsonScalarType::{self, self as S},
};
use ndc_models::{CollectionName, ObjectTypeName};

use self::keep_backward_compatible_changes::keep_backward_compatible_changes;

type ObjectField = WithName<ndc_models::FieldName, schema::ObjectField>;
type ObjectType = WithName<ndc_models::ObjectTypeName, schema::ObjectType>;

#[derive(Default)]
pub struct SampledSchema {
    pub schemas: BTreeMap<String, Schema>,

    /// Updates to existing schema changes are made conservatively. These diffs show the difference
    /// between each new configuration to be written to disk on the left, and the schema that would
    /// have been written if starting from scratch on the right.
    pub ignored_changes: BTreeMap<String, String>,
}

impl SampledSchema {
    pub fn insert_collection(
        &mut self,
        name: impl std::fmt::Display,
        collection: CollectionSchema,
    ) {
        self.schemas.insert(
            name.to_string(),
            Self::schema_from_collection(name, collection),
        );
    }

    pub fn record_ignored_collection_changes(
        &mut self,
        name: impl std::fmt::Display,
        before: &CollectionSchema,
        after: &CollectionSchema,
    ) -> Result<(), serde_json::error::Error> {
        let a = serde_json::to_value(Self::schema_from_collection(&name, before.clone()))?;
        let b = serde_json::to_value(Self::schema_from_collection(&name, after.clone()))?;
        if let Some(diff) = JsonDiff::diff_string(&a, &b, false) {
            self.ignored_changes.insert(name.to_string(), diff);
        }
        Ok(())
    }

    fn schema_from_collection(
        name: impl std::fmt::Display,
        collection: CollectionSchema,
    ) -> Schema {
        Schema {
            collections: [(name.to_string().into(), collection.collection)].into(),
            object_types: collection.object_types,
        }
    }
}

/// Sample from all collections in the database and return a Schema.
/// Return an error if there are any errors accessing the database
/// or if the types derived from the sample documents for a collection
/// are not unifiable.
pub async fn sample_schema_from_db(
    sample_size: u32,
    all_schema_nullable: bool,
    db: &impl DatabaseTrait,
    mut previously_defined_collections: BTreeMap<CollectionName, CollectionSchema>,
) -> anyhow::Result<SampledSchema> {
    let mut sampled_schema: SampledSchema = Default::default();
    let mut collections_cursor = db.list_collections().await?;

    while let Some(collection_spec) = collections_cursor.try_next().await? {
        let collection_name = collection_spec.name;

        // The `system.*` namespace is reserved for internal use. In some deployments, such as
        // MongoDB v6 running on Atlas, aggregate permissions are denied for `system.views` which
        // causes introspection to fail. So we skip those collections.
        if collection_name.starts_with("system.") {
            log_warning!("collection {collection_name} is under the system namespace which is reserved for internal use - skipping");
            continue;
        }

        let previously_defined_collection =
            previously_defined_collections.remove(collection_name.as_str());

        // Use previously-defined type name in case user has customized it
        let collection_type_name = previously_defined_collection
            .as_ref()
            .map(|c| c.collection.r#type.clone())
            .unwrap_or_else(|| collection_name.clone().into());

        let sample_result = match sample_schema_from_collection(
            &collection_name,
            collection_type_name.clone(),
            sample_size,
            all_schema_nullable,
            db,
        )
        .await
        {
            Ok(schema) => schema,
            Err(err) => {
                let indented_error = indent::indent_all_by(2, err.to_string());
                log_warning!(
                "an error occurred attempting to sample collection, {collection_name} - skipping\n{indented_error}"
            );
                continue;
            }
        };

        let Some(collection_schema) = sample_result else {
            log_warning!("could not find any documents to sample from collection, {collection_name} - skipping");
            continue;
        };

        let collection_schema = match previously_defined_collection {
            Some(previously_defined_collection) => {
                let backward_compatible_schema = keep_backward_compatible_changes(
                    previously_defined_collection,
                    collection_schema.object_types.clone(),
                );
                let _ = sampled_schema.record_ignored_collection_changes(
                    &collection_name,
                    &backward_compatible_schema,
                    &collection_schema,
                );
                let updated_collection = Collection {
                    r#type: collection_type_name,
                    description: collection_schema
                        .collection
                        .description
                        .or(backward_compatible_schema.collection.description),
                };
                CollectionSchema {
                    collection: updated_collection,
                    object_types: backward_compatible_schema.object_types,
                }
            }
            None => collection_schema,
        };

        sampled_schema.insert_collection(collection_name, collection_schema);
    }

    Ok(sampled_schema)
}

async fn sample_schema_from_collection(
    collection_name: &str,
    collection_type_name: ObjectTypeName,
    sample_size: u32,
    all_schema_nullable: bool,
    db: &impl DatabaseTrait,
) -> anyhow::Result<Option<CollectionSchema>> {
    let options = None;
    let mut cursor = db
        .collection(collection_name)
        .aggregate(
            Pipeline::new(vec![Stage::Other(doc! {
                "$sample": { "size": sample_size }
            })]),
            options,
        )
        .await?;
    let mut collected_object_types = vec![];
    let is_collection_type = true;
    while let Some(document) = cursor.try_next().await? {
        let object_types = make_object_type(
            &collection_type_name,
            &document,
            is_collection_type,
            all_schema_nullable,
        );
        collected_object_types = if collected_object_types.is_empty() {
            object_types
        } else {
            unify_object_types(collected_object_types, object_types)
        };
    }
    if collected_object_types.is_empty() {
        Ok(None)
    } else {
        let collection_info = schema::Collection {
            description: None,
            r#type: collection_type_name,
        };
        Ok(Some(CollectionSchema {
            collection: collection_info,
            object_types: WithName::into_map(collected_object_types),
        }))
    }
}

pub fn make_object_type(
    object_type_name: &ndc_models::ObjectTypeName,
    document: &Document,
    is_collection_type: bool,
    all_schema_nullable: bool,
) -> Vec<ObjectType> {
    let (mut object_type_defs, object_fields) = {
        let type_prefix = format!("{object_type_name}_");
        let (object_type_defs, object_fields): (Vec<Vec<ObjectType>>, Vec<ObjectField>) = document
            .iter()
            .map(|(field_name, field_value)| {
                make_object_field(
                    &type_prefix,
                    field_name,
                    field_value,
                    is_collection_type,
                    all_schema_nullable,
                )
            })
            .unzip();
        (object_type_defs.concat(), object_fields)
    };

    let object_type = WithName::named(
        object_type_name.to_owned(),
        schema::ObjectType {
            description: None,
            fields: WithName::into_map(object_fields),
        },
    );

    object_type_defs.push(object_type);
    object_type_defs
}

fn make_object_field(
    type_prefix: &str,
    field_name: &str,
    field_value: &Bson,
    is_collection_type: bool,
    all_schema_nullable: bool,
) -> (Vec<ObjectType>, ObjectField) {
    let object_type_name = format!("{type_prefix}{field_name}");
    let (collected_otds, field_type) =
        make_field_type(&object_type_name, field_value, all_schema_nullable);
    let object_field_value = WithName::named(
        field_name.into(),
        schema::ObjectField {
            description: None,
            r#type: field_type,
        },
    );
    let object_field = if all_schema_nullable && !(is_collection_type && field_name == "_id") {
        // The _id field on a collection type should never be nullable.
        make_nullable_field(object_field_value)
    } else {
        object_field_value
    };

    (collected_otds, object_field)
}

// Exported for use in tests
pub fn type_from_bson(
    object_type_name: &str,
    value: &Bson,
    all_schema_nullable: bool,
) -> (
    BTreeMap<ndc_models::ObjectTypeName, schema::ObjectType>,
    Type,
) {
    let (object_types, t) = make_field_type(object_type_name, value, all_schema_nullable);
    (WithName::into_map(object_types), t)
}

fn make_field_type(
    object_type_name: &str,
    field_value: &Bson,
    all_schema_nullable: bool,
) -> (Vec<ObjectType>, Type) {
    fn scalar(t: BsonScalarType) -> (Vec<ObjectType>, Type) {
        (vec![], Type::Scalar(t))
    }
    match field_value {
        Bson::Double(_) => scalar(S::Double),
        Bson::String(_) => scalar(S::String),
        Bson::Array(arr) => {
            // Examine all elements of the array and take the union of the resulting types.
            let mut collected_otds = vec![];
            let mut result_type = Type::Scalar(S::Undefined);
            for elem in arr {
                let (elem_collected_otds, elem_type) =
                    make_field_type(object_type_name, elem, all_schema_nullable);
                collected_otds = if collected_otds.is_empty() {
                    elem_collected_otds
                } else {
                    unify_object_types(collected_otds, elem_collected_otds)
                };
                result_type = unify_type(result_type, elem_type);
            }
            (collected_otds, Type::ArrayOf(Box::new(result_type)))
        }
        Bson::Document(document) => {
            let is_collection_type = false;
            let collected_otds = make_object_type(
                &object_type_name.into(),
                document,
                is_collection_type,
                all_schema_nullable,
            );
            (collected_otds, Type::Object(object_type_name.to_owned()))
        }
        Bson::Boolean(_) => scalar(S::Bool),
        Bson::Null => scalar(S::Null),
        Bson::RegularExpression(_) => scalar(S::Regex),
        Bson::JavaScriptCode(_) => scalar(S::Javascript),
        Bson::JavaScriptCodeWithScope(_) => scalar(S::JavascriptWithScope),
        Bson::Int32(_) => scalar(S::Int),
        Bson::Int64(_) => scalar(S::Long),
        Bson::Timestamp(_) => scalar(S::Timestamp),
        Bson::Binary(Binary { subtype, .. }) => {
            if *subtype == BinarySubtype::Uuid {
                scalar(S::UUID)
            } else {
                scalar(S::BinData)
            }
        }
        Bson::ObjectId(_) => scalar(S::ObjectId),
        Bson::DateTime(_) => scalar(S::Date),
        Bson::Symbol(_) => scalar(S::Symbol),
        Bson::Decimal128(_) => scalar(S::Decimal),
        Bson::Undefined => scalar(S::Undefined),
        Bson::MaxKey => scalar(S::MaxKey),
        Bson::MinKey => scalar(S::MinKey),
        Bson::DbPointer(_) => scalar(S::DbPointer),
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

    use super::make_object_type;

    #[test]
    fn simple_doc() -> Result<(), anyhow::Error> {
        let object_name = "foo".into();
        let doc = doc! {"my_int": 1, "my_string": "two"};
        let result = WithName::into_map::<BTreeMap<_, _>>(make_object_type(
            &object_name,
            &doc,
            false,
            false,
        ));

        let expected = BTreeMap::from([(
            object_name.to_owned(),
            ObjectType {
                fields: BTreeMap::from([
                    (
                        "my_int".into(),
                        ObjectField {
                            r#type: Type::Scalar(BsonScalarType::Int),
                            description: None,
                        },
                    ),
                    (
                        "my_string".into(),
                        ObjectField {
                            r#type: Type::Scalar(BsonScalarType::String),
                            description: None,
                        },
                    ),
                ]),
                description: None,
            },
        )]);

        assert_eq!(expected, result);

        Ok(())
    }

    #[test]
    fn simple_doc_nullable_fields() -> Result<(), anyhow::Error> {
        let object_name = "foo".into();
        let doc = doc! {"my_int": 1, "my_string": "two", "_id": 0};
        let result =
            WithName::into_map::<BTreeMap<_, _>>(make_object_type(&object_name, &doc, true, true));

        let expected = BTreeMap::from([(
            object_name.to_owned(),
            ObjectType {
                fields: BTreeMap::from([
                    (
                        "_id".into(),
                        ObjectField {
                            r#type: Type::Scalar(BsonScalarType::Int),
                            description: None,
                        },
                    ),
                    (
                        "my_int".into(),
                        ObjectField {
                            r#type: Type::Nullable(Box::new(Type::Scalar(BsonScalarType::Int))),
                            description: None,
                        },
                    ),
                    (
                        "my_string".into(),
                        ObjectField {
                            r#type: Type::Nullable(Box::new(Type::Scalar(BsonScalarType::String))),
                            description: None,
                        },
                    ),
                ]),
                description: None,
            },
        )]);

        assert_eq!(expected, result);

        Ok(())
    }

    #[test]
    fn array_of_objects() -> Result<(), anyhow::Error> {
        let object_name = "foo".into();
        let doc = doc! {"my_array": [{"foo": 42, "bar": ""}, {"bar": "wut", "baz": 3.77}]};
        let result = WithName::into_map::<BTreeMap<_, _>>(make_object_type(
            &object_name,
            &doc,
            false,
            false,
        ));

        let expected = BTreeMap::from([
            (
                "foo_my_array".into(),
                ObjectType {
                    fields: BTreeMap::from([
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
                                r#type: Type::Scalar(BsonScalarType::String),
                                description: None,
                            },
                        ),
                        (
                            "baz".into(),
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
                        "my_array".into(),
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
        ]);

        assert_eq!(expected, result);

        Ok(())
    }

    #[test]
    fn non_unifiable_array_of_objects() -> Result<(), anyhow::Error> {
        let object_name = "foo".into();
        let doc = doc! {"my_array": [{"foo": 42, "bar": ""}, {"bar": 17, "baz": 3.77}]};
        let result = WithName::into_map::<BTreeMap<_, _>>(make_object_type(
            &object_name,
            &doc,
            false,
            false,
        ));

        let expected = BTreeMap::from([
            (
                "foo_my_array".into(),
                ObjectType {
                    fields: BTreeMap::from([
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
                                r#type: Type::ExtendedJSON,
                                description: None,
                            },
                        ),
                        (
                            "baz".into(),
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
                        "my_array".into(),
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
        ]);

        assert_eq!(expected, result);

        Ok(())
    }
}
