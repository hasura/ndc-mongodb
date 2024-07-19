use std::collections::BTreeMap;

use ndc_models as ndc;
use ndc_query_plan::{self as plan, inline_object_types, QueryPlanError};

use crate::{schema, MongoScalarType};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Parameter {
    Value {
        parameter_type: plan::Type<MongoScalarType>,
    },
    Predicate {
        object_type_name: ndc::ObjectTypeName,
    },
}

impl Parameter {
    pub fn from_object_field(
        object_types: &BTreeMap<ndc::ObjectTypeName, ndc::ObjectType>,
        field: schema::ObjectField,
    ) -> Result<Parameter, QueryPlanError> {
        let parameter = match field.r#type {
            schema::Type::Predicate { object_type_name } => {
                Parameter::Predicate { object_type_name }
            }
            t => {
                let parameter_type = inline_object_types(
                    object_types,
                    &t.into(),
                    MongoScalarType::lookup_scalar_type,
                )?;
                Parameter::Value { parameter_type }
            }
        };
        Ok(parameter)
    }
}
