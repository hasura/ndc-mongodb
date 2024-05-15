use std::collections::BTreeMap;

use configuration::MongoScalarType;
use indexmap::IndexMap;
use itertools::Itertools;
use mongodb::bson::{self, Bson};
use ndc_models::{QueryResponse, RowFieldValue, RowSet};
use ndc_query_plan::NULLABLE;
use serde::Deserialize;
use thiserror::Error;
use tracing::instrument;

use crate::{
    mongo_query_plan::{Aggregate, Field, ObjectType, Query, QueryPlan, Type},
    query::serialization::{bson_to_json, BsonToJsonError},
};

#[derive(Debug, Error)]
pub enum QueryResponseError {
    #[error("expected aggregates to be an object at path {}", path.join("."))]
    AggregatesNotObject { path: Vec<String> },

    #[error("{0}")]
    BsonDeserialization(#[from] bson::de::Error),

    #[error("{0}")]
    BsonToJson(#[from] BsonToJsonError),

    // #[error("{0}")]
    // QueryPlan(#[from] QueryPlanError),
    //
    // #[error("expected an array at path {}", path.join("."))]
    // ExpectedArray { path: Vec<String> },

    // #[error("expected an object at path {}", path.join("."))]
    // ExpectedObject { path: Vec<String> },
    #[error("expected a single response document from MongoDB, but did not get one")]
    ExpectedSingleDocument,

    #[error("a query field referenced a relationship, but no fields from the relationship were selected")]
    NoFieldsSelected { path: Vec<String> },
}

type Result<T> = std::result::Result<T, QueryResponseError>;

// These structs describe possible shapes of data returned by MongoDB query plans

#[derive(Debug, Deserialize)]
struct ResponsesForVariableSets {
    row_sets: Vec<Vec<bson::Document>>,
}

#[derive(Debug, Deserialize)]
struct BsonRowSet {
    #[serde(default)]
    aggregates: Bson,
    #[serde(default)]
    rows: Vec<bson::Document>,
}

#[instrument(name = "Serialize Query Response", skip_all, fields(internal.visibility = "user"))]
pub fn serialize_query_response(
    query_plan: &QueryPlan,
    response_documents: Vec<bson::Document>,
) -> Result<QueryResponse> {
    let collection_name = &query_plan.collection;

    // If the query request specified variable sets then we should have gotten a single document
    // from MongoDB with fields for multiple sets of results - one for each set of variables.
    let row_sets = if query_plan.variables.is_some() {
        let responses: ResponsesForVariableSets = parse_single_document(response_documents)?;
        responses
            .row_sets
            .into_iter()
            .map(|docs| serialize_row_set(&[collection_name], &query_plan.query, docs))
            .try_collect()
    } else {
        Ok(vec![serialize_row_set(
            &[],
            &query_plan.query,
            response_documents,
        )?])
    }?;
    let response = QueryResponse(row_sets);
    tracing::debug!(query_response = %serde_json::to_string(&response).unwrap());
    Ok(response)
}

fn serialize_row_set(path: &[&str], query: &Query, docs: Vec<bson::Document>) -> Result<RowSet> {
    if !query.has_aggregates() {
        // When there are no aggregates we expect a list of rows
        let rows = query
            .fields
            .as_ref()
            .map(|fields| serialize_rows(path, fields, docs))
            .transpose()?;

        Ok(RowSet {
            aggregates: None,
            rows,
        })
    } else {
        // When there are aggregates we expect a single document with `rows` and `aggregates`
        // fields
        let row_set: BsonRowSet = parse_single_document(docs)?;

        let aggregates = query
            .aggregates
            .as_ref()
            .map(|aggregates| serialize_aggregates(path, aggregates, row_set.aggregates))
            .transpose()?;

        let rows = query
            .fields
            .as_ref()
            .map(|fields| serialize_rows(path, fields, row_set.rows))
            .transpose()?;

        Ok(RowSet { aggregates, rows })
    }
}

fn serialize_aggregates(
    path: &[&str],
    _query_aggregates: &IndexMap<String, Aggregate>,
    value: Bson,
) -> Result<IndexMap<String, serde_json::Value>> {
    let aggregates_type = type_for_aggregates()?;
    let json = bson_to_json(&aggregates_type, value)?;

    // The NDC type uses an IndexMap for aggregate values; we need to convert the map
    // underlying the Value::Object value to an IndexMap
    let aggregate_values = match json {
        serde_json::Value::Object(obj) => obj.into_iter().collect(),
        _ => Err(QueryResponseError::AggregatesNotObject {
            path: path_to_owned(path),
        })?,
    };
    Ok(aggregate_values)
}

fn serialize_rows(
    path: &[&str],
    query_fields: &IndexMap<String, Field>,
    docs: Vec<bson::Document>,
) -> Result<Vec<IndexMap<String, RowFieldValue>>> {
    let row_type = type_for_row(path, query_fields)?;

    docs.into_iter()
        .map(|doc| {
            let json = bson_to_json(&row_type, doc.into())?;
            // The NDC types use an IndexMap for each row value; we need to convert the map
            // underlying the Value::Object value to an IndexMap
            let index_map = match json {
                serde_json::Value::Object(obj) => obj
                    .into_iter()
                    .map(|(key, value)| (key, RowFieldValue(value)))
                    .collect(),
                _ => unreachable!(),
            };
            Ok(index_map)
        })
        .try_collect()
}

fn type_for_row_set(
    path: &[&str],
    aggregates: &Option<IndexMap<String, Aggregate>>,
    fields: &Option<IndexMap<String, Field>>,
) -> Result<Type> {
    let mut type_fields = BTreeMap::new();

    if aggregates.is_some() {
        type_fields.insert("aggregates".to_owned(), type_for_aggregates()?);
    }

    if let Some(query_fields) = fields {
        let row_type = type_for_row(path, query_fields)?;
        type_fields.insert("rows".to_owned(), Type::ArrayOf(Box::new(row_type)));
    }

    Ok(Type::Object(ObjectType { fields: type_fields, name: None }))
}

// TODO: infer response type for aggregates MDB-130
fn type_for_aggregates() -> Result<Type> {
    Ok(Type::Scalar(MongoScalarType::ExtendedJSON))
}

fn type_for_row(path: &[&str], query_fields: &IndexMap<String, Field>) -> Result<Type> {
    let fields = query_fields
        .iter()
        .map(|(field_name, field_definition)| {
            let field_type = type_for_field(
                &append_to_path(path, [field_name.as_ref()]),
                field_definition,
            )?;
            Ok((field_name.clone(), field_type))
        })
        .try_collect::<_, _, QueryResponseError>()?;
    Ok(Type::Object(ObjectType { fields, name: None }))
}

pub fn type_for_field(path: &[&str], field_definition: &Field) -> Result<Type> {
    let field_type = match field_definition {
        Field::Column { column_type, .. } => column_type.clone(),
        Field::NestedObject {
            query, is_nullable, ..
        } => {
            let t = match &query.fields {
                Some(query_fields) => type_for_row(path, query_fields),
                None => Err(QueryResponseError::NoFieldsSelected {
                    path: path_to_owned(path),
                }),
            }?;
            if *is_nullable == NULLABLE {
                t.into_nullable()
            } else {
                t
            }
        }
        Field::NestedArray {
            field, is_nullable, ..
        } => {
            let element_type = type_for_field(path, field)?;
            let t = Type::ArrayOf(Box::new(element_type));
            if *is_nullable == NULLABLE {
                t.into_nullable()
            } else {
                t
            }
        }
        Field::Relationship {
            aggregates, fields, ..
        } => type_for_row_set(path, aggregates, fields)?,
    };
    // Allow null values without failing the query. If we remove this then it will be necessary to
    // add some indication to the nested object, nested array, and relationship cases to signal
    // whether they are allowed to be nullable.
    Ok(field_type)
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
    use std::{borrow::Cow, collections::BTreeMap, str::FromStr};

    use configuration::schema::{ObjectType, Type};
    use mongodb::bson::{self, Bson};
    use mongodb_support::BsonScalarType;
    use ndc_sdk::models::{QueryRequest, QueryResponse, RowFieldValue, RowSet};
    use ndc_test_helpers::{
        array, collection, field, object, query, query_request, relation_field, relationship,
    };
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use crate::{
        api_type_conversions::QueryContext,
        test_helpers::{make_nested_schema, make_scalar_types, object_type},
    };

    use super::{serialize_query_response, type_for_row_set};

    #[test]
    fn serializes_response_with_nested_fields() -> anyhow::Result<()> {
        let query_context = make_nested_schema();
        let request = query_request()
            .collection("authors")
            .query(query().fields([field!("address" => "address", object!([
                field!("street"),
                field!("geocode" => "geocode", object!([
                    field!("longitude"),
                ])),
            ]))]))
            .into();

        let response_documents = vec![bson::doc! {
            "address": {
                "street": "137 Maple Dr",
                "geocode": {
                    "longitude": 122.4194,
                },
            },
        }];

        let response = serialize_query_response(&query_context, &request, response_documents)?;
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
            }])
        );
        Ok(())
    }

    #[test]
    fn serializes_response_with_nested_object_inside_array() -> anyhow::Result<()> {
        let query_context = make_nested_schema();
        let request = query_request()
            .collection("authors")
            .query(query().fields([field!("articles" => "articles", array!(
                object!([
                    field!("title"),
                ])
            ))]))
            .into();

        let response_documents = vec![bson::doc! {
            "articles": [
                { "title": "Modeling MongoDB with relational model" },
                { "title": "NoSQL databases: MongoDB vs cassandra" },
            ],
        }];

        let response = serialize_query_response(&query_context, &request, response_documents)?;
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
            }])
        );
        Ok(())
    }

    #[test]
    fn serializes_response_with_aliased_fields() -> anyhow::Result<()> {
        let query_context = make_nested_schema();
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

        let response = serialize_query_response(&query_context, &request, response_documents)?;
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
            }])
        );
        Ok(())
    }

    #[test]
    fn serializes_response_with_decimal_128_fields() -> anyhow::Result<()> {
        let query_context = QueryContext {
            collections: Cow::Owned([collection("business")].into()),
            functions: Default::default(),
            object_types: Cow::Owned(
                [(
                    "business".to_owned(),
                    object_type([
                        ("price", Type::Scalar(BsonScalarType::Decimal)),
                        ("price_extjson", Type::ExtendedJSON),
                    ]),
                )]
                .into(),
            ),
            scalar_types: Cow::Owned(make_scalar_types()),
        };

        let request = query_request()
            .collection("business")
            .query(query().fields([field!("price"), field!("price_extjson")]))
            .into();

        let response_documents = vec![bson::doc! {
            "price": Bson::Decimal128(bson::Decimal128::from_str("127.6486654").unwrap()),
            "price_extjson": Bson::Decimal128(bson::Decimal128::from_str("-4.9999999999").unwrap()),
        }];

        let response = serialize_query_response(&query_context, &request, response_documents)?;
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
            }])
        );
        Ok(())
    }

    #[test]
    fn serializes_response_with_nested_extjson() -> anyhow::Result<()> {
        let query_context = QueryContext {
            collections: Cow::Owned([collection("data")].into()),
            functions: Default::default(),
            object_types: Cow::Owned(
                [(
                    "data".to_owned(),
                    object_type([("value", Type::ExtendedJSON)]),
                )]
                .into(),
            ),
            scalar_types: Cow::Owned(make_scalar_types()),
        };

        let request = query_request()
            .collection("data")
            .query(query().fields([field!("value")]))
            .into();

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

        let response = serialize_query_response(&query_context, &request, response_documents)?;
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
            }])
        );
        Ok(())
    }

    #[test]
    fn uses_field_path_to_guarantee_distinct_type_names() -> anyhow::Result<()> {
        let query_context = make_nested_schema();
        let collection_name = "appearances";
        let request: QueryRequest = query_request()
            .collection(collection_name)
            .relationships([("author", relationship("authors", [("authorId", "id")]))])
            .query(
                query().fields([relation_field!("author" => "presenter", query().fields([
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
        let path = [collection_name];

        let (row_set_type, object_types) = type_for_row_set(
            &query_context,
            &request.collection_relationships,
            &path,
            collection_name,
            &request.query,
        )?;

        // Convert object types into a map so we can compare without worrying about order
        let object_types: BTreeMap<String, ObjectType> = object_types.into_iter().collect();

        assert_eq!(
            (row_set_type, object_types),
            (
                Type::Object("__query__appearances_row_set".to_owned()),
                [
                    (
                        "__query__appearances_row_set".to_owned(),
                        object_type([(
                            "rows".to_owned(),
                            Type::ArrayOf(Box::new(Type::Object(
                                "__query__appearances_row".to_owned()
                            )))
                        )]),
                    ),
                    (
                        "__query__appearances_row".to_owned(),
                        object_type([(
                            "presenter".to_owned(),
                            Type::Object("__query__appearances_presenter_row_set".to_owned())
                        )]),
                    ),
                    (
                        "__query__appearances_presenter_row_set".to_owned(),
                        object_type([(
                            "rows",
                            Type::ArrayOf(Box::new(Type::Object(
                                "__query__appearances_presenter_row".to_owned()
                            )))
                        )]),
                    ),
                    (
                        "__query__appearances_presenter_row".to_owned(),
                        object_type([
                            (
                                "addr",
                                Type::Object(
                                    "__query__appearances_presenter_addr_fields".to_owned()
                                )
                            ),
                            (
                                "articles",
                                Type::ArrayOf(Box::new(Type::Object(
                                    "__query__appearances_presenter_articles_fields".to_owned()
                                )))
                            ),
                        ]),
                    ),
                    (
                        "__query__appearances_presenter_addr_fields".to_owned(),
                        object_type([
                            (
                                "geocode",
                                Type::Nullable(Box::new(Type::Object(
                                    "__query__appearances_presenter_addr_geocode_fields".to_owned()
                                )))
                            ),
                            ("street", Type::Scalar(BsonScalarType::String)),
                        ]),
                    ),
                    (
                        "__query__appearances_presenter_addr_geocode_fields".to_owned(),
                        object_type([
                            ("latitude", Type::Scalar(BsonScalarType::Double)),
                            ("long", Type::Scalar(BsonScalarType::Double)),
                        ]),
                    ),
                    (
                        "__query__appearances_presenter_articles_fields".to_owned(),
                        object_type([("article_title", Type::Scalar(BsonScalarType::String))]),
                    ),
                ]
                .into()
            )
        );
        Ok(())
    }
}
