use itertools::Itertools as _;
use lazy_static::lazy_static;
use mongodb_support::BsonScalarType;
use std::collections::BTreeMap;

use configuration::{
    native_procedure::NativeProcedure,
    native_query::{NativeQuery, NativeQueryRepresentation},
    schema::{self, ObjectField},
    Configuration,
};
use ndc_sdk::{
    connector::SchemaError,
    models::{self, ArgumentInfo, Type},
};

use crate::{api_type_conversions::ConversionError, capabilities};

lazy_static! {
    pub static ref SCALAR_TYPES: BTreeMap<String, models::ScalarType> =
        capabilities::scalar_types();
}

pub async fn get_schema(config: &Configuration) -> Result<models::SchemaResponse, SchemaError> {
    let schema = &config.schema;
    let object_types = config.object_types().map(map_object_type).collect();
    let regular_collections = schema
        .collections
        .iter()
        .map(|(collection_name, collection)| {
            map_collection(&object_types, collection_name, collection)
        });

    let native_query_collections = config
        .native_queries
        .iter()
        .filter(|(_, nq)| nq.representation == NativeQueryRepresentation::Collection)
        .map(|(name, native_query)| map_native_query_collection(&object_types, name, native_query));

    let functions = config
        .native_queries
        .iter()
        .filter(|(_, nq)| nq.representation == NativeQueryRepresentation::Function)
        .map(|(name, native_query)| map_native_query_function(&object_types, name, native_query))
        .try_collect()?;

    let procedures = config
        .native_procedures
        .iter()
        .map(native_procedure_to_procedure)
        .collect();

    Ok(models::SchemaResponse {
        collections: regular_collections
            .chain(native_query_collections)
            .collect(),
        functions, // TODO: map object { __value: T } response type to simply T in schema response
        procedures,
        object_types,
        scalar_types: SCALAR_TYPES.clone(),
    })
}

fn map_object_type(
    (name, object_type): (&String, &schema::ObjectType),
) -> (String, models::ObjectType) {
    (
        name.clone(),
        models::ObjectType {
            fields: map_field_infos(&object_type.fields),
            description: object_type.description.clone(),
        },
    )
}

fn map_field_infos(
    fields: &BTreeMap<String, schema::ObjectField>,
) -> BTreeMap<String, models::ObjectField> {
    fields
        .iter()
        .map(|(name, field)| {
            (
                name.clone(),
                models::ObjectField {
                    r#type: map_type(&field.r#type),
                    description: field.description.clone(),
                },
            )
        })
        .collect()
}

fn map_type(t: &schema::Type) -> models::Type {
    fn map_normalized_type(t: &schema::Type) -> models::Type {
        match t {
            // ExtendedJSON can respresent any BSON value, including null, so it is always nullable
            schema::Type::ExtendedJSON => models::Type::Nullable {
                underlying_type: Box::new(models::Type::Named {
                    name: mongodb_support::EXTENDED_JSON_TYPE_NAME.to_owned(),
                }),
            },
            schema::Type::Scalar(t) => models::Type::Named {
                name: t.graphql_name(),
            },
            schema::Type::Object(t) => models::Type::Named { name: t.clone() },
            schema::Type::ArrayOf(t) => models::Type::Array {
                element_type: Box::new(map_normalized_type(t)),
            },
            schema::Type::Nullable(t) => models::Type::Nullable {
                underlying_type: Box::new(map_normalized_type(t)),
            },
        }
    }
    map_normalized_type(&t.clone().normalize_type())
}

fn get_primary_key_uniqueness_constraint(
    object_types: &BTreeMap<String, models::ObjectType>,
    name: &str,
    collection_type: &str,
) -> Option<(String, models::UniquenessConstraint)> {
    // Check to make sure our collection's object type contains the _id objectid field
    // If it doesn't (should never happen, all collections need an _id column), don't generate the constraint
    let object_type = object_types.get(collection_type)?;
    let id_field = object_type.fields.get("_id")?;
    match &id_field.r#type {
        models::Type::Named { name } => {
            if *name == BsonScalarType::ObjectId.graphql_name() {
                Some(())
            } else {
                None
            }
        }
        models::Type::Nullable { .. } => None,
        models::Type::Array { .. } => None,
        models::Type::Predicate { .. } => None,
    }?;
    let uniqueness_constraint = models::UniquenessConstraint {
        unique_columns: vec!["_id".into()],
    };
    let constraint_name = format!("{}_id", name);
    Some((constraint_name, uniqueness_constraint))
}

fn map_collection(
    object_types: &BTreeMap<String, models::ObjectType>,
    name: &str,
    collection: &schema::Collection,
) -> models::CollectionInfo {
    let pk_constraint =
        get_primary_key_uniqueness_constraint(object_types, name, &collection.r#type);

    models::CollectionInfo {
        name: name.to_owned(),
        collection_type: collection.r#type.clone(),
        description: collection.description.clone(),
        arguments: Default::default(),
        foreign_keys: Default::default(),
        uniqueness_constraints: BTreeMap::from_iter(pk_constraint),
    }
}

fn map_native_query_collection(
    object_types: &BTreeMap<String, models::ObjectType>,
    name: &str,
    native_query: &NativeQuery,
) -> models::CollectionInfo {
    let pk_constraint =
        get_primary_key_uniqueness_constraint(object_types, name, &native_query.r#type);

    models::CollectionInfo {
        name: name.to_owned(),
        collection_type: native_query.r#type.clone(),
        description: native_query.description.clone(),
        arguments: schema_arguments(native_query.arguments),
        foreign_keys: Default::default(),
        uniqueness_constraints: BTreeMap::from_iter(pk_constraint),
    }
}

fn map_native_query_function(
    object_types: &BTreeMap<String, models::ObjectType>,
    name: &str,
    native_query: &NativeQuery,
) -> Result<models::FunctionInfo, SchemaError> {
    Ok(models::FunctionInfo {
        name: name.to_owned(),
        description: native_query.description.clone(),
        arguments: schema_arguments(native_query.arguments),
        result_type: function_result_type(object_types, &native_query.r#type)
            .map_err(|err| SchemaError::Other(Box::new(err)))?
            .clone(),
    })
}

fn native_procedure_to_procedure(
    (procedure_name, procedure): (&String, &NativeProcedure),
) -> models::ProcedureInfo {
    models::ProcedureInfo {
        name: procedure_name.clone(),
        description: procedure.description.clone(),
        arguments: schema_arguments(procedure.arguments.clone()),
        result_type: map_type(&procedure.result_type),
    }
}

fn schema_arguments(
    configured_arguments: BTreeMap<String, ObjectField>,
) -> BTreeMap<String, ArgumentInfo> {
    configured_arguments
        .into_iter()
        .map(|(name, field)| {
            (
                name,
                models::ArgumentInfo {
                    argument_type: map_type(&field.r#type),
                    description: field.description,
                },
            )
        })
        .collect()
}

fn function_result_type<'a>(
    object_types: &'a BTreeMap<String, models::ObjectType>,
    object_type_name: &str,
) -> Result<&'a Type, ConversionError> {
    let object_type = object_types
        .get(object_type_name)
        .ok_or_else(|| ConversionError::UnknownObjectType(object_type_name.to_owned()))?;

    let value_field = object_type.fields.get("__value").ok_or_else(|| {
        ConversionError::UnknownObjectTypeField {
            object_type: object_type_name.to_owned(),
            field_name: "__value".to_owned(),
        }
    })?;

    Ok(&value_field.r#type)
}
