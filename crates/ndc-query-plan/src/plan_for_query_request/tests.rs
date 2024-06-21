use ndc_models::{self as ndc, OrderByTarget, OrderDirection, RelationshipType};
use ndc_test_helpers::*;
use pretty_assertions::assert_eq;
use serde_json::json;

use crate::{
    self as plan,
    plan_for_query_request::plan_test_helpers::{
        self, make_flat_schema, make_nested_schema, TestContext,
    },
    query_plan::UnrelatedJoin,
    ExistsInCollection, Expression, Field, OrderBy, Query, QueryContext, QueryPlan, Relationship,
};

use super::plan_for_query_request;

#[test]
fn translates_query_request_relationships() -> Result<(), anyhow::Error> {
    let request = query_request()
        .collection("schools")
        .relationships([
            (
                "school_classes",
                relationship("classes", [("_id", "school_id")]),
            ),
            (
                "class_students",
                relationship("students", [("_id", "class_id")]),
            ),
            (
                "class_department",
                relationship("departments", [("department_id", "_id")]).object_type(),
            ),
            (
                "school_directory",
                relationship("directory", [("_id", "school_id")]).object_type(),
            ),
            (
                "student_advisor",
                relationship("advisors", [("advisor_id", "_id")]).object_type(),
            ),
            (
                "existence_check",
                relationship("some_collection", [("some_id", "_id")]),
            ),
        ])
        .query(
            query()
                .fields([relation_field!("class_name" => "school_classes", query()
                    .fields([
                        relation_field!("student_name" => "class_students")
                    ])
                )])
                .order_by(vec![ndc::OrderByElement {
                    order_direction: OrderDirection::Asc,
                    target: OrderByTarget::Column {
                        name: "advisor_name".to_owned(),
                        field_path: None,
                        path: vec![
                            path_element("school_classes")
                                .predicate(binop(
                                    "Equal",
                                    target!(
                                        "_id",
                                        relations: [
                                        // path_element("school_classes"),
                                        path_element("class_department"),
                                    ],
                                    ),
                                    column_value!(
                                        "math_department_id",
                                        relations: [path_element("school_directory")],
                                    ),
                                ))
                                .into(),
                            path_element("class_students").into(),
                            path_element("student_advisor").into(),
                        ],
                    },
                }])
                // The `And` layer checks that we properly recursive into Expressions
                .predicate(and([ndc::Expression::Exists {
                    in_collection: related!("existence_check"),
                    predicate: None,
                }])),
        )
        .into();

    let expected = QueryPlan {
        collection: "schools".to_owned(),
        arguments: Default::default(),
        variables: None,
        variable_types: Default::default(),
        unrelated_collections: Default::default(),
        query: Query {
            predicate: Some(Expression::And {
                expressions: vec![Expression::Exists {
                    in_collection: ExistsInCollection::Related {
                        relationship: "existence_check".into(),
                    },
                    predicate: None,
                }],
            }),
            order_by: Some(OrderBy {
                elements: [plan::OrderByElement {
                    order_direction: OrderDirection::Asc,
                    target: plan::OrderByTarget::Column {
                        name: "advisor_name".into(),
                        field_path: Default::default(),
                        path: [
                            "school_classes_0".into(),
                            "class_students".into(),
                            "student_advisor".into(),
                        ]
                            .into(),
                    },
                }]
                    .into(),
            }),
            relationships: [
                (
                    "school_classes_0".to_owned(),
                    Relationship {
                        column_mapping: [("_id".to_owned(), "school_id".to_owned())].into(),
                        relationship_type: RelationshipType::Array,
                        target_collection: "classes".to_owned(),
                        arguments: Default::default(),
                        query: Query {
                            predicate: Some(plan::Expression::BinaryComparisonOperator {
                                column: plan::ComparisonTarget::Column {
                                    name: "_id".into(),
                                    field_path: None,
                                    column_type: plan::Type::Scalar(
                                        plan_test_helpers::ScalarType::Int,
                                    ),
                                    path: vec!["class_department".into()],
                                },
                                operator: plan_test_helpers::ComparisonOperator::Equal,
                                value: plan::ComparisonValue::Column {
                                    column: plan::ComparisonTarget::Column {
                                        name: "math_department_id".into(),
                                        field_path: None,
                                        column_type: plan::Type::Scalar(
                                            plan_test_helpers::ScalarType::Int,
                                        ),
                                        path: vec!["school_directory".into()],
                                    },
                                },
                            }),
                            relationships: [(
                                "class_department".into(),
                                plan::Relationship {
                                    target_collection: "departments".into(),
                                    column_mapping: [("department_id".into(), "_id".into())].into(),
                                    relationship_type: RelationshipType::Object,
                                    arguments: Default::default(),
                                    query: plan::Query {
                                        fields: Some([
                                            ("_id".into(), plan::Field::Column { column: "_id".into(), fields: None, column_type: plan::Type::Scalar(plan_test_helpers::ScalarType::Int) })
                                        ].into()),
                                        ..Default::default()
                                    },
                                },
                            ), (
                                    "class_students".into(),
                                    plan::Relationship {
                                        target_collection: "students".into(),
                                        column_mapping: [("_id".into(), "class_id".into())].into(),
                                        relationship_type: RelationshipType::Array,
                                        arguments: Default::default(),
                                        query: plan::Query {
                                            relationships: [(
                                                "student_advisor".into(),
                                                plan::Relationship {
                                                    column_mapping: [(
                                                        "advisor_id".into(),
                                                        "_id".into(),
                                                    )]
                                                        .into(),
                                                    relationship_type: RelationshipType::Object,
                                                    target_collection: "advisors".into(),
                                                    arguments: Default::default(),
                                                    query: plan::Query {
                                                        fields: Some(
                                                            [(
                                                                "advisor_name".into(),
                                                                plan::Field::Column {
                                                                    column: "advisor_name".into(),
                                                                    fields: None,
                                                                    column_type: plan::Type::Scalar(plan_test_helpers::ScalarType::String),
                                                                },
                                                            )]
                                                                .into(),
                                                        ),
                                                        ..Default::default()
                                                    },
                                                },
                                            )]
                                                .into(),
                                            ..Default::default()
                                        },
                                    },
                                ),
                                (
                                    "school_directory".to_owned(),
                                    Relationship {
                                        target_collection: "directory".to_owned(),
                                        column_mapping: [("_id".to_owned(), "school_id".to_owned())].into(),
                                        relationship_type: RelationshipType::Object,
                                        arguments: Default::default(),
                                        query: Query {
                                            fields: Some([
                                                ("math_department_id".into(), plan::Field::Column { column: "math_department_id".into(), fields: None, column_type: plan::Type::Scalar(plan_test_helpers::ScalarType::Int) })
                                            ].into()),
                                            ..Default::default()
                                        },
                                    },
                                ),
                            ]
                                .into(),
                            ..Default::default()
                        },
                    },
                ),
                (
                    "school_classes".to_owned(),
                    Relationship {
                        column_mapping: [("_id".to_owned(), "school_id".to_owned())].into(),
                        relationship_type: RelationshipType::Array,
                        target_collection: "classes".to_owned(),
                        arguments: Default::default(),
                        query: Query {
                            fields: Some(
                                [(
                                    "student_name".into(),
                                    plan::Field::Relationship {
                                        relationship: "class_students".into(),
                                        aggregates: None,
                                        fields: None,
                                    },
                                )]
                                    .into(),
                            ),
                            relationships: [(
                                "class_students".into(),
                                plan::Relationship {
                                    target_collection: "students".into(),
                                    column_mapping: [("_id".into(), "class_id".into())].into(),
                                    relationship_type: RelationshipType::Array,
                                    arguments: Default::default(),
                                    query: Query {
                                        scope: Some(plan::Scope::Named("scope_1".into())),
                                        ..Default::default() 
                                    },
                                },
                            )].into(),
                            scope: Some(plan::Scope::Named("scope_0".into())),
                            ..Default::default()
                        },
                    },
                ),
                (
                    "existence_check".to_owned(),
                    Relationship {
                        column_mapping: [("some_id".to_owned(), "_id".to_owned())].into(),
                        relationship_type: RelationshipType::Array,
                        target_collection: "some_collection".to_owned(),
                        arguments: Default::default(),
                        query: Query {
                            predicate: None,
                            ..Default::default()
                        },
                    },
                ),
            ]
                .into(),
            fields: Some(
                [(
                    "class_name".into(),
                    Field::Relationship {
                        relationship: "school_classes".into(),
                        aggregates: None,
                        fields: Some(
                            [(
                                "student_name".into(),
                                Field::Relationship {
                                    relationship: "class_students".into(),
                                    aggregates: None,
                                    fields: None,
                                },
                            )]
                                .into(),
                        ),
                    },
                )]
                    .into(),
            ),
            scope: Some(plan::Scope::Root),
            ..Default::default()
        },
    };

    let context = TestContext {
        collections: [
            collection("schools"),
            collection("classes"),
            collection("students"),
            collection("departments"),
            collection("directory"),
            collection("advisors"),
            collection("some_collection"),
        ]
        .into(),
        object_types: [
            (
                "schools".to_owned(),
                object_type([("_id", named_type("Int"))]),
            ),
            (
                "classes".to_owned(),
                object_type([
                    ("_id", named_type("Int")),
                    ("school_id", named_type("Int")),
                    ("department_id", named_type("Int")),
                ]),
            ),
            (
                "students".to_owned(),
                object_type([
                    ("_id", named_type("Int")),
                    ("class_id", named_type("Int")),
                    ("advisor_id", named_type("Int")),
                    ("student_name", named_type("String")),
                ]),
            ),
            (
                "departments".to_owned(),
                object_type([("_id", named_type("Int"))]),
            ),
            (
                "directory".to_owned(),
                object_type([
                    ("_id", named_type("Int")),
                    ("school_id", named_type("Int")),
                    ("math_department_id", named_type("Int")),
                ]),
            ),
            (
                "advisors".to_owned(),
                object_type([
                    ("_id", named_type("Int")),
                    ("advisor_name", named_type("String")),
                ]),
            ),
            (
                "some_collection".to_owned(),
                object_type([("_id", named_type("Int")), ("some_id", named_type("Int"))]),
            ),
        ]
        .into(),
        ..Default::default()
    };

    let query_plan = plan_for_query_request(&context, request)?;

    assert_eq!(query_plan, expected);
    Ok(())
}

