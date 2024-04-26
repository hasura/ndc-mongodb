use std::{borrow::Cow, collections::BTreeMap};

use configuration::schema::{ObjectField, ObjectType, Type};
use indexmap::IndexMap;
use itertools::Itertools as _;
use mongodb::bson::{self, from_bson, Bson};
use mongodb_agent_common::query::serialization::{bson_to_json, BsonToJsonError};
use ndc_sdk::models::{
    self as ndc, Aggregate, Field, NestedArray, NestedField, NestedObject, Query, QueryRequest,
    QueryResponse, RowFieldValue, RowSet,
};
use serde::Deserialize;
use thiserror::Error;

use crate::api_type_conversions::{ConversionError, QueryContext};

#[derive(Debug, Error)]
pub enum QueryResponseError {
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

    #[error("missing aggregate value in response: {0}")]
    MissingAggregateValue(String),

    #[error("expected {collection_name} to have a field named {column} of type {expected_type:?}, but value is missing from database response")]
    MissingColumnValue {
        collection_name: String,
        column: String,
        expected_type: Type,
    },

    #[error("results from relation are missing at path {}", path.join("."))]
    MissingRelationData { path: Vec<String> },

    #[error("placeholder")]
    TODORemoveMe,
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
    aggregates: BTreeMap<String, Bson>,
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
                    query_request,
                    &[],
                    collection_name,
                    &query_request.query,
                    docs,
                )
            })
            .try_collect()
    } else {
        // TODO: in an aggregation response we expect one document instead of a list of documents
        Ok(vec![serialize_row_set(
            query_context,
            query_request,
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
    query_request: &QueryRequest,
    path: &[&str],
    collection_name: &str,
    query: &Query,
    docs: Vec<bson::Document>,
) -> Result<RowSet> {
    if query
        .aggregates
        .as_ref()
        .unwrap_or(&IndexMap::new())
        .is_empty()
    {
        // When there are no aggregates we expect a list of rows
        let rows = query
            .fields
            .as_ref()
            .map(|fields| {
                serialize_rows(
                    query_context,
                    query_request,
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
            .map(|aggregates| serialize_aggregates(aggregates, row_set.aggregates))
            .transpose()?;

        let rows = query
            .fields
            .as_ref()
            .map(|fields| {
                serialize_rows(
                    query_context,
                    query_request,
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
    query_aggregates: &IndexMap<String, Aggregate>,
    mut aggregate_values: BTreeMap<String, Bson>,
) -> Result<IndexMap<String, serde_json::Value>> {
    query_aggregates
        .iter()
        .map(
            |(key, aggregate_definition)| match aggregate_values.remove_entry(key) {
                Some((owned_key, value)) => Ok((
                    owned_key,
                    // TODO: bson_to_json
                    from_bson(value).map_err(|_| QueryResponseError::TODORemoveMe)?,
                )),
                None => Err(QueryResponseError::MissingAggregateValue(key.clone())),
            },
        )
        .try_collect()
}

fn serialize_rows(
    query_context: &QueryContext<'_>,
    query_request: &QueryRequest,
    path: &[&str],
    collection_name: &str,
    query_fields: &IndexMap<String, Field>,
    docs: Vec<bson::Document>,
) -> Result<Vec<IndexMap<String, RowFieldValue>>> {
    docs.into_iter()
        .map(|doc| {
            serialize_single_row(
                query_context,
                query_request,
                path,
                collection_name,
                query_fields,
                doc,
            )
        })
        .try_collect()
}

fn serialize_single_row(
    query_context: &QueryContext<'_>,
    query_request: &QueryRequest,
    path: &[&str],
    collection_name: &str,
    query_fields: &IndexMap<String, Field>,
    mut doc: bson::Document,
) -> Result<IndexMap<String, RowFieldValue>> {
    query_fields
        .iter()
        .map(|(field_name, field_definition)| {
            let value = serialize_field_value(
                query_context,
                query_request,
                &append_to_path(path, [field_name.as_ref()]),
                collection_name,
                field_definition,
                field_name,
                &mut doc,
            )?;
            Ok((field_name.clone(), RowFieldValue(value)))
        })
        .try_collect()
}

fn serialize_field_value(
    query_context: &QueryContext<'_>,
    query_request: &QueryRequest,
    path: &[&str],
    collection_name: &str,
    field_definition: &ndc::Field,
    field_name: &str,
    input: &mut bson::Document,
) -> Result<serde_json::Value> {
    let value_option = input.remove(field_name);

    let (requested_type, value, temp_object_types) = match field_definition {
        ndc::Field::Column { column, fields } => {
            let field_type = find_field_type(query_context, path, collection_name, column)?;

            let (requested_type, temp_object_types) =
                prune_type_to_field_selection(query_context, query_request, path, field_type, fields.as_ref())?;

            let value = value_from_option(collection_name, column, &requested_type, value_option)?;

            (requested_type, value, temp_object_types)
        }

        ndc::Field::Relationship {
            query,
            relationship,
            ..
        } => {
            let (requested_type, temp_object_types) =
                type_for_relation_field(query_context, query_request, path, query, relationship)?;

            let value = value_option.ok_or_else(|| QueryResponseError::MissingRelationData {
                path: path_to_owned(path),
            })?;

            (requested_type, value, temp_object_types)
        }
    };

    let object_types = if temp_object_types.is_empty() {
        query_context.object_types.clone() // We're cloning a Cow, not a BTreeMap
    } else {
        let mut configured_types = query_context.object_types.clone().into_owned();
        configured_types.extend(temp_object_types);
        Cow::Owned(configured_types)
    };

    let json = bson_to_json(&requested_type, &object_types, value)?;
    Ok(json)
}
// TODO: test object relationship type
// TODO: test array relationship type

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
fn prune_type_to_field_selection(
    query_context: &QueryContext<'_>,
    query_request: &QueryRequest,
    path: &[&str],
    field_type: &Type,
    fields: Option<&NestedField>,
) -> Result<(Type, Vec<(String, ObjectType)>)> {
    match (field_type, fields) {
        (t, None) => Ok((t.clone(), Default::default())),
        (t @ Type::Scalar(_) | t @ Type::ExtendedJSON, _) => Ok((t.clone(), Default::default())),

        (Type::Nullable(t), _) => {
            let (underlying_type, object_types) =
                prune_type_to_field_selection(query_context, query_request, path, t, fields)?;
            Ok((Type::Nullable(Box::new(underlying_type)), object_types))
        }
        (Type::ArrayOf(t), Some(NestedField::Array(nested))) => {
            let (element_type, object_types) = prune_type_to_field_selection(
                query_context,
                query_request,
                path,
                t,
                Some(&nested.fields),
            )?;
            Ok((Type::ArrayOf(Box::new(element_type)), object_types))
        }
        (Type::Object(t), Some(NestedField::Object(nested))) => {
            object_type_for_field_subset(query_context, query_request, path, t, nested)
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
    query_request: &QueryRequest,
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
                query_request,
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
    let pruned_object_type_name = format!("requested_fields_{}", path.iter().join("_"));
    let pruned_type = Type::Object(pruned_object_type_name.clone());

    let mut object_types: Vec<(String, ObjectType)> =
        object_type_sets.into_iter().flatten().collect();
    object_types.push((pruned_object_type_name, pruned_object_type));

    Ok((pruned_type, object_types))
}
// TODO: why are objectIds serializing as extended JSON?

/// Given an object type for a value, and a requested field from that value, produce an updated
/// object field definition to match the request. This must take into account aliasing where the
/// name of the requested field maps to a different name on the underlying type.
fn requested_field_definition(
    query_context: &QueryContext<'_>,
    query_request: &QueryRequest,
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
                query_request,
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
                type_for_relation_field(query_context, query_request, path, query, relationship)?;
            let relation_field = ObjectField {
                r#type: relation_type,
                description: None,
            };
            Ok((relation_field, temp_object_types))
        }
    }
}

/// We have a predefined object type for each collection, and for each nested object in
/// a collection. Those types don't have fields defined for joined relationships since such fields
/// are a query-time thing. When a query requests related data we have to create a new field
/// definition to merge with fields in the predefined object type.
fn type_for_relation_field(
    query_context: &QueryContext<'_>,
    query_request: &QueryRequest,
    path: &[&str],
    query: &Query,
    relationship: &str,
) -> Result<(Type, Vec<(String, ObjectType)>)> {
    let relationship_def = query_request
        .collection_relationships
        .get(relationship)
        .ok_or_else(|| ConversionError::UnknownRelationship {
            relationship_name: relationship.to_owned(),
            path: path_to_owned(path),
        })?;
    let collection_name = &relationship_def.target_collection;
    let collection = query_context.find_collection(collection_name)?;

    // Related data always comes back as an array, even if the relation type is "Object".
    let relation_type = Type::ArrayOf(Box::new(Type::Object(
        collection.collection_type.to_owned(),
    )));

    // Translate requested query fields into a `NestedField` value to match what we get for
    // column fields.
    let fields = query.fields.as_ref().map(|query_fields| {
        NestedField::Array(NestedArray {
            fields: Box::new(NestedField::Object(NestedObject {
                fields: query_fields.clone(),
            })),
        })
    });

    let (requested_relation_type, mut temp_object_types) = prune_type_to_field_selection(
        query_context,
        query_request,
        path,
        &relation_type,
        fields.as_ref(),
    )?;

    // Relation data is wrapped in an object with a `rows` property
    let relation_object_type = ObjectType {
        fields: [(
            "rows".to_owned(),
            ObjectField {
                r#type: requested_relation_type,
                description: None,
            },
        )]
        .into(),
        description: Default::default(),
    };
    let relation_object_type_name = format!("relation_{}", path.iter().join("_"));
    temp_object_types.push((relation_object_type_name.clone(), relation_object_type));
    let requested_type = Type::Object(relation_object_type_name);

    Ok((requested_type, temp_object_types))
}

/// Check option result for a BSON value. If the value is missing but the expected type is nullable
/// then return null. Otherwise return an error.
fn value_from_option(
    collection_name: &str,
    column: &str,
    expected_type: &Type,
    value_option: Option<Bson>,
) -> Result<Bson> {
    match (expected_type, value_option) {
        (_, Some(value)) => Ok(value),
        (Type::Nullable(_), None) => Ok(Bson::Null),
        _ => Err(QueryResponseError::MissingColumnValue {
            collection_name: collection_name.to_string(),
            column: column.to_string(),
            expected_type: expected_type.clone(),
        }),
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
    let value = bson::from_document(document).map_err(|_| QueryResponseError::TODORemoveMe)?;
    Ok(value)
}

fn append_to_path<'a>(path: &[&'a str], elems: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    path.iter().copied().chain(elems).collect()
}

fn path_to_owned(path: &[&str]) -> Vec<String> {
    path.iter().map(|x| (*x).to_owned()).collect()
}

// TODO: test nested objects in arrays
#[cfg(test)]
mod tests {
    use std::{borrow::Cow, str::FromStr};

    use configuration::schema::Type;
    use mongodb::bson::{self, Bson};
    use mongodb_support::BsonScalarType;
    use ndc_sdk::models::{QueryResponse, RowFieldValue, RowSet};
    use ndc_test_helpers::{collection, field, object, object_type, query, query_request};
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use crate::{
        api_type_conversions::QueryContext,
        test_helpers::{make_nested_schema, make_scalar_types},
    };

    use super::serialize_query_response;

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
                    ])
                    .into(),
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
                    object_type([("value", Type::ExtendedJSON)]).into(),
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
}
