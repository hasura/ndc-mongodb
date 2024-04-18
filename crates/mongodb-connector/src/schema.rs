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
    models::{self as ndc, ArgumentInfo, Type},
};

use crate::{api_type_conversions::ConversionError, capabilities};

lazy_static! {
    pub static ref SCALAR_TYPES: BTreeMap<String, models::ScalarType> =
        capabilities::scalar_types();
}

pub async fn get_schema(config: &Configuration) -> Result<models::SchemaResponse, SchemaError> {
    Ok(models::SchemaResponse {
        collections: config.collections.clone(),
        functions: config.functions.clone(),
        procedures: config.procedures.clone(),
        object_types: config.object_types.iter().map(object_type_to_ndc).collect(),
        scalar_types: SCALAR_TYPES.clone(),
    })
}

fn object_type_to_ndc(
    (name, object_type): (&String, &schema::ObjectType),
) -> (String, ndc::ObjectType) {
    (
        name.clone(),
        ndc::ObjectType {
            fields: field_to_ndc(&object_type.fields),
            description: object_type.description.clone(),
        },
    )
}

fn field_to_ndc(
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