#[test]
fn translates_root_column_references() -> Result<(), anyhow::Error> {
    let query_context = make_flat_schema();
    let query = query_request()
        .collection("authors")
        .query(query().fields([field!("last_name")]).predicate(exists(
            unrelated!("articles"),
            and([
                binop("Equal", target!("author_id"), column_value!(root("id"))),
                binop("Regex", target!("title"), value!("Functional.*")),
            ]),
        )))
        .into();
    let query_plan = plan_for_query_request(&query_context, query)?;

    let expected = QueryPlan {
        collection: "authors".into(),
        query: plan::Query {
            predicate: Some(plan::Expression::Exists {
                in_collection: plan::ExistsInCollection::Unrelated {
                    unrelated_collection: "__join_articles_0".into(),
                },
                predicate: Some(Box::new(plan::Expression::And {
                    expressions: vec![
                        plan::Expression::BinaryComparisonOperator {
                            column: plan::ComparisonTarget::Column {
                                name: "author_id".into(),
                                field_path: Default::default(),
                                column_type: plan::Type::Scalar(plan_test_helpers::ScalarType::Int),
                                path: Default::default(),
                            },
                            operator: plan_test_helpers::ComparisonOperator::Equal,
                            value: plan::ComparisonValue::Column {
                                column: plan::ComparisonTarget::ColumnInScope {
                                    name: "id".into(),
                                    field_path: Default::default(),
                                    column_type: plan::Type::Scalar(
                                        plan_test_helpers::ScalarType::Int,
                                    ),
                                    scope: plan::Scope::Root,
                                },
                            },
                        },
                        plan::Expression::BinaryComparisonOperator {
                            column: plan::ComparisonTarget::Column {
                                name: "title".into(),
                                field_path: Default::default(),
                                column_type: plan::Type::Scalar(
                                    plan_test_helpers::ScalarType::String,
                                ),
                                path: Default::default(),
                            },
                            operator: plan_test_helpers::ComparisonOperator::Regex,
                            value: plan::ComparisonValue::Scalar {
                                value: json!("Functional.*"),
                                value_type: plan::Type::Scalar(
                                    plan_test_helpers::ScalarType::String,
                                ),
                            },
                        },
                    ],
                })),
            }),
            fields: Some(
                [(
                    "last_name".into(),
                    plan::Field::Column {
                        column: "last_name".into(),
                        fields: None,
                        column_type: plan::Type::Scalar(plan_test_helpers::ScalarType::String),
                    },
                )]
                .into(),
            ),
            scope: Some(plan::Scope::Root),
            ..Default::default()
        },
        unrelated_collections: [(
            "__join_articles_0".into(),
            UnrelatedJoin {
                target_collection: "articles".into(),
                arguments: Default::default(),
                query: plan::Query {
                    predicate: Some(plan::Expression::And {
                        expressions: vec![
                            plan::Expression::BinaryComparisonOperator {
                                column: plan::ComparisonTarget::Column {
                                    name: "author_id".into(),
                                    column_type: plan::Type::Scalar(
                                        plan_test_helpers::ScalarType::Int,
                                    ),
                                    field_path: None,
                                    path: vec![],
                                },
                                operator: plan_test_helpers::ComparisonOperator::Equal,
                                value: plan::ComparisonValue::Column {
                                    column: plan::ComparisonTarget::ColumnInScope {
                                        name: "id".into(),
                                        scope: plan::Scope::Root,
                                        column_type: plan::Type::Scalar(
                                            plan_test_helpers::ScalarType::Int,
                                        ),
                                        field_path: None,
                                    },
                                },
                            },
                            plan::Expression::BinaryComparisonOperator {
                                column: plan::ComparisonTarget::Column {
                                    name: "title".into(),
                                    column_type: plan::Type::Scalar(
                                        plan_test_helpers::ScalarType::String,
                                    ),
                                    field_path: None,
                                    path: vec![],
                                },
                                operator: plan_test_helpers::ComparisonOperator::Regex,
                                value: plan::ComparisonValue::Scalar {
                                    value: "Functional.*".into(),
                                    value_type: plan::Type::Scalar(
                                        plan_test_helpers::ScalarType::String,
                                    ),
                                },
                            },
                        ],
                    }),
                    ..Default::default()
                },
            },
        )]
        .into(),
        arguments: Default::default(),
        variables: Default::default(),
        variable_types: Default::default(),
    };

    assert_eq!(query_plan, expected);
    Ok(())
}

