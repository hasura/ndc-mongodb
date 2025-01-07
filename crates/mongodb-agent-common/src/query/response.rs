use std::collections::BTreeMap;

use configuration::MongoScalarType;
use indexmap::IndexMap;
use itertools::Itertools;
use mongodb::bson::{self, Bson};
use mongodb_support::ExtendedJsonMode;
use ndc_models::{QueryResponse, RowFieldValue, RowSet};
use serde::Deserialize;
use thiserror::Error;
use tracing::instrument;

use crate::{
    mongo_query_plan::{
        Aggregate, Field, NestedArray, NestedField, NestedObject, ObjectField, ObjectType, Query,
        QueryPlan, Type,
    },
    query::serialization::{bson_to_json, BsonToJsonError},
};

use super::serialization::is_nullable;

#[derive(Debug, Error)]
pub enum QueryResponseError {
    #[error("expected aggregates to be an object at path {}", path.join("."))]
    AggregatesNotObject { path: Vec<String> },

    #[error("{0}")]
    BsonDeserialization(#[from] bson::de::Error),

    #[error("{0}")]
    BsonToJson(#[from] BsonToJsonError),

    #[error("expected a single response document from MongoDB, but did not get one")]
    ExpectedSingleDocument,

    #[error("a query field referenced a relationship, but no fields from the relationship were selected")]
    NoFieldsSelected { path: Vec<String> },
}

type Result<T> = std::result::Result<T, QueryResponseError>;

#[derive(Debug, Deserialize)]
struct BsonRowSet {
    #[serde(default)]
    aggregates: Bson,
    #[serde(default)]
    rows: Vec<bson::Document>,
}

#[instrument(name = "Serialize Query Response", skip_all, fields(internal.visibility = "user"))]
pub fn serialize_query_response(
    mode: ExtendedJsonMode,
    query_plan: &QueryPlan,
    response_documents: Vec<bson::Document>,
) -> Result<QueryResponse> {
    let collection_name = &query_plan.collection;

    let row_sets = if query_plan.has_variables() {
        response_documents
            .into_iter()
            .map(|document| {
                let row_set = bson::from_document(document)?;
                serialize_row_set_with_aggregates(
                    mode,
                    &[collection_name.as_str()],
                    &query_plan.query,
                    row_set,
                )
            })
            .try_collect()
    } else if query_plan.query.has_aggregates() {
        let row_set = parse_single_document(response_documents)?;
        Ok(vec![serialize_row_set_with_aggregates(
            mode,
            &[],
            &query_plan.query,
            row_set,
        )?])
    } else {
        Ok(vec![serialize_row_set_rows_only(
            mode,
            &[],
            &query_plan.query,
            response_documents,
        )?])
    }?;
    let response = QueryResponse(row_sets);
    tracing::debug!(query_response = %serde_json::to_string(&response).unwrap());
    Ok(response)
}

// When there are no aggregates we expect a list of rows
fn serialize_row_set_rows_only(
    mode: ExtendedJsonMode,
    path: &[&str],
    query: &Query,
    docs: Vec<bson::Document>,
) -> Result<RowSet> {
    let rows = query
        .fields
        .as_ref()
        .map(|fields| serialize_rows(mode, path, fields, docs))
        .transpose()?;

    Ok(RowSet {
        aggregates: None,
        rows,
        groups: None, // TODO: ENG-1486 implement group by
    })
}

// When there are aggregates we expect a single document with `rows` and `aggregates`
// fields
fn serialize_row_set_with_aggregates(
    mode: ExtendedJsonMode,
    path: &[&str],
    query: &Query,
    row_set: BsonRowSet,
) -> Result<RowSet> {
    let aggregates = query
        .aggregates
        .as_ref()
        .map(|aggregates| serialize_aggregates(mode, path, aggregates, row_set.aggregates))
        .transpose()?;

    let rows = query
        .fields
        .as_ref()
        .map(|fields| serialize_rows(mode, path, fields, row_set.rows))
        .transpose()?;

    Ok(RowSet {
        aggregates,
        rows,
        groups: None, // TODO: ENG-1486 implement group by
    })
}

fn serialize_aggregates(
    mode: ExtendedJsonMode,
    path: &[&str],
    query_aggregates: &IndexMap<ndc_models::FieldName, Aggregate>,
    value: Bson,
) -> Result<IndexMap<ndc_models::FieldName, serde_json::Value>> {
    let aggregates_type = type_for_aggregates(query_aggregates);
    let json = bson_to_json(mode, &aggregates_type, value)?;

    // The NDC type uses an IndexMap for aggregate values; we need to convert the map
    // underlying the Value::Object value to an IndexMap
    let aggregate_values = match json {
        serde_json::Value::Object(obj) => obj.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        _ => Err(QueryResponseError::AggregatesNotObject {
            path: path_to_owned(path),
        })?,
    };
    Ok(aggregate_values)
}

fn serialize_rows(
    mode: ExtendedJsonMode,
    path: &[&str],
    query_fields: &IndexMap<ndc_models::FieldName, Field>,
    docs: Vec<bson::Document>,
) -> Result<Vec<IndexMap<ndc_models::FieldName, RowFieldValue>>> {
    let row_type = type_for_row(path, query_fields)?;

    docs.into_iter()
        .map(|doc| {
            let json = bson_to_json(mode, &row_type, doc.into())?;
            // The NDC types use an IndexMap for each row value; we need to convert the map
            // underlying the Value::Object value to an IndexMap
            let index_map = match json {
                serde_json::Value::Object(obj) => obj
                    .into_iter()
                    .map(|(key, value)| (key.into(), RowFieldValue(value)))
                    .collect(),
                _ => unreachable!(),
            };
            Ok(index_map)
        })
        .try_collect()
}

fn type_for_row_set(
    path: &[&str],
    aggregates: &Option<IndexMap<ndc_models::FieldName, Aggregate>>,
    fields: &Option<IndexMap<ndc_models::FieldName, Field>>,
) -> Result<Type> {
    let mut object_fields = BTreeMap::new();

    if let Some(aggregates) = aggregates {
        object_fields.insert(
            "aggregates".into(),
            ObjectField {
                r#type: type_for_aggregates(aggregates),
                parameters: Default::default(),
            },
        );
    }

    if let Some(query_fields) = fields {
        let row_type = type_for_row(path, query_fields)?;
        object_fields.insert(
            "rows".into(),
            ObjectField {
                r#type: Type::ArrayOf(Box::new(row_type)),
                parameters: Default::default(),
            },
        );
    }

    Ok(Type::Object(ObjectType {
        fields: object_fields,
        name: None,
    }))
}

fn type_for_aggregates(query_aggregates: &IndexMap<ndc_models::FieldName, Aggregate>) -> Type {
    let fields = query_aggregates
        .iter()
        .map(|(field_name, aggregate)| {
            let result_type = match aggregate {
                Aggregate::ColumnCount { .. } => {
                    Type::Scalar(MongoScalarType::Bson(mongodb_support::BsonScalarType::Long))
                }
                Aggregate::StarCount => {
                    Type::Scalar(MongoScalarType::Bson(mongodb_support::BsonScalarType::Long))
                }
                Aggregate::SingleColumn { result_type, .. } => result_type.clone(),
            };
            (
                field_name.to_string().into(),
                ObjectField {
                    r#type: result_type,
                    parameters: Default::default(),
                },
            )
        })
        .collect();
    Type::Object(ObjectType { fields, name: None })
}

fn type_for_row(
    path: &[&str],
    query_fields: &IndexMap<ndc_models::FieldName, Field>,
) -> Result<Type> {
    let fields = query_fields
        .iter()
        .map(|(field_name, field_definition)| {
            let field_type = type_for_field(
                &append_to_path(path, [field_name.as_str()]),
                field_definition,
            )?;
            let object_field = ObjectField {
                r#type: field_type,
                parameters: Default::default() 
           };
            Ok((field_name.clone(), object_field))
        })
        .try_collect::<_, _, QueryResponseError>()?;
    Ok(Type::Object(ObjectType { fields, name: None }))
}

fn type_for_field(path: &[&str], field_definition: &Field) -> Result<Type> {
    let field_type: Type = match field_definition {
        Field::Column {
            column_type,
            fields: None,
            ..
        } => column_type.clone(),
        Field::Column {
            column_type,
            fields: Some(nested_field),
            ..
        } => type_for_nested_field(path, column_type, nested_field)?,
        Field::Relationship {
            aggregates, fields, ..
        } => type_for_row_set(path, aggregates, fields)?,
    };
    Ok(field_type)
}

pub fn type_for_nested_field(
    path: &[&str],
    parent_type: &Type,
    nested_field: &NestedField,
) -> Result<Type> {
    let field_type = match nested_field {
        ndc_query_plan::NestedField::Object(NestedObject { fields }) => {
            let t = type_for_row(path, fields)?;
            if is_nullable(parent_type) {
                t.into_nullable()
            } else {
                t
            }
        }
        ndc_query_plan::NestedField::Array(NestedArray {
            fields: nested_field,
        }) => {
            let element_type = type_for_nested_field(
                &append_to_path(path, ["[]"]),
                element_type(parent_type),
                nested_field,
            )?;
            let t = Type::ArrayOf(Box::new(element_type));
            if is_nullable(parent_type) {
                t.into_nullable()
            } else {
                t
            }
        }
    };
    Ok(field_type)
}

/// Get type for elements within an array type. Be permissive if the given type is not an array.
fn element_type(probably_array_type: &Type) -> &Type {
    match probably_array_type {
        Type::Nullable(pt) => element_type(pt),
        Type::ArrayOf(pt) => pt,
        pt => pt,
    }
}

fn parse_single_document<T>(documents: Vec<bson::Document>) -> Result<T>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let document = documents
        .into_iter()
        .next()
        .ok_or(QueryResponseError::ExpectedSingleDocument)?;
    let value = bson::from_document(document)?;
    Ok(value)
}

fn append_to_path<'a>(path: &[&'a str], elems: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    path.iter().copied().chain(elems).collect()
}

