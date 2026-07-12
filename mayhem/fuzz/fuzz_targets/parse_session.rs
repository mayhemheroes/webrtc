#![no_main]
use libfuzzer_sys::fuzz_target;

// SDP session parsing: SessionDescription::unmarshal over the raw input. Mirrors the
// old fork's `parse_session` target (sdp::SessionDescription::unmarshal), re-pointed
// at the post-monorepo rtc-sdp crate.
fuzz_target!(|data: &[u8]| {
    let mut cursor = std::io::Cursor::new(data);
    let _ = rtc_sdp::SessionDescription::unmarshal(&mut cursor);
});