#[test]
fn translates_aggregate_selections() -> Result<(), anyhow::Error> {
    let query_context = make_flat_schema();
    let query = query_request()
        .collection("authors")
        .query(query().aggregates([
            star_count_aggregate!("count_star"),
            column_count_aggregate!("count_id" => "last_name", distinct: true),
            column_aggregate!("avg_id" => "id", "Average"),
        ]))
        .into();
    let query_plan = plan_for_query_request(&query_context, query)?;

    let expected = QueryPlan {
        collection: "authors".into(),
        query: plan::Query {
            aggregates: Some(
                [
                    ("count_star".into(), plan::Aggregate::StarCount),
                    (
                        "count_id".into(),
                        plan::Aggregate::ColumnCount {
                            column: "last_name".into(),
                            distinct: true,
                        },
                    ),
                    (
                        "avg_id".into(),
                        plan::Aggregate::SingleColumn {
                            column: "id".into(),
                            function: plan_test_helpers::AggregateFunction::Average,
                            result_type: plan::Type::Scalar(plan_test_helpers::ScalarType::Double),
                        },
                    ),
                ]
                .into(),
            ),
            scope: Some(plan::Scope::Root),
            ..Default::default()
        },
        arguments: Default::default(),
        variables: Default::default(),
        variable_types: Default::default(),
        unrelated_collections: Default::default(),
    };

    assert_eq!(query_plan, expected);
    Ok(())
}

