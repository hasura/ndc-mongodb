use mongodb::bson::Bson;
use serde_json::Value;

/// Converts BSON values to JSON.
///
/// The BSON library already has a `Serialize` impl that can convert to JSON. But that
/// implementation emits Extended JSON which includes inline type tags in JSON output to
/// disambiguate types on the BSON side. We don't want those tags because we communicate type
/// information out of band.
pub fn bson_to_json(value: &Bson) -> anyhow::Result<Value> {
    Ok(Value::String("hello".to_owned()))
}
