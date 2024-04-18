use lazy_static::lazy_static;
use std::collections::BTreeMap;

use configuration::{schema, Configuration};
use ndc_sdk::{connector::SchemaError, models as ndc};

use crate::capabilities;

lazy_static! {
    pub static ref SCALAR_TYPES: BTreeMap<String, ndc::ScalarType> = capabilities::scalar_types();
}

pub async fn get_schema(config: &Configuration) -> Result<ndc::SchemaResponse, SchemaError> {
    Ok(ndc::SchemaResponse {
        collections: config.collections.values().cloned().collect(),
        functions: config.functions.values().cloned().collect(),
        procedures: config.procedures.values().cloned().collect(),
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
) -> BTreeMap<String, ndc::ObjectField> {
    fields
        .iter()
        .map(|(name, field)| {
            (
                name.clone(),
                ndc::ObjectField {
                    r#type: map_type(&field.r#type),
                    description: field.description.clone(),
                },
            )
        })
        .collect()
}

fn map_type(t: &schema::Type) -> ndc::Type {
    fn map_normalized_type(t: &schema::Type) -> ndc::Type {
        match t {
            // ExtendedJSON can respresent any BSON value, including null, so it is always nullable
            schema::Type::ExtendedJSON => ndc::Type::Nullable {
                underlying_type: Box::new(ndc::Type::Named {
                    name: mongodb_support::EXTENDED_JSON_TYPE_NAME.to_owned(),
                }),
            },
            schema::Type::Scalar(t) => ndc::Type::Named {
                name: t.graphql_name(),
            },
            schema::Type::Object(t) => ndc::Type::Named { name: t.clone() },
            schema::Type::ArrayOf(t) => ndc::Type::Array {
                element_type: Box::new(map_normalized_type(t)),
            },
            schema::Type::Nullable(t) => ndc::Type::Nullable {
                underlying_type: Box::new(map_normalized_type(t)),
            },
        }
    }
    map_normalized_type(&t.clone().normalize_type())
}
