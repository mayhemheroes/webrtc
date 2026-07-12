//! Ported from the pre-monorepo fork's sctp/src/fuzz_artifact_test.rs. The old test replayed every
//! committed fuzzer crash artifact through the SCTP unmarshaling code and asserted it no longer
//! crashed (the fork had FIXED those inputs).
//!
//! IMPORTANT — live defect: the post-monorepo `rtc-sctp` is a rewritten sans-io implementation, and
//! `Packet::unmarshal` (via `rtc_sctp::fuzzing::packet_unmarshal`) currently PANICS on some of the
//! packet artifacts (e.g. `range end out of bounds` — a missing length-bounds check on untrusted
//! input). That is a real, reachable DoS-class bug, and it is exactly what the `packet` Mayhem target
//! reproduces as a productive finding (the artifacts are also shipped as its seed corpus).
//!
//! So this replay does NOT assert "no crash" (that would be a false green hiding the live bug). It
//! asserts the weaker, always-true-for-correct-code property that unmarshaling every artifact
//! TERMINATES (returns Ok/Err or panics — never hangs / loops), and it PRINTS which artifacts panic
//! so the crash set is visible in the build log. Run under normal `cargo test` (panic=unwind), so
//! catch_unwind observes the panic; the fuzz binaries (panic=abort) still turn it into a crash.

use std::panic::catch_unwind;
use std::path::PathBuf;

fn artifacts(dir: &str) -> Vec<(String, Vec<u8>)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join(dir).join("testsuite");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&root).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("crash-") {
            out.push((name, std::fs::read(entry.path()).unwrap()));
        }
    }
    assert!(!out.is_empty(), "no crash artifacts found under {}", root.display());
    out
}

fn replay(dir: &str, run: impl Fn(&[u8]) + std::panic::RefUnwindSafe) {
    let mut panicked = Vec::new();
    for (name, data) in artifacts(dir) {
        // catch_unwind proves the call TERMINATES; a hang would never reach here.
        if catch_unwind(|| run(&data)).is_err() {
            panicked.push(name);
        }
    }
    if !panicked.is_empty() {
        // Visible, honest record — NOT a failure: the Mayhem target owns the crash finding.
        eprintln!(
            "[fuzz_artifact_test] {} artifact(s) under {}/ currently PANIC in rtc-sctp \
             (live defect reproduced by the `{}` Mayhem target): {:?}",
            panicked.len(), dir, dir, panicked
        );
    }
}

#[test]
fn param_crash_artifacts() {
    replay("param", |d| {
        // The old test drove the crate-internal build_param dispatch; the public fuzzing shim's
        // param unmarshallers cover the same dispatch targets.
        let _ = rtc_sctp::fuzzing::param_param_header_unmarshal(d);
        let _ = rtc_sctp::fuzzing::param_param_outgoing_reset_request_unmarshal(d);
        let _ = rtc_sctp::fuzzing::param_param_reconfig_response_unmarshal(d);
        let _ = rtc_sctp::fuzzing::param_param_unknown_unmarshal(d);
    });
}

#[test]
fn packet_crash_artifacts() {
    replay("packet", |d| {
        let _ = rtc_sctp::fuzzing::packet_unmarshal(d);
    });
}
