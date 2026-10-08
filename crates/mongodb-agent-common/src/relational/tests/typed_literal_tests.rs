//! Literals compared with ObjectId and UUID fields are converted to those types, as the GraphQL
//! query path converts typed comparison values.

use configuration::Configuration;
use mongodb::bson::{self, doc, oid::ObjectId, Bson, Document};
use mongodb_support::aggregate::Stage;
use ndc_models::{CastType, Relation, RelationalExpression as E, RelationalLiteral};
use ndc_test_helpers::{array_of, collection, named_type, nullable, object_type};

use crate::{
    mongo_query_plan::MongoConfiguration,
    relational::{
        pipeline_builder::build_relational_pipeline_with_config, type_lookup::literal_operand,
    },
};

const ID: &str = "507f1f77bcf86cd799439011";
const OTHER_ID: &str = "507f191e810c19729de860ea";

fn orders_config() -> MongoConfiguration {
    MongoConfiguration(Configuration {
        collections: [collection("orders")].into(),
        object_types: [
            (
                "orders".into(),
                object_type([
                    ("_id", named_type("ObjectId")),
                    ("tenant_id", nullable(named_type("ObjectId"))),
                    ("status", named_type("String")),
                    ("owner_ids", array_of(named_type("ObjectId"))),
                    ("token", named_type("UUID")),
                    ("meta", named_type("order_meta")),
                ]),
            ),
            (
                "order_meta".into(),
                object_type([("created_by", named_type("ObjectId"))]),
            ),
        ]
        .into(),
        functions: Default::default(),
        procedures: Default::default(),
        native_mutations: Default::default(),
        native_queries: Default::default(),
        options: Default::default(),
    })
}

fn oid(hex: &str) -> ObjectId {
    ObjectId::parse_str(hex).unwrap()
}

fn column(index: u64) -> Box<E> {
    Box::new(E::Column { index })
}

fn string(value: &str) -> E {
    E::Literal {
        literal: RelationalLiteral::String {
            value: value.into(),
        },
    }
}

/// The shape the engine gives a permission-filter value: `CAST(<literal> AS <column type>)`.
fn permission_value(value: &str) -> Box<E> {
    Box::new(E::Cast {
        expr: Box::new(string(value)),
        from_type: Some(CastType::Utf8),
        as_type: CastType::Utf8,
    })
}

fn filter_orders(columns: &[&str], predicate: E) -> Relation {
    Relation::Filter {
        input: Box::new(Relation::From {
            collection: "orders".into(),
            columns: columns.iter().map(|c| (*c).into()).collect(),
            arguments: Default::default(),
        }),
        predicate,
    }
}

fn stages(relation: &Relation) -> Vec<Stage> {
    build_relational_pipeline_with_config(relation, Some(&orders_config()))
        .unwrap()
        .pipeline
        .stages
}

/// The early match and the filter's own `$match` carry the same query document.
fn assert_query_document(relation: &Relation, expected: Document) {
    assert_eq!(
        stages(relation),
        vec![Stage::Match(expected.clone()), Stage::Match(expected)]
    );
}

#[test]
fn permission_filter_value_becomes_object_id() {
    // `SELECT COUNT(1) FROM orders` under a `tenant_id` row filter, in the shape the engine sends.
    let relation = Relation::Project {
        input: Box::new(Relation::Aggregate {
            input: Box::new(Relation::Project {
                input: Box::new(filter_orders(
                    &["tenant_id"],
                    E::Eq {
                        left: column(0),
                        right: permission_value(ID),
                    },
                )),
                exprs: vec![],
            }),
            group_by: vec![],
            aggregates: vec![E::Count {
                expr: Box::new(E::Literal {
                    literal: RelationalLiteral::Int64 { value: 1 },
                }),
                distinct: false,
            }],
        }),
        exprs: vec![*column(0)],
    };

    let stages = stages(&relation);
    let expected = Stage::Match(doc! { "tenant_id": { "$eq": oid(ID) } });
    assert_eq!(stages[0], expected);
    assert_eq!(stages[1], expected);
    assert!(!format!("{stages:?}").contains("$toString"));
}

#[test]
fn reversed_comparison_flips_the_operator() {
    let relation = filter_orders(
        &["_id"],
        E::Lt {
            left: permission_value(ID),
            right: column(0),
        },
    );
    assert_query_document(&relation, doc! { "_id": { "$gt": oid(ID) } });
}

