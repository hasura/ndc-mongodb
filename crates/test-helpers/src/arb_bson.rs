use std::time::SystemTime;

use mongodb::bson::{self, oid::ObjectId, Bson};
use proptest::{collection, prelude::*, sample::SizeRange};

pub fn arb_bson() -> impl Strategy<Value = Bson> {
    let leaf = prop_oneof![
        Just(Bson::Null),
        Just(Bson::Undefined),
        Just(Bson::MaxKey),
        Just(Bson::MinKey),
        any::<bool>().prop_map(Bson::Boolean),
        any::<i32>().prop_map(Bson::Int32),
        any::<i64>().prop_map(Bson::Int64),
        any::<f64>().prop_map(Bson::Double),
        any::<SystemTime>().prop_map(|t| Bson::DateTime(bson::DateTime::from_system_time(t))),
        arb_object_id().prop_map(Bson::ObjectId),
        any::<String>().prop_map(Bson::String),
        any::<String>().prop_map(Bson::Symbol),
        any::<[u8; 128 / 8]>().prop_map(|b| Bson::Decimal128(bson::Decimal128::from_bytes(b))),
        any::<String>().prop_map(Bson::JavaScriptCode),
        (any::<u32>(), any::<u32>())
            .prop_map(|(time, increment)| Bson::Timestamp(bson::Timestamp { time, increment })),
        arb_binary().prop_map(Bson::Binary),
        (".*", "i?l?m?s?u?x?").prop_map(|(pattern, options)| Bson::RegularExpression(
            bson::Regex { pattern, options }
        )),
        // skipped DbPointer because it is deprecated, and does not have a public constructor
    ];
    leaf.prop_recursive(
        8,   // 8 levels deep
        256, // aim for maximum of 256 nodes
        10,  // branch factor: we have up to this many elements per array, or fields per document
        |inner| {
            prop_oneof![
                collection::vec(inner.clone(), 0..10).prop_map(Bson::Array),
                arb_bson_document_recursive(inner.clone(), 0..10).prop_map(Bson::Document),
                (any::<String>(), arb_bson_document_recursive(inner, 0..10)).prop_map(
                    |(code, scope)| {
                        Bson::JavaScriptCodeWithScope(bson::JavaScriptCodeWithScope { code, scope })
                    }
                ),
            ]
        },
    )
}

pub fn arb_bson_document(size: impl Into<SizeRange>) -> impl Strategy<Value = bson::Document> {
    arb_bson_document_recursive(arb_bson(), size)
}

fn arb_bson_document_recursive(
    value: impl Strategy<Value = Bson>,
    size: impl Into<SizeRange>,
) -> impl Strategy<Value = bson::Document> {
    collection::btree_map(".*", value, size).prop_map(|fields| fields.into_iter().collect())
}

fn arb_binary() -> impl Strategy<Value = bson::Binary> {
    let binary_subtype = any::<u8>().prop_map(Into::into);
    let bytes = collection::vec(any::<u8>(), 1..256);
    (binary_subtype, bytes).prop_map(|(subtype, bytes)| bson::Binary { subtype, bytes })
}

fn arb_object_id() -> impl Strategy<Value = ObjectId> {
    any::<[u8; 12]>().prop_map(Into::into)
}

