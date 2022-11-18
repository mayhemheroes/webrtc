#![no_main]
use libfuzzer_sys::fuzz_target;

use webrtc_sctp::param::fuzz_build_param;
use bytes::Bytes;

fuzz_target!(|data: &[u8]| {
    let bytes = Bytes::from(data.to_vec());
    fuzz_build_param(&bytes);
});
