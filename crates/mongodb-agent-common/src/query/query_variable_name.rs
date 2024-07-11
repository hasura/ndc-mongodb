use std::borrow::Cow;

use configuration::MongoScalarType;

use crate::{
    mongo_query_plan::{ObjectType, Type},
    mongodb::sanitize::variable,
};

/// Maps a variable name and type from a [ndc_models::QueryRequest] `variables` map to a variable
/// name for use in a MongoDB aggregation pipeline. The type is incorporated into the produced name
/// because it is possible the same request variable may be used in different type contexts, which
/// may require different BSON conversions for the different contexts.
///
/// This function has some important requirements:
///
/// - reproducibility: the same input name and type must always produce the same output name
/// - distinct outputs: inputs with different types (or names) must produce different output names
/// - It must produce a valid MongoDB variable name (see https://www.mongodb.com/docs/manual/reference/aggregation-variables/)
pub fn query_variable_name(name: &ndc_models::VariableName, variable_type: &Type) -> String {
    variable(&format!("{}_{}", name, type_name(variable_type)))
}

fn type_name(input_type: &Type) -> Cow<'static, str> {
    match input_type {
        Type::Scalar(MongoScalarType::Bson(t)) => t.bson_name().into(),
        Type::Scalar(MongoScalarType::ExtendedJSON) => "unknown".into(),
        Type::Object(obj) => object_type_name(obj).into(),
        Type::ArrayOf(t) => format!("[{}]", type_name(t)).into(),
        Type::Nullable(t) => format!("nullable({})", type_name(t)).into(),
    }
}

fn object_type_name(obj: &ObjectType) -> String {
    let mut output = "{".to_string();
    for (key, t) in &obj.fields {
        output.push_str(&format!("{key}:{}", type_name(t)));
    }
    output.push('}');
    output
}

#[cfg(test)]
mod tests {
    use once_cell::sync::Lazy;
    use proptest::prelude::*;
    use regex::Regex;
    use test_helpers::arb_plan_type;

    use super::query_variable_name;

    proptest! {
        #[test]
        fn variable_names_are_reproducible(variable_name: String, variable_type in arb_plan_type()) {
            let a = query_variable_name(&variable_name.as_str().into(), &variable_type);
            let b = query_variable_name(&variable_name.into(), &variable_type);
            prop_assert_eq!(a, b)
        }
    }

    proptest! {
        #[test]
        fn variable_names_are_distinct_when_input_names_are_distinct(
            (name_a, name_b) in (any::<String>(), any::<String>()).prop_filter("names are equale", |(a, b)| a != b),
            variable_type in arb_plan_type()
        ) {
            let a = query_variable_name(&name_a.into(), &variable_type);
            let b = query_variable_name(&name_b.into(), &variable_type);
            prop_assert_ne!(a, b)
        }
    }

    proptest! {
        #[test]
        fn variable_names_are_distinct_when_types_are_distinct(
            variable_name: String,
            (type_a, type_b) in (arb_plan_type(), arb_plan_type()).prop_filter("types are equal", |(a, b)| a != b)
        ) {
            let a = query_variable_name(&variable_name.as_str().into(), &type_a);
            let b = query_variable_name(&variable_name.into(), &type_b);
            prop_assert_ne!(a, b)
        }
    }

    proptest! {
        #[test]
        fn variable_names_are_valid_for_mongodb_expressions(variable_name: String, variable_type in arb_plan_type()) {
            static VALID_NAME: Lazy<Regex> =
                Lazy::new(|| Regex::new(r"^[a-z\P{ascii}][_a-zA-Z0-9\P{ascii}]*$").unwrap());
            let name = query_variable_name(&variable_name.into(), &variable_type);
            prop_assert!(VALID_NAME.is_match(&name))
        }
    }
}
