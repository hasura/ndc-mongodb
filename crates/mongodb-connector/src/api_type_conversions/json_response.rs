use ndc_sdk::json_response as ndc_sdk;

/// Transform a [`dc_api::JsonResponse`] to a [`ndc_sdk::JsonResponse`] value **assuming
/// pre-serialized bytes do not need to be transformed**. The given mapping function will be used
/// to transform values that have not already been serialized, but serialized bytes will be
/// re-wrapped without modification.
#[allow(dead_code)] // TODO: MVC-7
pub fn map_unserialized<A, B, Fn>(
    input: dc_api::JsonResponse<A>,
    mapping: Fn,
) -> ndc_sdk::JsonResponse<B>
where
    Fn: FnOnce(A) -> B,
{
    match input {
        dc_api::JsonResponse::Value(value) => ndc_sdk::JsonResponse::Value(mapping(value)),
        dc_api::JsonResponse::Serialized(bytes) => ndc_sdk::JsonResponse::Serialized(bytes),
    }
}