#[test]
fn in_list_values_become_object_ids() {
    let relation = filter_orders(
        &["tenant_id"],
        E::In {
            expr: column(0),
            list: vec![*permission_value(ID), string(OTHER_ID)],
        },
    );
    assert_query_document(
        &relation,
        doc! { "tenant_id": { "$in": [oid(ID), oid(OTHER_ID)] } },
    );
}

#[test]
fn nested_field_value_becomes_object_id() {
    let relation = filter_orders(
        &["meta"],
        E::Eq {
            left: Box::new(E::GetField {
                column: column(0),
                field: "created_by".into(),
            }),
            right: Box::new(string(ID)),
        },
    );
    assert_query_document(&relation, doc! { "meta.created_by": { "$eq": oid(ID) } });
}

#[test]
fn array_field_compares_by_element() {
    let relation = filter_orders(
        &["owner_ids"],
        E::Eq {
            left: column(0),
            right: Box::new(string(ID)),
        },
    );
    assert_query_document(&relation, doc! { "owner_ids": { "$eq": oid(ID) } });
}

#[test]
fn uuid_field_value_becomes_uuid() {
    let uuid = "6f1a3c2e-8b4d-4e5f-9a7b-0c1d2e3f4a5b";
    let relation = filter_orders(
        &["token"],
        E::Eq {
            left: column(0),
            right: permission_value(uuid),
        },
    );
    let expected = Bson::from(bson::Binary::from_uuid(
        bson::Uuid::parse_str(uuid).unwrap(),
    ));
    assert_query_document(&relation, doc! { "token": { "$eq": expected } });
}

#[test]
fn non_hex_string_stays_a_string() {
    let relation = filter_orders(
        &["tenant_id"],
        E::Eq {
            left: column(0),
            right: permission_value("not-an-object-id"),
        },
    );
    assert_query_document(
        &relation,
        doc! { "tenant_id": { "$eq": "not-an-object-id" } },
    );
}

#[test]
fn string_field_keeps_a_hex_looking_string() {
    let relation = filter_orders(
        &["status"],
        E::Eq {
            left: column(0),
            right: permission_value(ID),
        },
    );
    assert_query_document(&relation, doc! { "status": { "$eq": ID } });
}

#[test]
fn expr_fallback_converts_values() {
    // The column-to-column branch can't be a query document, so the whole OR goes to `$expr`.
    let relation = filter_orders(
        &["tenant_id", "status", "_id"],
        E::Or {
            left: Box::new(E::Eq {
                left: column(0),
                right: permission_value(ID),
            }),
            right: Box::new(E::Eq {
                left: column(1),
                right: column(2),
            }),
        },
    );
    assert_eq!(
        stages(&relation),
        vec![Stage::Match(doc! { "$expr": { "$or": [
            { "$eq": ["$tenant_id", oid(ID)] },
            { "$eq": ["$status", "$_id"] },
        ] } })]
    );
}

#[test]
fn not_in_converts_values_in_expr() {
    let relation = filter_orders(
        &["tenant_id"],
        E::NotIn {
            expr: column(0),
            list: vec![*permission_value(ID)],
        },
    );
    assert_eq!(
        stages(&relation),
        vec![Stage::Match(doc! { "$expr": {
            "$not": [{ "$in": ["$tenant_id", [oid(ID)]] }]
        } })]
    );
}

#[test]
fn between_bounds_become_object_ids() {
    let relation = filter_orders(
        &["_id"],
        E::Between {
            low: Box::new(string(ID)),
            expr: column(0),
            high: permission_value(OTHER_ID),
        },
    );
    assert_eq!(
        stages(&relation),
        vec![Stage::Match(doc! { "$expr": { "$and": [
            { "$gte": ["$_id", oid(ID)] },
            { "$lte": ["$_id", oid(OTHER_ID)] },
        ] } })]
    );
}

#[test]
fn literal_operand_looks_through_identity_casts_only() {
    let literal = RelationalLiteral::String { value: ID.into() };
    let nested = E::TryCast {
        expr: permission_value(ID),
        from_type: None,
        as_type: CastType::Utf8,
    };
    assert_eq!(literal_operand(&nested), Some(&literal));

    // A cast that changes the value's type is real work, not a wrapper.
    let to_timestamp = E::Cast {
        expr: Box::new(string("2026-01-01")),
        from_type: Some(CastType::Utf8),
        as_type: CastType::Timestamp,
    };
    assert_eq!(literal_operand(&to_timestamp), None);
}
