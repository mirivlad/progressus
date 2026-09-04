#!/usr/bin/env bash
set -euo pipefail

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
cargo run --release -q -p progressus-sim --example prototype01_benchmark
