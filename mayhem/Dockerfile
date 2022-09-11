# Build Stage
FROM ghcr.io/evanrichter/cargo-fuzz:latest as builder

## Add source code to the build stage.
ADD . /src
WORKDIR /src

RUN echo building instrumented harnesses && \
    bash -c "pushd sdp/fuzz && cargo +nightly -Z sparse-registry fuzz build && popd" && \
    bash -c "pushd sctp/fuzz && cargo +nightly -Z sparse-registry fuzz build && popd" && \
    mv sdp/fuzz/target/x86_64-unknown-linux-gnu/release/parse_session /parse_session && \
    mv sctp/fuzz/target/x86_64-unknown-linux-gnu/release/packet /packet && \
    mv sctp/fuzz/target/x86_64-unknown-linux-gnu/release/param /param && \
    echo done

RUN echo building non-instrumented harnesses && \
    export RUSTFLAGS="--cfg fuzzing -Clink-dead-code -Cdebug-assertions -C codegen-units=1" && \
    bash -c "pushd sdp/fuzz && cargo +nightly -Z sparse-registry build --release && popd" && \
    bash -c "pushd sctp/fuzz && cargo +nightly -Z sparse-registry build --release && popd" && \
    mv sdp/fuzz/target/release/parse_session /parse_session_no_inst && \
    mv sctp/fuzz/target/release/packet /packet_no_inst && \
    mv sctp/fuzz/target/release/param /param_no_inst && \
    echo done

# Package Stage
FROM rustlang/rust:nightly

COPY --from=builder /parse_session /parse_session_no_inst /
COPY --from=builder /packet /packet_no_inst /
COPY --from=builder /param /param_no_inst /
