use assert_json::{Error, Validator};
use serde_json::Value;

pub fn non_empty_array() -> NonEmptyArrayValidator {
    NonEmptyArrayValidator
}

pub struct NonEmptyArrayValidator;

impl Validator for NonEmptyArrayValidator {
    fn validate<'a>(&self, value: &'a Value) -> Result<(), Error<'a>> {
        if let Value::Array(xs) = value {
            if xs.is_empty() {
                Err(Error::InvalidValue(value, "non-empty array".to_string()))
            } else {
                Ok(())
            }
        } else {
            Err(Error::InvalidType(value, "array".to_string()))
        }
    }
}
