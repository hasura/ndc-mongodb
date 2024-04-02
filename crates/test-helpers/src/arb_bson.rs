use std::time::SystemTime;

use mongodb::bson::{self, oid::ObjectId, Bson};
use proptest::{collection, prelude::*, sample::SizeRange};

pub fn arb_bson() -> impl Strategy<Value = Bson> {
    arb_bson_with_options(Default::default())
}

#[derive(Clone, Debug)]
pub struct ArbBsonOptions {
    /// max AST depth of generated values
    pub depth: u32,

    /// number of AST nodes to target
    pub desired_size: u32,

    /// minimum and maximum number of elements per array, or fields per document
    pub branch_range: SizeRange,

    /// If set to false arrays are generated such that all elements have a uniform type according
    /// to `type_unification` in the introspection crate. Note that we consider "nullable" a valid
    /// type, so array elements will sometimes be null even if this is set to true.
    pub heterogeneous_arrays: bool,
}

impl Default for ArbBsonOptions {
    fn default() -> Self {
        Self {
            depth: 8,
            desired_size: 256,
            branch_range: (0, 10).into(),
            heterogeneous_arrays: true,
        }
    }
}

pub fn arb_bson_with_options(options: ArbBsonOptions) -> impl Strategy<Value = Bson> {
    let leaf = prop_oneof![
        Just(Bson::Null),
        Just(Bson::Undefined),
        Just(Bson::MaxKey),
        Just(Bson::MinKey),
        any::<bool>().prop_map(Bson::Boolean),
        any::<i32>().prop_map(Bson::Int32),
        any::<i64>().prop_map(Bson::Int64),
        any::<f64>().prop_map(Bson::Double),
        arb_datetime().prop_map(Bson::DateTime),
        arb_object_id().prop_map(Bson::ObjectId),
        any::<String>().prop_map(Bson::String),
        any::<String>().prop_map(Bson::Symbol),
        arb_decimal().prop_map(Bson::Decimal128),
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
        options.depth,
        options.desired_size,
        options.branch_range.end_incl().try_into().unwrap(),
        move |inner| {
            prop_oneof![
                arb_bson_array_recursive(inner.clone(), options.clone()).prop_map(Bson::Array),
                arb_bson_document_recursive(inner.clone(), options.branch_range.clone())
                    .prop_map(Bson::Document),
                (
                    any::<String>(),
                    arb_bson_document_recursive(inner, options.branch_range.clone())
                )
                    .prop_map(|(code, scope)| {
                        Bson::JavaScriptCodeWithScope(bson::JavaScriptCodeWithScope { code, scope })
                    }),
            ]
        },
    )
}

fn arb_bson_array_recursive(
    value: impl Strategy<Value = Bson> + 'static,
    options: ArbBsonOptions,
) -> impl Strategy<Value = Vec<Bson>> {
    if options.heterogeneous_arrays {
        collection::vec(value, options.branch_range).boxed()
    } else {
        // To make sure the array is homogeneously-typed generate one arbitrary BSON value and
        // replicate it. But we still want a chance to include null values because we can unify
        // those into a non-Any type. So each array element has a 10% chance to be null instead of
        // the generated value.
        (
            value,
            collection::vec(proptest::bool::weighted(0.9), options.branch_range),
        )
            .prop_map(|(value, non_nulls)| {
                non_nulls
                    .into_iter()
                    .map(|non_null| if non_null { value.clone() } else { Bson::Null })
                    .collect()
            })
            .boxed()
    }
}

pub fn arb_bson_document(size: impl Into<SizeRange>) -> impl Strategy<Value = bson::Document> {
    arb_bson_document_recursive(arb_bson(), size)
}

fn arb_bson_document_recursive(
    value: impl Strategy<Value = Bson>,
    size: impl Into<SizeRange>,
) -> impl Strategy<Value = bson::Document> {
    collection::btree_map(".+", value, size).prop_map(|fields| fields.into_iter().collect())
}

fn arb_binary() -> impl Strategy<Value = bson::Binary> {
    let binary_subtype = any::<u8>().prop_map(Into::into);
    let bytes = collection::vec(any::<u8>(), 1..256);
    (binary_subtype, bytes).prop_map(|(subtype, bytes)| bson::Binary { subtype, bytes })
}

pub fn arb_datetime() -> impl Strategy<Value = bson::DateTime> {
    any::<SystemTime>().prop_map(bson::DateTime::from_system_time)
}

// Generate bytes for a 128-bit decimal, and convert to a string and back to normalize. This does
// not produce a uniform probability distribution over decimal values so it would not make a good
// random number generator. But it is useful for testing serialization.
fn arb_decimal() -> impl Strategy<Value = bson::Decimal128> {
    any::<[u8; 128 / 8]>().prop_map(|bytes| {
        let raw_decimal = bson::Decimal128::from_bytes(bytes);
        raw_decimal.to_string().parse().unwrap()
    })
}

fn arb_object_id() -> impl Strategy<Value = ObjectId> {
    any::<[u8; 12]>().prop_map(Into::into)
}
