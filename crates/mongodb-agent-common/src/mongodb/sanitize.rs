use std::borrow::Cow;

use anyhow::anyhow;
use mongodb::bson::{doc, Document};
use once_cell::sync::Lazy;
use regex::Regex;

use crate::interface_types::MongoAgentError;

/// Produces a MongoDB expression that references a field by name in a way that is safe from code
/// injection.
///
/// TODO: equivalent to ColumnRef::Expression
pub fn get_field(name: &str) -> Document {
    doc! { "$getField": { "$literal": name } }
}

/// Returns its input prefixed with "v_" if it is a valid MongoDB variable name. Valid names may
/// include the ASCII characters [_a-zA-Z0-9] or any non-ASCII characters. The exclusion of special
/// characters like `$` and `.` avoids potential code injection.
///
/// We add the "v_" prefix because variable names may not begin with an underscore, but in some
/// cases, like when using relation-mapped column names as variable names, we want to be able to
/// use names like "_id".
///
/// TODO: Instead of producing an error we could use an escaping scheme to unambiguously map
/// invalid characters to safe ones.
pub fn variable(name: &str) -> Result<String, MongoAgentError> {
    static VALID_EXPRESSION: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^[_a-zA-Z0-9\P{ascii}]+$").unwrap());
    if VALID_EXPRESSION.is_match(name) {
        Ok(format!("v_{name}"))
    } else {
        Err(MongoAgentError::InvalidVariableName(name.to_owned()))
    }
}

/// Returns false if the name contains characters that MongoDB will interpret specially, such as an
/// initial dollar sign, or dots.
pub fn is_name_safe(name: &str) -> bool {
    !(name.starts_with('$') || name.contains('.'))
}

/// Given a collection or field name, returns Ok if the name is safe, or Err if it contains
/// characters that MongoDB will interpret specially.
///
/// TODO: MDB-159, MBD-160 remove this function in favor of ColumnRef which is infallible
pub fn safe_name(name: &str) -> Result<Cow<str>, MongoAgentError> {
    if name.starts_with('$') || name.contains('.') {
        Err(MongoAgentError::BadQuery(anyhow!("cannot execute query that includes the name, \"{name}\", because it includes characters that MongoDB interperets specially")))
    } else {
        Ok(Cow::Borrowed(name))
    }
}

// The escape character must be a valid character in MongoDB variable names, but must not appear in
// lower-case hex strings. A non-ASCII character works if we specifically map it to a two-character
// hex escape sequence (see [ESCAPE_CHAR_ESCAPE_SEQUENCE]). Another option would be to use an
// allowed ASCII character such as 'x'.
const ESCAPE_CHAR: char = '·';

/// We want all escape sequences to be two-character hex strings so this must be a value that does
/// not represent an ASCII character, and that is <= 0xff.
const ESCAPE_CHAR_ESCAPE_SEQUENCE: u32 = 0xff;

/// MongoDB variable names allow a limited set of ASCII characters, or any non-ASCII character.
/// See https://www.mongodb.com/docs/manual/reference/aggregation-variables/
pub fn escape_invalid_variable_chars(input: &str) -> String {
    let mut encoded = String::new();
    for char in input.chars() {
        match char {
            ESCAPE_CHAR => push_encoded_char(&mut encoded, ESCAPE_CHAR_ESCAPE_SEQUENCE),
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' => encoded.push(char),
            char if char as u32 <= 127 => push_encoded_char(&mut encoded, char as u32),
            char => encoded.push(char),
        }
    }
    encoded
}

/// Escape invalid characters using the escape character followed by a two-character hex sequence
/// that gives the character's ASCII codepoint
fn push_encoded_char(encoded: &mut String, char: u32) {
    encoded.push(ESCAPE_CHAR);
    let zero_pad = if char < 0x10 { "0" } else { "" };
    encoded.push_str(&format!("{zero_pad}{char:x}"));
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{escape_invalid_variable_chars, ESCAPE_CHAR, ESCAPE_CHAR_ESCAPE_SEQUENCE};

    proptest! {
        // Escaped strings must be consistent and distinct. A round-trip test demonstrates this.
        #[test]
        fn escaping_variable_chars_roundtrips(input in any::<String>()) {
            let encoded = escape_invalid_variable_chars(&input);
            let decoded = unescape_invalid_variable_chars(&encoded);
            prop_assert_eq!(decoded, input, "encoded string: {}", encoded)
        }
    }

    proptest! {
        // Escaped strings must be consistent and distinct. A round-trip test demonstrates this.
        #[test]
        fn escaped_variable_names_are_valid(input in any::<String>()) {
            let encoded = escape_invalid_variable_chars(&input);
            prop_assert!(
                encoded.chars().all(|char|
                    char as u32 > 127 ||
                        ('a'..='z').contains(&char) ||
                        ('A'..='Z').contains(&char) ||
                        ('0'..='9').contains(&char) ||
                        char == '_'
                ),
                "encoded string contains only valid characters\nencoded string: {}",
                encoded
            )
        }
    }

    fn unescape_invalid_variable_chars(input: &str) -> String {
        let mut decoded = String::new();
        let mut chars = input.chars();
        while let Some(char) = chars.next() {
            if char == ESCAPE_CHAR {
                let escape_sequence = [chars.next().unwrap(), chars.next().unwrap()];
                let code_point =
                    u32::from_str_radix(&escape_sequence.iter().collect::<String>(), 16).unwrap();
                if code_point == ESCAPE_CHAR_ESCAPE_SEQUENCE {
                    decoded.push(ESCAPE_CHAR)
                } else {
                    decoded.push(char::from_u32(code_point).unwrap())
                }
            } else {
                decoded.push(char)
            }
        }
        decoded
    }
}
