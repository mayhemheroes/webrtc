#!/usr/bin/env bash
#
# webrtc/mayhem/build.sh — build webrtc-rs' SCTP + SDP cargo-fuzz targets as sanitized
# libFuzzer binaries (OSS-Fuzz Rust path: cargo-fuzz + ASan via RUSTFLAGS).
#
# Post-monorepo layout: the fuzzed code lives in the `rtc` git submodule
# (rtc/rtc-sctp, rtc/rtc-sdp). Upstream ships no fuzz crate anymore, so the cargo-fuzz
# crate is ADDITIVE under mayhem/fuzz/ and depends on those crates by path — upstream
# stays untouched. Targets (names preserved from the old fork for run-history parity):
#   packet         — rtc_sctp::fuzzing::packet_unmarshal   (full SCTP packet parse)
#   param          — every rtc_sctp::fuzzing::param_*_unmarshal (SCTP parameter parse)
#   parse_session  — rtc_sdp::SessionDescription::unmarshal (SDP session parse)
# The Mayhemfile target for parse_session is `parse-session` (slug) as before.
set -euo pipefail

# clang rejects SOURCE_DATE_EPOCH='' — must be unset or a valid integer (kept for parity;
# cargo's cc-built deps may invoke clang).
[ -n "${SOURCE_DATE_EPOCH:-}" ] || unset SOURCE_DATE_EPOCH

: "${MAYHEM_JOBS:=$(nproc)}"
export MAYHEM_JOBS
# cargo-fuzz has no --jobs flag; cargo reads parallelism from CARGO_BUILD_JOBS.
export CARGO_BUILD_JOBS="$MAYHEM_JOBS"

# DWARF < 4 debug-info contract (§6.2 item 10). Force DWARF 2 so Mayhem triage / gdb
# can resolve project source lines. The rlenv runtime may export RUST_DEBUG_FLAGS before
# re-running build.sh offline; the `:-` default only applies when unset/empty.
: "${RUST_DEBUG_FLAGS:=-C debuginfo=2 -C force-frame-pointers=yes -C llvm-args=--dwarf-version=2}"

cd "$SRC"

# ── DWARF < 4 enforcement (§6.2 item 10) ────────────────────────────────────────────────────────
# Rust's ASan runtime (librustc-nightly_rt.asan.a) is compiled with the nightly's bundled LLVM,
# which defaults to DWARF 5, and is linked BEFORE project code — so the first .debug_info CU would
# be DWARF 5 and fail verify-repo. Strip the ASan archive's debug sections once; the stripped .a is
# baked into the image so the offline PATCH re-run sees the same file.
ASAN_RT="$(find "$RUSTUP_HOME/toolchains" -name "librustc-nightly_rt.asan.a" 2>/dev/null | head -1)"
if [ -n "$ASAN_RT" ] && [ -f "$ASAN_RT" ]; then
    echo "Stripping debug info from Rust ASan runtime to enforce DWARF < 4: $ASAN_RT"
    objcopy --strip-debug "$ASAN_RT" || true
fi

# libfuzzer-sys compiles libFuzzer from C++ via the cc crate; force DWARF 3 so those CUs also
# satisfy the check. On the re-run these flags are identical, so cargo reuses the cached libfuzzer.a.
export CFLAGS="${CFLAGS:+$CFLAGS }-gdwarf-3"
export CXXFLAGS="${CXXFLAGS:+$CXXFLAGS }-gdwarf-3"

FUZZ_DIR="mayhem/fuzz"
FUZZ_TARGETS=(packet param parse_session)
TRIPLE="x86_64-unknown-linux-gnu"

# The base image exports SANITIZER_FLAGS (clang ASan+UBSan, halting) as the C/C++ default, but
# cargo-fuzz drives RUST instrumentation via RUSTFLAGS, not clang's $SANITIZER_FLAGS. Honor
# SANITIZER_FLAGS as the on/off + which-sanitizer control so an explicit `--build-arg
# SANITIZER_FLAGS=` yields a natural-crash (uninstrumented) build, matching the C/C++ contract.
# Default to ASan (the OSS-Fuzz Rust sanitizer) when unset.
: "${SANITIZER_FLAGS=-fsanitize=address}"
RUST_SAN=""
case "$SANITIZER_FLAGS" in
  *address*) RUST_SAN="$RUST_SAN -Zsanitizer=address" ;;
esac
export SANITIZER_FLAGS

# OSS-Fuzz Rust libFuzzer+ASan flags. `--cfg fuzzing` matches libfuzzer-sys. RUST_DEBUG_FLAGS adds
# DWARF ≤ 2 debug info; with the stripped ASan runtime this keeps the first .debug_info CU < 4.
export RUSTFLAGS="${RUSTFLAGS:-} --cfg fuzzing ${RUST_SAN} ${RUST_DEBUG_FLAGS}"

echo "=== cargo fuzz build (image-default nightly toolchain, ASan via RUSTFLAGS) ==="
echo "RUSTFLAGS=$RUSTFLAGS"

# `-O` + `--debug-assertions` mirrors OSS-Fuzz's build.sh. Use the image's DEFAULT toolchain
# (the Dockerfile pins the required nightly); a `+toolchain` override would try to install another
# channel into the shared /opt/toolchains/rust.
for t in "${FUZZ_TARGETS[@]}"; do
  echo "--- building fuzz target: $t ---"
  cargo fuzz build --fuzz-dir "$FUZZ_DIR" -O --debug-assertions "$t"
done

# Resolve the cargo target dir robustly via `cargo metadata`.
TARGET_DIR="$(cargo metadata --no-deps --format-version 1 --manifest-path "$FUZZ_DIR/Cargo.toml" \
  | python3 -c 'import json,sys;print(json.load(sys.stdin)["target_directory"])')"
echo "fuzz target_directory: $TARGET_DIR"

REL="$TARGET_DIR/$TRIPLE/release"
for t in "${FUZZ_TARGETS[@]}"; do
  bin="$REL/$t"
  if [ ! -x "$bin" ]; then
    echo "ERROR: expected fuzz binary not found at $bin" >&2
    ls -la "$REL" >&2 || true
    exit 1
  fi
  cp "$bin" "/mayhem/$t"
  echo "built /mayhem/$t"
done

# Build the fuzzed crates' TEST suite too — with normal flags (no sanitizer RUSTFLAGS, separate
# default target dir under rtc/) — so mayhem/test.sh only RUNS it, never compiles. rtc-sctp/rtc-sdp
# carry webrtc-rs' real known-answer parser/marshal assertion tests (the code these targets fuzz).
echo "=== cargo test --no-run for rtc-sctp + rtc-sdp (normal flags, pre-building the suite) ==="
RUSTFLAGS="" cargo test --no-run --jobs "$MAYHEM_JOBS" \
  --manifest-path rtc/Cargo.toml -p rtc-sctp -p rtc-sdp

# Also pre-build the ported crash-artifact regression test (mayhem/fuzz/tests/fuzz_artifact_test.rs,
# from the old fork's sctp/src/fuzz_artifact_test.rs) with normal flags.
RUSTFLAGS="" cargo test --no-run --jobs "$MAYHEM_JOBS" --manifest-path "$FUZZ_DIR/Cargo.toml"

echo "build.sh complete:"
ls -la /mayhem/packet /mayhem/param /mayhem/parse_session 2>&1 || true
