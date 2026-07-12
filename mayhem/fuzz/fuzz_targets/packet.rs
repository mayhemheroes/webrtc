#![no_main]
use libfuzzer_sys::fuzz_target;

// SCTP packet parsing: unmarshal a raw SCTP packet (parses the common header, every
// chunk, and their embedded params). Mirrors the old fork's `packet` target
// (webrtc_sctp::packet::fuzz_packet_unmarshal), re-pointed at the post-monorepo
// rtc-sctp crate's public fuzzing shim.
fuzz_target!(|data: &[u8]| {
    let _ = rtc_sctp::fuzzing::packet_unmarshal(data);
});