fn path_to_owned(path: &[&str]) -> Vec<String> {
    path.iter().map(|x| (*x).to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use configuration::{Configuration, MongoScalarType};
    use mongodb::bson::{self, Bson};
    use mongodb_support::{BsonScalarType, ExtendedJsonMode};
    use ndc_models::{QueryRequest, QueryResponse, RowFieldValue, RowSet};
    use ndc_query_plan::plan_for_query_request;
    use ndc_test_helpers::{
        array, collection, field, named_type, object, object_type, query, query_request,
        relation_field, relationship,
    };
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use crate::{
        mongo_query_plan::{MongoConfiguration, ObjectType, Type},
        test_helpers::make_nested_schema,
    };

    use super::{serialize_query_response, type_for_row_set};

    #[test]
    fn serializes_response_with_nested_fields() -> anyhow::Result<()> {
        let request = query_request()
            .collection("authors")
            .query(query().fields([field!("address" => "address", object!([
                field!("street"),
                field!("geocode" => "geocode", object!([
                    field!("longitude"),
                ])),
            ]))]))
            .into();
        let query_plan = plan_for_query_request(&make_nested_schema(), request)?;

        let response_documents = vec![bson::doc! {
            "address": {
                "street": "137 Maple Dr",
                "geocode": {
                    "longitude": 122.4194,
                },
            },
        }];

        let response =
            serialize_query_response(ExtendedJsonMode::Canonical, &query_plan, response_documents)?;
        assert_eq!(
            response,
            QueryResponse(vec![RowSet {
                aggregates: Default::default(),
                rows: Some(vec![[(
                    "address".into(),
                    RowFieldValue(json!({
                        "street": "137 Maple Dr",
                        "geocode": {
                            "longitude": 122.4194,
                        },
                    }))
                )]
                .into()]),
                groups: Default::default(),
            }])
        );
        Ok(())
    }

    #[test]
    fn serializes_response_with_nested_object_inside_array() -> anyhow::Result<()> {
        let request = query_request()
            .collection("authors")
            .query(query().fields([field!("articles" => "articles", array!(
                object!([
                    field!("title"),
                ])
            ))]))
            .into();
        let query_plan = plan_for_query_request(&make_nested_schema(), request)?;

        let response_documents = vec![bson::doc! {
            "articles": [
                { "title": "Modeling MongoDB with relational model" },
                { "title": "NoSQL databases: MongoDB vs cassandra" },
            ],
        }];

        let response =
            serialize_query_response(ExtendedJsonMode::Canonical, &query_plan, response_documents)?;
        assert_eq!(
            response,
            QueryResponse(vec![RowSet {
                aggregates: Default::default(),
                rows: Some(vec![[(
                    "articles".into(),
                    RowFieldValue(json!([
                        { "title": "Modeling MongoDB with relational model" },
                        { "title": "NoSQL databases: MongoDB vs cassandra" },
                    ]))
                )]
                .into()]),
                groups: Default::default(),
            }])
        );
        Ok(())
    }

    #[test]
    fn serializes_response_with_aliased_fields() -> anyhow::Result<()> {
        let request = query_request()
            .collection("authors")
            .query(query().fields([
                field!("address1" => "address", object!([
                    field!("line1" => "street"),
                ])),
                field!("address2" => "address", object!([
                    field!("latlong" => "geocode", object!([
                        field!("long" => "longitude"),
                    ])),
                ])),
            ]))
            .into();
        let query_plan = plan_for_query_request(&make_nested_schema(), request)?;

        let response_documents = vec![bson::doc! {
            "address1": {
                "line1": "137 Maple Dr",
            },
            "address2": {
                "latlong": {
                    "long": 122.4194,
                },
            },
        }];

        let response =
            serialize_query_response(ExtendedJsonMode::Canonical, &query_plan, response_documents)?;
        assert_eq!(
            response,
            QueryResponse(vec![RowSet {
                aggregates: Default::default(),
                rows: Some(vec![[
                    (
                        "address1".into(),
                        RowFieldValue(json!({
                            "line1": "137 Maple Dr",
                        }))
                    ),
                    (
                        "address2".into(),
                        RowFieldValue(json!({
                            "latlong": {
                                "long": 122.4194,
                            },
                        }))
                    )
                ]
                .into()]),
                groups: Default::default(),
            }])
        );
        Ok(())
    }

    #[test]
    fn serializes_response_with_decimal_128_fields() -> anyhow::Result<()> {
        let query_context = MongoConfiguration(Configuration {
            collections: [collection("business")].into(),
            object_types: [(
                "business".into(),
                object_type([
                    ("price", named_type("Decimal")),
                    ("price_extjson", named_type("ExtendedJSON")),
                ]),
            )]
            .into(),
            functions: Default::default(),
            procedures: Default::default(),
            native_mutations: Default::default(),
            native_queries: Default::default(),
            options: Default::default(),
        });

        let request = query_request()
            .collection("business")
            .query(query().fields([field!("price"), field!("price_extjson")]))
            .into();

        let query_plan = plan_for_query_request(&query_context, request)?;

        let response_documents = vec![bson::doc! {
            "price": Bson::Decimal128(bson::Decimal128::from_str("127.6486654").unwrap()),
            "price_extjson": Bson::Decimal128(bson::Decimal128::from_str("-4.9999999999").unwrap()),
        }];

        let response =
            serialize_query_response(ExtendedJsonMode::Canonical, &query_plan, response_documents)?;
        assert_eq!(
            response,
            QueryResponse(vec![RowSet {
                aggregates: Default::default(),
                rows: Some(vec![[
                    ("price".into(), RowFieldValue(json!("127.6486654"))),
                    (
                        "price_extjson".into(),
                        RowFieldValue(json!({
                            "$numberDecimal": "-4.9999999999"
                        }))
                    ),
                ]
                .into()]),
                groups: Default::default(),
            }])
        );
        Ok(())
    }

    #[test]
    fn serializes_response_with_nested_extjson() -> anyhow::Result<()> {
        let query_context = MongoConfiguration(Configuration {
            collections: [collection("data")].into(),
            object_types: [(
                "data".into(),
                object_type([("value", named_type("ExtendedJSON"))]),
            )]
            .into(),
            functions: Default::default(),
            procedures: Default::default(),
            native_mutations: Default::default(),
            native_queries: Default::default(),
            options: Default::default(),
        });

        let request = query_request()
            .collection("data")
            .query(query().fields([field!("value")]))
            .into();

        let query_plan = plan_for_query_request(&query_context, request)?;

        let response_documents = vec![bson::doc! {
            "value": {
                "array": [
                    { "number": Bson::Int32(3) },
                    { "number": Bson::Decimal128(bson::Decimal128::from_str("127.6486654").unwrap()) },
                ],
                "string": "hello",
                "object": {
                    "foo": 1,
                    "bar": 2,
                },
            },
        }];

        let response =
            serialize_query_response(ExtendedJsonMode::Canonical, &query_plan, response_documents)?;
        assert_eq!(
            response,
            QueryResponse(vec![RowSet {
                aggregates: Default::default(),
                rows: Some(vec![[(
                    "value".into(),
                    RowFieldValue(json!({
                        "array": [
                            { "number": { "$numberInt": "3" } },
                            { "number": { "$numberDecimal": "127.6486654" } },
                        ],
                        "string": "hello",
                        "object": {
                            "foo": { "$numberInt": "1" },
                            "bar": { "$numberInt": "2" },
                        },
                    }))
                )]
                .into()]),
                groups: Default::default(),
            }])
        );
        Ok(())
    }

    #[test]
    fn serializes_response_with_nested_extjson_in_relaxed_mode() -> anyhow::Result<()> {
        let query_context = MongoConfiguration(Configuration {
            collections: [collection("data")].into(),
            object_types: [(
                "data".into(),
                object_type([("value", named_type("ExtendedJSON"))]),
            )]
            .into(),
            functions: Default::default(),
            procedures: Default::default(),
            native_mutations: Default::default(),
            native_queries: Default::default(),
            options: Default::default(),
        });

        let request = query_request()
            .collection("data")
            .query(query().fields([field!("value")]))
            .into();

        let query_plan = plan_for_query_request(&query_context, request)?;

        let response_documents = vec![bson::doc! {
            "value": {
                "array": [
                    { "number": Bson::Int32(3) },
                    { "number": Bson::Decimal128(bson::Decimal128::from_str("127.6486654").unwrap()) },
                ],
                "string": "hello",
                "object": {
                    "foo": 1,
                    "bar": 2,
                },
            },
        }];

        let response =
            serialize_query_response(ExtendedJsonMode::Relaxed, &query_plan, response_documents)?;
        assert_eq!(
            response,
            QueryResponse(vec![RowSet {
                aggregates: Default::default(),
                rows: Some(vec![[(
                    "value".into(),
                    RowFieldValue(json!({
                        "array": [
                            { "number": 3 },
                            { "number": { "$numberDecimal": "127.6486654" } },
                        ],
                        "string": "hello",
                        "object": {
                            "foo": 1,
                            "bar": 2,
                        },
                    }))
                )]
                .into()]),
                groups: Default::default(),
            }])
        );
        Ok(())
    }

    #[test]
    fn uses_field_path_to_guarantee_distinct_type_names() -> anyhow::Result<()> {
        let collection_name = "appearances";
        let request: QueryRequest = query_request()
            .collection(collection_name)
            .relationships([("author", relationship("authors", [("authorId", "id")]))])
            .query(
                query().fields([relation_field!("presenter" => "author", query().fields([
                    field!("addr" => "address", object!([
                        field!("street"),
                        field!("geocode" => "geocode", object!([
                            field!("latitude"),
                            field!("long" => "longitude"),
                        ]))
                    ])),
                    field!("articles" => "articles", array!(object!([
                        field!("article_title" => "title")
                    ]))),
                ]))]),
            )
            .into();
        let query_plan = plan_for_query_request(&make_nested_schema(), request)?;
        let path = [collection_name];

        let row_set_type = type_for_row_set(
            &path,
            &query_plan.query.aggregates,
            &query_plan.query.fields,
        )?;

        let expected = Type::Object(ObjectType {
            name: None,
            fields: [
                ("rows".into(), Type::ArrayOf(Box::new(Type::Object(ObjectType {
                    name: None,
                    fields: [
                        ("presenter".into(), Type::Object(ObjectType {
                            name: None,
                            fields: [
                                ("rows".into(), Type::ArrayOf(Box::new(Type::Object(ObjectType {
                                    name: None,
                                    fields: [
                                        ("addr".into(), Type::Object(ObjectType {
                                            name: None,
                                            fields: [
                                                ("geocode".into(), Type::Nullable(Box::new(Type::Object(ObjectType {
                                                    name: None,
                                                    fields: [
                                                        ("latitude".into(), Type::Scalar(MongoScalarType::Bson(BsonScalarType::Double))),
                                                        ("long".into(), Type::Scalar(MongoScalarType::Bson(BsonScalarType::Double))),
                                                    ].into(),
                                                })))),
                                                ("street".into(), Type::Scalar(MongoScalarType::Bson(BsonScalarType::String))),
                                            ].into(),
                                        })),
                                        ("articles".into(), Type::ArrayOf(Box::new(Type::Object(ObjectType {
                                            name: None,
                                            fields: [
                                                ("article_title".into(), Type::Scalar(MongoScalarType::Bson(BsonScalarType::String))),
                                            ].into(),
                                        })))),
                                    ].into(),
                                }))))
                            ].into(),
                        }))
                    ].into()
                }))))
            ].into(),
        });

        assert_eq!(row_set_type, expected);
        Ok(())
    }
}
