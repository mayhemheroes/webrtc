#!/usr/bin/env bash
#
# webrtc/mayhem/test.sh — RUN webrtc-rs' own test suite for the fuzzed crates and emit a CTRF
# summary. exit 0 iff no test failed. build.sh pre-compiled the suite (`cargo test --no-run`).
#
# Scope: the fuzz targets exercise the SCTP (rtc-sctp) and SDP (rtc-sdp) parsers, so this runs
# those crates' ENTIRE upstream test suites — real known-answer assertion tests that check exact
# parsed/marshalled values (rtc-sctp: chunk_test, param_test, packet round-trips, queue/association/
# endpoint; rtc-sdp: lexer + session/media unmarshal known-answer tests). These assert concrete
# values, so a no-op / "exit(0)" patch CANNOT pass — this is the PATCH-grade oracle for the code
# these targets fuzz.
#
# Skipped upstream tests: the sibling rtc-* crates (ice/dtls/turn/srtp/stun/mdns/...) and the outer
# `webrtc` integration tests (tests/*.rs) are peer-connection / ICE / DTLS interop tests that stand
# up live loopback transports + TLS and are not part of the fuzzed code path; they are out of scope
# for this oracle and not run at build time.
set -uo pipefail
[ -n "${SOURCE_DATE_EPOCH:-}" ] || unset SOURCE_DATE_EPOCH

: "${MAYHEM_JOBS:=$(nproc)}"
cd "$SRC"

# emit_ctrf <tool> <passed> <failed> [skipped] [pending] [other]
emit_ctrf() {
  local tool="$1" passed="$2" failed="$3" skipped="${4:-0}" pending="${5:-0}" other="${6:-0}"
  local tests=$(( passed + failed + skipped + pending + other ))
  cat > "${CTRF_REPORT:-$SRC/ctrf-report.json}" <<JSON
{
  "results": {
    "tool": { "name": "$tool" },
    "summary": {
      "tests": $tests,
      "passed": $passed,
      "failed": $failed,
      "pending": $pending,
      "skipped": $skipped,
      "other": $other
    }
  }
}
JSON
  printf 'CTRF {"results":{"tool":{"name":"%s"},"summary":{"tests":%d,"passed":%d,"failed":%d,"pending":%d,"skipped":%d,"other":%d}}}\n' \
    "$tool" "$tests" "$passed" "$failed" "$pending" "$skipped" "$other"
  [ "$failed" -eq 0 ]
}

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not available — cannot run the test suite" >&2
  emit_ctrf "cargo-test" 0 1 0; exit 2
fi

echo "=== running cargo test (rtc-sctp + rtc-sdp upstream suites) ==="
# Image's DEFAULT toolchain (pinned to the same nightly build.sh uses). --no-fail-fast to count
# every test; RUSTFLAGS cleared so it inherits nothing from the sanitizer build.
out="$(RUSTFLAGS="" cargo test --no-fail-fast --jobs "$MAYHEM_JOBS" \
  --manifest-path rtc/Cargo.toml -p rtc-sctp -p rtc-sdp 2>&1)"; rc=$?
echo "$out"

# Ported crash-artifact regression test (old fork's sctp/src/fuzz_artifact_test.rs): replay every
# committed fuzzer crash artifact through the SCTP unmarshalers and assert no crash.
out2="$(RUSTFLAGS="" cargo test --no-fail-fast --jobs "$MAYHEM_JOBS" \
  --manifest-path mayhem/fuzz/Cargo.toml 2>&1)"; rc2=$?
echo "$out2"
out="$out
$out2"
[ "$rc2" -eq 0 ] || rc=$rc2

# libtest prints one line per test binary:
#   test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; ...
PASSED=0; FAILED=0; IGNORED=0
while read -r p f i; do
  PASSED=$(( PASSED + p )); FAILED=$(( FAILED + f )); IGNORED=$(( IGNORED + i ))
done < <(printf '%s\n' "$out" \
  | sed -n 's/^test result:.* \([0-9][0-9]*\) passed; \([0-9][0-9]*\) failed; \([0-9][0-9]*\) ignored.*/\1 \2 \3/p')

# If we parsed no result lines, fall back to the cargo exit code (e.g. compile error).
if [ "$(( PASSED + FAILED + IGNORED ))" -eq 0 ]; then
  echo "could not parse any 'test result:' lines; using cargo exit code $rc" >&2
  [ "$rc" -eq 0 ] && { emit_ctrf "cargo-test" 1 0 0; exit 0; }
  emit_ctrf "cargo-test" 0 1 0; exit 1
fi

emit_ctrf "cargo-test" "$PASSED" "$FAILED" "$IGNORED"