#[test]
fn translates_relationships_in_fields_predicates_and_orderings() -> Result<(), anyhow::Error> {
    let query_context = make_flat_schema();
    let query = query_request()
        .collection("authors")
        .query(
            query()
                .fields([
                    field!("last_name"),
                    relation_field!(
                        "articles" => "author_articles",
                        query().fields([field!("title"), field!("year")])
                    ),
                ])
                .predicate(exists(
                    related!("author_articles"),
                    binop("Regex", target!("title"), value!("Functional.*")),
                ))
                .order_by(vec![
                    ndc::OrderByElement {
                        order_direction: OrderDirection::Asc,
                        target: OrderByTarget::SingleColumnAggregate {
                            column: "year".into(),
                            function: "Average".into(),
                            path: vec![path_element("author_articles").into()],
                            field_path: None,
                        },
                    },
                    ndc::OrderByElement {
                        order_direction: OrderDirection::Desc,
                        target: OrderByTarget::Column {
                            name: "id".into(),
                            field_path: None,
                            path: vec![],
                        },
                    },
                ]),
        )
        .relationships([(
            "author_articles",
            relationship("articles", [("id", "author_id")]),
        )])
        .into();
    let query_plan = plan_for_query_request(&query_context, query)?;

    let expected = QueryPlan {
        collection: "authors".into(),
        query: plan::Query {
            predicate: Some(plan::Expression::Exists {
                in_collection: plan::ExistsInCollection::Related {
                    relationship: "author_articles".into(),
                },
                predicate: Some(Box::new(plan::Expression::BinaryComparisonOperator {
                    column: plan::ComparisonTarget::Column {
                        name: "title".into(),
                        field_path: Default::default(),
                        column_type: plan::Type::Scalar(plan_test_helpers::ScalarType::String),
                        path: Default::default(),
                    },
                    operator: plan_test_helpers::ComparisonOperator::Regex,
                    value: plan::ComparisonValue::Scalar {
                        value: "Functional.*".into(),
                        value_type: plan::Type::Scalar(plan_test_helpers::ScalarType::String),
                    },
                })),
            }),
            order_by: Some(plan::OrderBy {
                elements: vec![
                    plan::OrderByElement {
                        order_direction: OrderDirection::Asc,
                        target: plan::OrderByTarget::SingleColumnAggregate {
                            column: "year".into(),
                            function: plan_test_helpers::AggregateFunction::Average,
                            result_type: plan::Type::Scalar(plan_test_helpers::ScalarType::Double),
                            path: vec!["author_articles".into()],
                        },
                    },
                    plan::OrderByElement {
                        order_direction: OrderDirection::Desc,
                        target: plan::OrderByTarget::Column {
                            name: "id".into(),
                            field_path: None,
                            path: vec![],
                        },
                    },
                ],
            }),
            fields: Some(
                [
                    (
                        "last_name".into(),
                        plan::Field::Column {
                            column: "last_name".into(),
                            column_type: plan::Type::Scalar(plan_test_helpers::ScalarType::String),
                            fields: None,
                        },
                    ),
                    (
                        "articles".into(),
                        plan::Field::Relationship {
                            relationship: "author_articles".into(),
                            aggregates: None,
                            fields: Some(
                                [
                                    (
                                        "title".into(),
                                        plan::Field::Column {
                                            column: "title".into(),
                                            column_type: plan::Type::Scalar(
                                                plan_test_helpers::ScalarType::String,
                                            ),
                                            fields: None,
                                        },
                                    ),
                                    (
                                        "year".into(),
                                        plan::Field::Column {
                                            column: "year".into(),
                                            column_type: plan::Type::Nullable(Box::new(
                                                plan::Type::Scalar(
                                                    plan_test_helpers::ScalarType::Int,
                                                ),
                                            )),
                                            fields: None,
                                        },
                                    ),
                                ]
                                .into(),
                            ),
                        },
                    ),
                ]
                .into(),
            ),
            relationships: [(
                "author_articles".into(),
                plan::Relationship {
                    target_collection: "articles".into(),
                    column_mapping: [("id".into(), "author_id".into())].into(),
                    relationship_type: RelationshipType::Array,
                    arguments: Default::default(),
                    query: plan::Query {
                        fields: Some(
                            [
                                (
                                    "title".into(),
                                    plan::Field::Column {
                                        column: "title".into(),
                                        column_type: plan::Type::Scalar(
                                            plan_test_helpers::ScalarType::String,
                                        ),
                                        fields: None,
                                    },
                                ),
                                (
                                    "year".into(),
                                    plan::Field::Column {
                                        column: "year".into(),
                                        column_type: plan::Type::Nullable(Box::new(
                                            plan::Type::Scalar(plan_test_helpers::ScalarType::Int),
                                        )),
                                        fields: None,
                                    },
                                ),
                            ]
                            .into(),
                        ),
                        scope: Some(plan::Scope::Named("scope_0".into())),
                        ..Default::default()
                    },
                },
            )]
            .into(),
            scope: Some(plan::Scope::Root),
            ..Default::default()
        },
        arguments: Default::default(),
        variables: Default::default(),
        variable_types: Default::default(),
        unrelated_collections: Default::default(),
    };

    assert_eq!(query_plan, expected);
    Ok(())
}

