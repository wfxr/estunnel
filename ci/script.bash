#!/usr/bin/env bash

set -ex

# Incorporate TARGET env var to the build and test process
cargo build --target "$TARGET" --verbose
