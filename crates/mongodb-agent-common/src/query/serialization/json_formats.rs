//! Types defined just to get serialization logic for BSON "scalar" types that are represented in
//! JSON as composite structures. The types here are designed to match the representations of BSON
//! types in extjson.

use mongodb::bson::{self, Bson};
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, hex::Hex, serde_as};

#[serde_as]
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinData {
    #[serde_as(as = "Base64")]
    base64: Vec<u8>,
    #[serde_as(as = "Hex")]
    sub_type: [u8; 1],
}

impl From<BinData> for Bson {
    fn from(value: BinData) -> Self {
        Bson::Binary(bson::Binary {
            bytes: value.base64,
            subtype: value.sub_type[0].into(),
        })
    }
}

impl From<bson::Binary> for BinData {
    fn from(value: bson::Binary) -> Self {
        BinData {
            base64: value.bytes,
            sub_type: [value.subtype.into()],
        }
    }
}

#[derive(Deserialize)]
pub struct JavaScriptCodeWithScope {
    #[serde(rename = "$code")]
    code: String,
    #[serde(rename = "$scope")]
    scope: bson::Document, // TODO: serialize as extjson!
}

impl From<JavaScriptCodeWithScope> for Bson {
    fn from(value: JavaScriptCodeWithScope) -> Self {
        Bson::JavaScriptCodeWithScope(bson::JavaScriptCodeWithScope {
            code: value.code,
            scope: value.scope,
        })
    }
}

impl From<bson::JavaScriptCodeWithScope> for JavaScriptCodeWithScope {
    fn from(value: bson::JavaScriptCodeWithScope) -> Self {
        JavaScriptCodeWithScope {
            code: value.code,
            scope: value.scope,
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct Regex {
    pattern: String,
    options: String,
}

impl From<Regex> for Bson {
    fn from(value: Regex) -> Self {
        Bson::RegularExpression(bson::Regex {
            pattern: value.pattern,
            options: value.options,
        })
    }
}

impl From<bson::Regex> for Regex {
    fn from(value: bson::Regex) -> Self {
        Regex {
            pattern: value.pattern,
            options: value.options,
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct Timestamp {
    t: u32,
    i: u32,
}

impl From<Timestamp> for Bson {
    fn from(value: Timestamp) -> Self {
        Bson::Timestamp(bson::Timestamp {
            time: value.t,
            increment: value.i,
        })
    }
}

impl From<bson::Timestamp> for Timestamp {
    fn from(value: bson::Timestamp) -> Self {
        Timestamp {
            t: value.time,
            i: value.increment,
        }
    }
}