#[test]
fn translates_nested_fields() -> Result<(), anyhow::Error> {
    let query_context = make_nested_schema();
    let query_request = query_request()
        .collection("authors")
        .query(query().fields([
            field!("author_address" => "address", object!([field!("address_country" => "country")])),
            field!("author_articles" => "articles", array!(object!([field!("article_title" => "title")]))),
            field!("author_array_of_arrays" => "array_of_arrays", array!(array!(object!([field!("article_title" => "title")]))))
        ]))
        .into();
    let query_plan = plan_for_query_request(&query_context, query_request)?;

    let expected = QueryPlan {
        collection: "authors".into(),
        query: plan::Query {
            fields: Some(
                [
                    (
                        "author_address".into(),
                        plan::Field::Column {
                            column: "address".into(),
                            column_type: plan::Type::Object(
                                query_context.find_object_type("Address")?,
                            ),
                            fields: Some(plan::NestedField::Object(plan::NestedObject {
                                fields: [(
                                    "address_country".into(),
                                    plan::Field::Column {
                                        column: "country".into(),
                                        column_type: plan::Type::Scalar(
                                            plan_test_helpers::ScalarType::String,
                                        ),
                                        fields: None,
                                    },
                                )]
                                .into(),
                            })),
                        },
                    ),
                    (
                        "author_articles".into(),
                        plan::Field::Column {
                            column: "articles".into(),
                            column_type: plan::Type::ArrayOf(Box::new(plan::Type::Object(
                                query_context.find_object_type("Article")?,
                            ))),
                            fields: Some(plan::NestedField::Array(plan::NestedArray {
                                fields: Box::new(plan::NestedField::Object(plan::NestedObject {
                                    fields: [(
                                        "article_title".into(),
                                        plan::Field::Column {
                                            column: "title".into(),
                                            fields: None,
                                            column_type: plan::Type::Scalar(
                                                plan_test_helpers::ScalarType::String,
                                            ),
                                        },
                                    )]
                                    .into(),
                                })),
                            })),
                        },
                    ),
                    (
                        "author_array_of_arrays".into(),
                        plan::Field::Column {
                            column: "array_of_arrays".into(),
                            fields: Some(plan::NestedField::Array(plan::NestedArray {
                                fields: Box::new(plan::NestedField::Array(plan::NestedArray {
                                    fields: Box::new(plan::NestedField::Object(
                                        plan::NestedObject {
                                            fields: [(
                                                "article_title".into(),
                                                plan::Field::Column {
                                                    column: "title".into(),
                                                    fields: None,
                                                    column_type: plan::Type::Scalar(
                                                        plan_test_helpers::ScalarType::String,
                                                    ),
                                                },
                                            )]
                                            .into(),
                                        },
                                    )),
                                })),
                            })),
                            column_type: plan::Type::ArrayOf(Box::new(plan::Type::ArrayOf(
                                Box::new(plan::Type::Object(
                                    query_context.find_object_type("Article")?,
                                )),
                            ))),
                        },
                    ),
                ]
                .into(),
            ),
            scope: Some(plan::Scope::Root),
            ..Default::default()
        },
        arguments: Default::default(),
        variables: Default::default(),
        variable_types: Default::default(),
        unrelated_collections: Default::default(),
    };

    assert_eq!(query_plan, expected);
    Ok(())
}

