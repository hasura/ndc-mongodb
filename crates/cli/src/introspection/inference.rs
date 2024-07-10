use std::collections::BTreeMap;

use super::type_unification::{make_nullable_field, unify_object_types, unify_type};
use configuration::{
    schema::{self, Type},
    WithName,
};
use mongodb::bson::{Bson, Document};
use mongodb_support::BsonScalarType::{self, *};

type ObjectField = WithName<schema::ObjectField>;
type ObjectType = WithName<schema::ObjectType>;

pub fn make_object_type_for_collection(
    object_type_name: &str,
    document: &Document,
    all_schema_nullable: bool,
) -> Vec<ObjectType> {
    make_object_type_helper(object_type_name, document, true, all_schema_nullable)
}

fn make_object_type_helper(
    object_type_name: &str,
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
        object_type_name.to_string(),
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
        field_name.to_owned(),
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
) -> (BTreeMap<std::string::String, schema::ObjectType>, Type) {
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
        Bson::Double(_) => scalar(Double),
        Bson::String(_) => scalar(String),
        Bson::Array(arr) => {
            // Examine all elements of the array and take the union of the resulting types.
            let mut collected_otds = vec![];
            let mut result_type = Type::Scalar(Undefined);
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
            let collected_otds = make_object_type_helper(
                object_type_name,
                document,
                is_collection_type,
                all_schema_nullable,
            );
            (collected_otds, Type::Object(object_type_name.to_owned()))
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

    use super::make_object_type_helper;

    #[test]
    fn simple_doc() -> Result<(), anyhow::Error> {
        let object_name = "foo";
        let doc = doc! {"my_int": 1, "my_string": "two"};
        let result =
            WithName::into_map::<BTreeMap<_, _>>(make_object_type_helper(object_name, &doc, false, false));

        let expected = BTreeMap::from([(
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
        )]);

        assert_eq!(expected, result);

        Ok(())
    }

    #[test]
    fn simple_doc_nullable_fields() -> Result<(), anyhow::Error> {
        let object_name = "foo";
        let doc = doc! {"my_int": 1, "my_string": "two", "_id": 0};
        let result =
            WithName::into_map::<BTreeMap<_, _>>(make_object_type_helper(object_name, &doc, true, true));

        let expected = BTreeMap::from([(
            object_name.to_owned(),
            ObjectType {
                fields: BTreeMap::from([
                    (
                        "_id".to_owned(),
                        ObjectField {
                            r#type: Type::Scalar(BsonScalarType::Int),
                            description: None,
                        },
                    ),
                    (
                        "my_int".to_owned(),
                        ObjectField {
                            r#type: Type::Nullable(Box::new(Type::Scalar(BsonScalarType::Int))),
                            description: None,
                        },
                    ),
                    (
                        "my_string".to_owned(),
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
        let object_name = "foo";
        let doc = doc! {"my_array": [{"foo": 42, "bar": ""}, {"bar": "wut", "baz": 3.77}]};
        let result =
            WithName::into_map::<BTreeMap<_, _>>(make_object_type_helper(object_name, &doc, false, false));

        let expected = BTreeMap::from([
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
        ]);

        assert_eq!(expected, result);

        Ok(())
    }

    #[test]
    fn non_unifiable_array_of_objects() -> Result<(), anyhow::Error> {
        let object_name = "foo";
        let doc = doc! {"my_array": [{"foo": 42, "bar": ""}, {"bar": 17, "baz": 3.77}]};
        let result =
            WithName::into_map::<BTreeMap<_, _>>(make_object_type_helper(object_name, &doc, false, false));

        let expected = BTreeMap::from([
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
                                r#type: Type::ExtendedJSON,
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
        ]);

        assert_eq!(expected, result);

        Ok(())
    }
}