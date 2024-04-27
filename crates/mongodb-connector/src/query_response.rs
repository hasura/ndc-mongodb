use std::{borrow::Cow, collections::BTreeMap};

use configuration::schema::{ObjectField, ObjectType, Type};
use indexmap::IndexMap;
use itertools::Itertools;
use mongodb::bson::{self, Bson};
use mongodb_agent_common::query::serialization::{bson_to_json, BsonToJsonError};
use ndc_sdk::models::{
    self as ndc, Aggregate, Field, NestedField, NestedObject, Query, QueryRequest, QueryResponse,
    Relationship, RowFieldValue, RowSet,
};
use serde::Deserialize;
use thiserror::Error;

use crate::api_type_conversions::{ConversionError, QueryContext};

const GEN_OBJECT_TYPE_PREFIX: &str = "__query__";

#[derive(Debug, Error)]
pub enum QueryResponseError {
    #[error("expected aggregates to be an object at path {}", path.join("."))]
    AggregatesNotObject { path: Vec<String> },

    #[error("{0}")]
    BsonDeserialization(#[from] bson::de::Error),

    #[error("{0}")]
    BsonToJson(#[from] BsonToJsonError),

    #[error("{0}")]
    Conversion(#[from] ConversionError),

    #[error("expected an array at path {}", path.join("."))]
    ExpectedArray { path: Vec<String> },

    #[error("expected an object at path {}", path.join("."))]
    ExpectedObject { path: Vec<String> },

    #[error("expected a single response document from MongoDB, but did not get one")]
    ExpectedSingleDocument,
}

type ObjectTypes = Vec<(String, ObjectType)>;
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

pub fn serialize_query_response(
    query_context: &QueryContext<'_>,
    query_request: &QueryRequest,
    response_documents: Vec<bson::Document>,
) -> Result<QueryResponse> {
    tracing::debug!(response_documents = %serde_json::to_string(&response_documents).unwrap(), "response from MongoDB");

    let collection_info = query_context.find_collection(&query_request.collection)?;
    let collection_name = &collection_info.name;

    // If the query request specified variable sets then we should have gotten a single document
    // from MongoDB with fields for multiple sets of results - one for each set of variables.
    let row_sets = if query_request.variables.is_some() {
        let responses: ResponsesForVariableSets = parse_single_document(response_documents)?;
        responses
            .row_sets
            .into_iter()
            .map(|docs| {
                serialize_row_set(
                    query_context,
                    &query_request.collection_relationships,
                    &[collection_name],
                    collection_name,
                    &query_request.query,
                    docs,
                )
            })
            .try_collect()
    } else {
        Ok(vec![serialize_row_set(
            query_context,
            &query_request.collection_relationships,
            &[],
            collection_name,
            &query_request.query,
            response_documents,
        )?])
    }?;
    let response = QueryResponse(row_sets);
    tracing::debug!(query_response = %serde_json::to_string(&response).unwrap());
    Ok(response)
}

fn serialize_row_set(
    query_context: &QueryContext<'_>,
    relationships: &BTreeMap<String, Relationship>,
    path: &[&str],
    collection_name: &str,
    query: &Query,
    docs: Vec<bson::Document>,
) -> Result<RowSet> {
    if !has_aggregates(query) {
        // When there are no aggregates we expect a list of rows
        let rows = query
            .fields
            .as_ref()
            .map(|fields| {
                serialize_rows(
                    query_context,
                    relationships,
                    path,
                    collection_name,
                    fields,
                    docs,
                )
            })
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
            .map(|aggregates| {
                serialize_aggregates(query_context, path, aggregates, row_set.aggregates)
            })
            .transpose()?;

        let rows = query
            .fields
            .as_ref()
            .map(|fields| {
                serialize_rows(
                    query_context,
                    relationships,
                    path,
                    collection_name,
                    fields,
                    row_set.rows,
                )
            })
            .transpose()?;

        Ok(RowSet { aggregates, rows })
    }
}

fn serialize_aggregates(
    query_context: &QueryContext<'_>,
    path: &[&str],
    _query_aggregates: &IndexMap<String, Aggregate>,
    value: Bson,
) -> Result<IndexMap<String, serde_json::Value>> {
    let (aggregates_type, temp_object_types) = type_for_aggregates()?;

    let object_types = extend_configured_object_types(query_context, temp_object_types);

    let json = bson_to_json(&aggregates_type, &object_types, value)?;

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
    query_context: &QueryContext<'_>,
    relationships: &BTreeMap<String, Relationship>,
    path: &[&str],
    collection_name: &str,
    query_fields: &IndexMap<String, Field>,
    docs: Vec<bson::Document>,
) -> Result<Vec<IndexMap<String, RowFieldValue>>> {
    let (row_type, temp_object_types) = type_for_row(
        query_context,
        relationships,
        path,
        collection_name,
        query_fields,
    )?;

    let object_types = extend_configured_object_types(query_context, temp_object_types);

    docs.into_iter()
        .map(|doc| {
            let json = bson_to_json(&row_type, &object_types, doc.into())?;
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
    query_context: &QueryContext<'_>,
    relationships: &BTreeMap<String, Relationship>,
    path: &[&str],
    collection_name: &str,
    query: &Query,
) -> Result<(Type, ObjectTypes)> {
    let mut fields = BTreeMap::new();
    let mut object_types = vec![];

    if has_aggregates(query) {
        let (aggregates_type, nested_object_types) = type_for_aggregates()?;
        fields.insert(
            "aggregates".to_owned(),
            ObjectField {
                r#type: aggregates_type,
                description: Default::default(),
            },
        );
        object_types.extend(nested_object_types);
    }

    if let Some(query_fields) = &query.fields {
        let (row_type, nested_object_types) = type_for_row(
            query_context,
            relationships,
            path,
            collection_name,
            query_fields,
        )?;
        fields.insert(
            "rows".to_owned(),
            ObjectField {
                r#type: Type::ArrayOf(Box::new(row_type)),
                description: Default::default(),
            },
        );
        object_types.extend(nested_object_types);
    }

    let (row_set_type_name, row_set_type) = named_type(path, "row_set");
    let object_type = ObjectType {
        description: Default::default(),
        fields,
    };
    object_types.push((row_set_type_name, object_type));

    Ok((row_set_type, object_types))
}

// TODO: infer response type for aggregates MDB-130
fn type_for_aggregates() -> Result<(Type, ObjectTypes)> {
    Ok((Type::ExtendedJSON, Default::default()))
}

fn type_for_row(
    query_context: &QueryContext<'_>,
    relationships: &BTreeMap<String, Relationship>,
    path: &[&str],
    collection_name: &str,
    query_fields: &IndexMap<String, Field>,
) -> Result<(Type, ObjectTypes)> {
    let mut object_types = vec![];

    let fields = query_fields
        .iter()
        .map(|(field_name, field_definition)| {
            let (field_type, nested_object_types) = type_for_field(
                query_context,
                relationships,
                &append_to_path(path, [field_name.as_ref()]),
                collection_name,
                field_definition,
            )?;
            object_types.extend(nested_object_types);
            Ok((
                field_name.clone(),
                ObjectField {
                    description: Default::default(),
                    r#type: field_type,
                },
            ))
        })
        .try_collect::<_, _, QueryResponseError>()?;

    let (row_type_name, row_type) = named_type(path, "row");
    let object_type = ObjectType {
        description: Default::default(),
        fields,
    };
    object_types.push((row_type_name, object_type));

    Ok((row_type, object_types))
}

fn type_for_field(
    query_context: &QueryContext<'_>,
    relationships: &BTreeMap<String, Relationship>,
    path: &[&str],
    collection_name: &str,
    field_definition: &ndc::Field,
) -> Result<(Type, ObjectTypes)> {
    match field_definition {
        ndc::Field::Column { column, fields } => {
            let field_type = find_field_type(query_context, path, collection_name, column)?;

            let (requested_type, temp_object_types) = prune_type_to_field_selection(
                query_context,
                relationships,
                path,
                field_type,
                fields.as_ref(),
            )?;

            Ok((requested_type, temp_object_types))
        }

        ndc::Field::Relationship {
            query,
            relationship,
            ..
        } => {
            let (requested_type, temp_object_types) =
                type_for_relation_field(query_context, relationships, path, query, relationship)?;

            Ok((requested_type, temp_object_types))
        }
    }
}

fn find_field_type<'a>(
    query_context: &'a QueryContext<'a>,
    path: &[&str],
    collection_name: &str,
    column: &str,
) -> Result<&'a Type> {
    let object_type = query_context.find_collection_object_type(collection_name)?;
    let field_type = object_type.value.fields.get(column).ok_or_else(|| {
        ConversionError::UnknownObjectTypeField {
            object_type: object_type.name.to_string(),
            field_name: column.to_string(),
            path: path_to_owned(path),
        }
    })?;
    Ok(&field_type.r#type)
}

/// Computes a new hierarchy of object types (if necessary) that select a subset of fields from
/// existing object types to match the fields requested by the query. Recurses into nested objects,
/// arrays, and nullable type references.
///
/// Scalar types are returned without modification.
///
/// Returns a reference to the pruned type, and a list of newly-computed object types with
/// generated names.
pub fn prune_type_to_field_selection(
    query_context: &QueryContext<'_>,
    relationships: &BTreeMap<String, Relationship>,
    path: &[&str],
    input_type: &Type,
    fields: Option<&NestedField>,
) -> Result<(Type, Vec<(String, ObjectType)>)> {
    match (input_type, fields) {
        (t, None) => Ok((t.clone(), Default::default())),
        (t @ Type::Scalar(_) | t @ Type::ExtendedJSON, _) => Ok((t.clone(), Default::default())),

        (Type::Nullable(t), _) => {
            let (underlying_type, object_types) =
                prune_type_to_field_selection(query_context, relationships, path, t, fields)?;
            Ok((Type::Nullable(Box::new(underlying_type)), object_types))
        }
        (Type::ArrayOf(t), Some(NestedField::Array(nested))) => {
            let (element_type, object_types) = prune_type_to_field_selection(
                query_context,
                relationships,
                path,
                t,
                Some(&nested.fields),
            )?;
            Ok((Type::ArrayOf(Box::new(element_type)), object_types))
        }
        (Type::Object(t), Some(NestedField::Object(nested))) => {
            object_type_for_field_subset(query_context, relationships, path, t, nested)
        }

        (_, Some(NestedField::Array(_))) => Err(QueryResponseError::ExpectedArray {
            path: path_to_owned(path),
        }),
        (_, Some(NestedField::Object(_))) => Err(QueryResponseError::ExpectedObject {
            path: path_to_owned(path),
        }),
    }
}

/// We have a configured object type for a collection, or for a nested object in a collection. But
/// the query may request a subset of fields from that object type. We need to compute a new object
/// type for that requested subset.
///
/// Returns a reference to the newly-generated object type, and a list of all new object types with
/// generated names including the newly-generated object type, and types for any nested objects.
fn object_type_for_field_subset(
    query_context: &QueryContext<'_>,
    relationships: &BTreeMap<String, Relationship>,
    path: &[&str],
    object_type_name: &str,
    requested_fields: &NestedObject,
) -> Result<(Type, Vec<(String, ObjectType)>)> {
    let object_type = query_context.find_object_type(object_type_name)?.value;
    let (fields, object_type_sets): (_, Vec<Vec<_>>) = requested_fields
        .fields
        .iter()
        .map(|(name, requested_field)| {
            let (object_field, object_types) = requested_field_definition(
                query_context,
                relationships,
                &append_to_path(path, [name.as_ref()]),
                object_type_name,
                object_type,
                requested_field,
            )?;
            Ok(((name.clone(), object_field), object_types))
        })
        .process_results::<_, _, QueryResponseError, _>(|iter| iter.unzip())?;

    let pruned_object_type = ObjectType {
        fields,
        description: None,
    };
    let (pruned_object_type_name, pruned_type) = named_type(path, "fields");

    let mut object_types: Vec<(String, ObjectType)> =
        object_type_sets.into_iter().flatten().collect();
    object_types.push((pruned_object_type_name, pruned_object_type));

    Ok((pruned_type, object_types))
}

/// Given an object type for a value, and a requested field from that value, produce an updated
/// object field definition to match the request. This must take into account aliasing where the
/// name of the requested field maps to a different name on the underlying type.
fn requested_field_definition(
    query_context: &QueryContext<'_>,
    relationships: &BTreeMap<String, Relationship>,
    path: &[&str],
    object_type_name: &str,
    object_type: &ObjectType,
    requested_field: &Field,
) -> Result<(ObjectField, Vec<(String, ObjectType)>)> {
    match requested_field {
        Field::Column { column, fields } => {
            let field_def = object_type.fields.get(column).ok_or_else(|| {
                ConversionError::UnknownObjectTypeField {
                    object_type: object_type_name.to_owned(),
                    field_name: column.to_owned(),
                    path: path_to_owned(path),
                }
            })?;
            let (field_type, object_types) = prune_type_to_field_selection(
                query_context,
                relationships,
                path,
                &field_def.r#type,
                fields.as_ref(),
            )?;
            let pruned_field = ObjectField {
                r#type: field_type,
                description: None,
            };
            Ok((pruned_field, object_types))
        }
        Field::Relationship {
            query,
            relationship,
            ..
        } => {
            let (relation_type, temp_object_types) =
                type_for_relation_field(query_context, relationships, path, query, relationship)?;
            let relation_field = ObjectField {
                r#type: relation_type,
                description: None,
            };
            Ok((relation_field, temp_object_types))
        }
    }
}

fn type_for_relation_field(
    query_context: &QueryContext<'_>,
    relationships: &BTreeMap<String, Relationship>,
    path: &[&str],
    query: &Query,
    relationship: &str,
) -> Result<(Type, Vec<(String, ObjectType)>)> {
    let relationship_def =
        relationships
            .get(relationship)
            .ok_or_else(|| ConversionError::UnknownRelationship {
                relationship_name: relationship.to_owned(),
                path: path_to_owned(path),
            })?;
    type_for_row_set(
        query_context,
        relationships,
        path,
        &relationship_def.target_collection,
        query,
    )
}

pub fn extend_configured_object_types<'a>(
    query_context: &QueryContext<'a>,
    object_types: ObjectTypes,
) -> Cow<'a, BTreeMap<String, ObjectType>> {
    if object_types.is_empty() {
        // We're cloning a Cow, not a BTreeMap here. In production that will be a [Cow::Borrowed]
        // variant so effectively that means we're cloning a wide pointer
        query_context.object_types.clone()
    } else {
        // This time we're cloning the BTreeMap
        let mut extended_object_types = query_context.object_types.clone().into_owned();
        extended_object_types.extend(object_types);
        Cow::Owned(extended_object_types)
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

fn has_aggregates(query: &Query) -> bool {
    match &query.aggregates {
        Some(aggregates) => !aggregates.is_empty(),
        None => false,
    }
}

fn append_to_path<'a>(path: &[&'a str], elems: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    path.iter().copied().chain(elems).collect()
}

fn path_to_owned(path: &[&str]) -> Vec<String> {
    path.iter().map(|x| (*x).to_owned()).collect()
}

fn named_type(path: &[&str], name_suffix: &str) -> (String, Type) {
    let name = format!(
        "{GEN_OBJECT_TYPE_PREFIX}{}_{name_suffix}",
        path.iter().join("_")
    );
    let t = Type::Object(name.clone());
    (name, t)
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
