#!/usr/bin/env bash

set -ex

# Incorporate TARGET env var to the build and test process
if [[ $TARGET == x86_64-unknown-linux-musl ]]; then
    # TODO: run test after build
    docker run -v "$PWD:/volume" "clux/muslrust:$DOCKER_RUST_VERSION" cargo build --target "$TARGET" --release
else
    # We cannot run arm executables on linux
    if [[ $TARGET != arm-unknown-linux-* ]]; then
        cargo test --target "$TARGET"
    fi
    cargo build --target "$TARGET" --release
fi

cargo install cargo-strip && cargo strip
