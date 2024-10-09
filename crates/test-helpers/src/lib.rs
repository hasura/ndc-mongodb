pub mod arb_bson;
mod arb_plan_type;
pub mod arb_type;
pub mod configuration;

use enum_iterator::Sequence as _;
use mongodb_support::ExtendedJsonMode;
use proptest::prelude::*;

pub use arb_bson::{arb_bson, arb_bson_with_options, ArbBsonOptions};
pub use arb_plan_type::arb_plan_type;
pub use arb_type::arb_type;

pub fn arb_extended_json_mode() -> impl Strategy<Value = ExtendedJsonMode> {
    (0..ExtendedJsonMode::CARDINALITY)
        .prop_map(|n| enum_iterator::all::<ExtendedJsonMode>().nth(n).unwrap())
}
