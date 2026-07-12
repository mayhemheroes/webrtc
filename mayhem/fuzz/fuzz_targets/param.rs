#![no_main]
use libfuzzer_sys::fuzz_target;

// SCTP parameter parsing. The old fork's `param` target drove the crate-internal
// `build_param` type dispatch (webrtc_sctp::param::fuzz_build_param). After the
// monorepo merge `build_param` is `pub(crate)`, so we drive the same code paths
// through rtc-sctp's public `fuzzing` shim: every concrete parameter unmarshaller
// (the exact set `build_param` dispatches to, plus the shared param header). Feeding
// one input to all of them is a strict superset of the single-type dispatch.
use rtc_sctp::fuzzing::{
    param_param_chunk_list_unmarshal, param_param_forward_tsn_supported_unmarshal,
    param_param_header_unmarshal, param_param_heartbeat_info_unmarshal,
    param_param_outgoing_reset_request_unmarshal, param_param_random_unmarshal,
    param_param_reconfig_response_unmarshal, param_param_requested_hmac_algorithm_unmarshal,
    param_param_state_cookie_unmarshal, param_param_supported_extensions_unmarshal,
    param_param_unknown_unmarshal,
};

fuzz_target!(|data: &[u8]| {
    let _ = param_param_header_unmarshal(data);
    let _ = param_param_forward_tsn_supported_unmarshal(data);
    let _ = param_param_supported_extensions_unmarshal(data);
    let _ = param_param_random_unmarshal(data);
    let _ = param_param_requested_hmac_algorithm_unmarshal(data);
    let _ = param_param_chunk_list_unmarshal(data);
    let _ = param_param_state_cookie_unmarshal(data);
    let _ = param_param_heartbeat_info_unmarshal(data);
    let _ = param_param_outgoing_reset_request_unmarshal(data);
    let _ = param_param_reconfig_response_unmarshal(data);
    let _ = param_param_unknown_unmarshal(data);
});
