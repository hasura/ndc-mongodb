use std::borrow::Cow;

use configuration::MongoScalarType;

use crate::mongo_query_plan::{ObjectType, Type};

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
pub fn query_variable_name(name: &str, variable_type: &Type) -> String {
    let output = format!("var_{name}_{}", type_name(variable_type));
    escape_invalid_ascii(&output)
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

const ESCAPE_CHAR: char = '·';

/// MongoDB variable names allow a limited set of ASCII characters, or any non-ASCII character.
/// See https://www.mongodb.com/docs/manual/reference/aggregation-variables/
fn escape_invalid_ascii(input: &str) -> String {
    let mut encoded = String::new();
    for char in input.chars() {
        match char {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' => encoded.push(char),
            ESCAPE_CHAR => push_encoded_char(&mut encoded, char),
            char if char as u32 <= 127 => push_encoded_char(&mut encoded, char),
            char => encoded.push(char),
        }
    }
    encoded
}

/// Escape invalid characters using the escape character followed by the character's Unicode
/// codepoint in lowercase hex format
fn push_encoded_char(encoded: &mut String, char: char) {
    encoded.push(ESCAPE_CHAR);
    encoded.push_str(&format!("{:x}", char as u32));
}

#[cfg(test)]
mod tests {
    proptest! {
        #[test]
        fn variable_names_are_reproducible(bson in arb_bson()) {

        }
    }

    proptest! {
        #[test]
        fn variable_names_are_distinct(bson in arb_bson()) {

        }
    }

    proptest! {
        #[test]
        fn variable_names_are_valid_for_mongodb_expressions(bson in arb_bson()) {
            // begin with lowercase letter
            // limited ascii characters
        }
    }
}
