use std::{borrow::Cow, collections::BTreeMap};

use configuration::schema;
use mongodb_support::BsonScalarType;
use ndc_sdk::models::{
    AggregateFunctionDefinition, CollectionInfo, ComparisonOperatorDefinition, ScalarType, Type,
    TypeRepresentation, UniquenessConstraint,
};

use crate::api_type_conversions::QueryContext;

pub fn make_scalar_types() -> BTreeMap<String, ScalarType> {
    BTreeMap::from([
        (
            "String".to_owned(),
            ScalarType {
                representation: Some(TypeRepresentation::String),
                aggregate_functions: Default::default(),
                comparison_operators: BTreeMap::from([
                    ("_eq".to_owned(), ComparisonOperatorDefinition::Equal),
                    (
                        "_regex".to_owned(),
                        ComparisonOperatorDefinition::Custom {
                            argument_type: Type::Named {
                                name: "String".to_owned(),
                            },
                        },
                    ),
                ]),
            },
        ),
        (
            "Int".to_owned(),
            ScalarType {
                representation: Some(TypeRepresentation::Int32),
                aggregate_functions: BTreeMap::from([(
                    "avg".into(),
                    AggregateFunctionDefinition {
                        result_type: Type::Named {
                            name: "Float".into(), // Different result type to the input scalar type
                        },
                    },
                )]),
                comparison_operators: BTreeMap::from([(
                    "_eq".to_owned(),
                    ComparisonOperatorDefinition::Equal,
                )]),
            },
        ),
    ])
}

pub fn make_flat_schema() -> QueryContext<'static> {
    QueryContext {
        collections: Cow::Owned(BTreeMap::from([
            (
                "authors".into(),
                CollectionInfo {
                    name: "authors".to_owned(),
                    description: None,
                    collection_type: "Author".into(),
                    arguments: Default::default(),
                    uniqueness_constraints: make_primary_key_uniqueness_constraint("authors"),
                    foreign_keys: Default::default(),
                },
            ),
            (
                "articles".into(),
                CollectionInfo {
                    name: "articles".to_owned(),
                    description: None,
                    collection_type: "Article".into(),
                    arguments: Default::default(),
                    uniqueness_constraints: make_primary_key_uniqueness_constraint("articles"),
                    foreign_keys: Default::default(),
                },
            ),
        ])),
        functions: Default::default(),
        object_types: Cow::Owned(BTreeMap::from([
            (
                "Author".into(),
                schema::ObjectType {
                    description: None,
                    fields: BTreeMap::from([
                        (
                            "id".into(),
                            schema::ObjectField {
                                description: None,
                                r#type: schema::Type::Scalar(BsonScalarType::Int),
                            },
                        ),
                        (
                            "last_name".into(),
                            schema::ObjectField {
                                description: None,
                                r#type: schema::Type::Scalar(BsonScalarType::String),
                            },
                        ),
                    ]),
                },
            ),
            (
                "Article".into(),
                schema::ObjectType {
                    description: None,
                    fields: BTreeMap::from([
                        (
                            "author_id".into(),
                            schema::ObjectField {
                                description: None,
                                r#type: schema::Type::Scalar(BsonScalarType::Int),
                            },
                        ),
                        (
                            "title".into(),
                            schema::ObjectField {
                                description: None,
                                r#type: schema::Type::Scalar(BsonScalarType::String),
                            },
                        ),
                        (
                            "year".into(),
                            schema::ObjectField {
                                description: None,
                                r#type: schema::Type::Nullable(Box::new(schema::Type::Scalar(
                                    BsonScalarType::Int,
                                ))),
                            },
                        ),
                    ]),
                },
            ),
        ])),
        scalar_types: Cow::Owned(make_scalar_types()),
    }
}

pub fn make_nested_schema() -> QueryContext<'static> {
    QueryContext {
        collections: Cow::Owned(BTreeMap::from([(
            "authors".into(),
            CollectionInfo {
                name: "authors".into(),
                description: None,
                collection_type: "Author".into(),
                arguments: Default::default(),
                uniqueness_constraints: make_primary_key_uniqueness_constraint("authors"),
                foreign_keys: Default::default(),
            },
        )])),
        functions: Default::default(),
        object_types: Cow::Owned(BTreeMap::from([
            (
                "Author".into(),
                schema::ObjectType {
                    description: None,
                    fields: BTreeMap::from([
                        (
                            "address".into(),
                            schema::ObjectField {
                                description: None,
                                r#type: schema::Type::Object("Address".into()),
                            },
                        ),
                        (
                            "articles".into(),
                            schema::ObjectField {
                                description: None,
                                r#type: schema::Type::ArrayOf(Box::new(schema::Type::Object(
                                    "Article".into(),
                                ))),
                            },
                        ),
                        (
                            "array_of_arrays".into(),
                            schema::ObjectField {
                                description: None,
                                r#type: schema::Type::ArrayOf(Box::new(schema::Type::ArrayOf(
                                    Box::new(schema::Type::Object("Article".into())),
                                ))),
                            },
                        ),
                    ]),
                },
            ),
            (
                "Address".into(),
                schema::ObjectType {
                    description: None,
                    fields: BTreeMap::from([
                        (
                            "country".into(),
                            schema::ObjectField {
                                description: None,
                                r#type: schema::Type::Scalar(BsonScalarType::String),
                            },
                        ),
                        (
                            "street".into(),
                            schema::ObjectField {
                                description: None,
                                r#type: schema::Type::Scalar(BsonScalarType::String),
                            },
                        ),
                        (
                            "apartment".into(),
                            schema::ObjectField {
                                description: None,
                                r#type: schema::Type::Nullable(Box::new(schema::Type::Scalar(
                                    BsonScalarType::String,
                                ))),
                            },
                        ),
                        (
                            "geocode".into(),
                            schema::ObjectField {
                                description: Some("Lat/Long".to_owned()),
                                r#type: schema::Type::Nullable(Box::new(schema::Type::Object(
                                    "Geocode".to_owned(),
                                ))),
                            },
                        ),
                    ]),
                },
            ),
            (
                "Article".into(),
                schema::ObjectType {
                    description: None,
                    fields: BTreeMap::from([(
                        "title".into(),
                        schema::ObjectField {
                            description: None,
                            r#type: schema::Type::Scalar(BsonScalarType::String),
                        },
                    )]),
                },
            ),
            (
                "Geocode".into(),
                schema::ObjectType {
                    description: None,
                    fields: BTreeMap::from([
                        (
                            "latitude".into(),
                            schema::ObjectField {
                                description: None,
                                r#type: schema::Type::Scalar(BsonScalarType::Double),
                            },
                        ),
                        (
                            "longitude".into(),
                            schema::ObjectField {
                                description: None,
                                r#type: schema::Type::Scalar(BsonScalarType::Double),
                            },
                        ),
                    ]),
                },
            ),
        ])),
        scalar_types: Cow::Owned(make_scalar_types()),
    }
}

fn make_primary_key_uniqueness_constraint(
    collection_name: &str,
) -> BTreeMap<String, UniquenessConstraint> {
    [(
        format!("{collection_name}_id"),
        UniquenessConstraint {
            unique_columns: vec!["_id".to_owned()],
        },
    )]
    .into()
}
