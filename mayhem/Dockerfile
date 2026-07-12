# syntax=docker/dockerfile:1.7
#
# Commit-image Dockerfile (SMALL form) — builds ON TOP of the org C/C++ base image
# (ghcr.io/mayhemheroes/base). webrtc-rs is a RUST workspace; the base ships no Rust
# toolchain, so this image installs rustup (nightly) + cargo-fuzz as root into a SHARED
# location (/opt/toolchains/rust) usable by the non-root `mayhem` user, then runs
# build.sh + test.sh.
#
# The fuzzed code lives in the `rtc` git submodule (rtc/rtc-sctp, rtc/rtc-sdp); the CI
# checkout uses submodules: recursive so the submodule tree is present in the build context.
#
# Build (from the repo root, on the `mayhem` branch):
#   DOCKER_BUILDKIT=1 docker build -f mayhem/Dockerfile \
#       --build-arg COMMIT=$(git rev-parse HEAD) \
#       [--build-arg SOURCE_DATE_EPOCH=$(date +%s)] -t webrtc-commit:test .

FROM ghcr.io/mayhemheroes/base:20260621

ARG COMMIT
ARG REPO_URL
# SOURCE_DATE_EPOCH stays an ARG (available to build.sh); never bake it as ENV — an empty value
# breaks clang. build.sh unsets it when empty.
ARG SOURCE_DATE_EPOCH

# Declared for parity: the base exports SANITIZER_FLAGS (ASan+UBSan, halting), but cargo-fuzz
# drives Rust instrumentation via RUSTFLAGS (-Zsanitizer=address), NOT $SANITIZER_FLAGS.
ARG SANITIZER_FLAGS
ARG MAYHEM_JOBS=

# Pin the nightly toolchain (cargo-fuzz needs nightly for -Zsanitizer=address; webrtc-rs is
# edition 2024, so the nightly must be recent enough). Pin cargo-fuzz too.
ARG RUST_TOOLCHAIN=nightly-2025-09-05
ARG CARGO_FUZZ_VERSION=0.12.0

# Rust toolchain at a FIXED, $HOME-independent prefix under /opt/toolchains/rust (SPEC §6.2 item 8),
# so the offline PATCH re-run of build.sh finds the same cargo registry cache under any identity.
ENV RUSTUP_HOME=/opt/toolchains/rust/rustup \
    CARGO_HOME=/opt/toolchains/rust/cargo \
    PATH=/opt/toolchains/rust/cargo/bin:${PATH}

USER root
RUN set -eux; \
    install -d /opt/toolchains/rust; \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path --profile minimal --default-toolchain "$RUST_TOOLCHAIN"; \
    rustup component add rust-src --toolchain "$RUST_TOOLCHAIN"; \
    cargo install cargo-fuzz --version "$CARGO_FUZZ_VERSION" --locked; \
    # Hand the whole prefix to mayhem (2000): the commit build AND the air-gapped PATCH re-run
    # both run as mayhem and need to read the toolchain + read/refresh the registry cache.
    chown -R 2000:2000 /opt/toolchains/rust; \
    rustc --version; cargo --version; cargo fuzz --version
USER mayhem

# Source enters from the build CONTEXT (the CI checkout): the FULL repo (incl. .git history and the
# rtc submodule tree) is copied to /mayhem as `mayhem`; build.sh builds in place.
WORKDIR /mayhem
COPY --chown=2000:2000 . /mayhem

# The fuzzed code lives in the `rtc` git submodule (rtc/rtc-sctp, rtc/rtc-sdp). Initialize it from
# the baked-in .git so the build does NOT depend on how the checkout fetched submodules: real CI uses
# `submodules: recursive` (rtc/ already populated — this is then a no-op), while verify-repo's
# CI-parity build clones HEAD without submodule CONTENTS (only the gitlink) — this step fills it in.
# Runs online at image-build time; the offline PATCH re-run of build.sh reuses the populated tree.
RUN git config --global --add safe.directory /mayhem \
    && git submodule update --init --recursive rtc

RUN bash mayhem/build.sh

# Run webrtc-rs' real test suite (rtc-sctp + rtc-sdp) at build time; a failing suite fails the image.
RUN if [ -f mayhem/test.sh ]; then bash mayhem/test.sh; else echo "no mayhem/test.sh"; fi

LABEL org.opencontainers.image.source="${REPO_URL}" \
      org.opencontainers.image.revision="${COMMIT}" \
      rlenv.tier="commit" \
      rlenv.source_date_epoch="${SOURCE_DATE_EPOCH}" \
      rlenv.user="mayhem"

USER mayhem
WORKDIR /mayhem