#[test]
fn translates_predicate_referencing_field_of_related_collection() -> anyhow::Result<()> {
    let query_context = make_nested_schema();
    let request = query_request()
        .collection("appearances")
        .relationships([("author", relationship("authors", [("authorId", "id")]))])
        .query(
            query()
                .fields([relation_field!("presenter" => "author", query().fields([
                    field!("name"),
                ]))])
                .predicate(not(is_null(
                    target!("name", relations: [path_element("author")]),
                ))),
        )
        .into();
    let query_plan = plan_for_query_request(&query_context, request)?;

    let expected = QueryPlan {
        collection: "appearances".into(),
        query: plan::Query {
            predicate: Some(plan::Expression::Not {
                expression: Box::new(plan::Expression::UnaryComparisonOperator {
                    column: plan::ComparisonTarget::Column {
                        name: "name".into(),
                        field_path: None,
                        column_type: plan::Type::Scalar(plan_test_helpers::ScalarType::String),
                        path: vec!["author".into()],
                    },
                    operator: ndc_models::UnaryComparisonOperator::IsNull,
                }),
            }),
            fields: Some(
                [(
                    "presenter".into(),
                    plan::Field::Relationship {
                        relationship: "author".into(),
                        aggregates: None,
                        fields: Some(
                            [(
                                "name".into(),
                                plan::Field::Column {
                                    column: "name".into(),
                                    fields: None,
                                    column_type: plan::Type::Scalar(
                                        plan_test_helpers::ScalarType::String,
                                    ),
                                },
                            )]
                            .into(),
                        ),
                    },
                )]
                .into(),
            ),
            relationships: [(
                "author".into(),
                plan::Relationship {
                    column_mapping: [("authorId".into(), "id".into())].into(),
                    relationship_type: RelationshipType::Array,
                    target_collection: "authors".into(),
                    arguments: Default::default(),
                    query: plan::Query {
                        fields: Some(
                            [(
                                "name".into(),
                                plan::Field::Column {
                                    column: "name".into(),
                                    fields: None,
                                    column_type: plan::Type::Scalar(
                                        plan_test_helpers::ScalarType::String,
                                    ),
                                },
                            )]
                            .into(),
                        ),
                        scope: Some(plan::Scope::Named("scope_0".into())),
                        ..Default::default()
                    },
                },
            )]
            .into(),
            scope: Some(plan::Scope::Root),
            ..Default::default()
        },
        arguments: Default::default(),
        variables: Default::default(),
        variable_types: Default::default(),
        unrelated_collections: Default::default(),
    };

    assert_eq!(query_plan, expected);
    Ok(())
}
