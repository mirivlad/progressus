#!/usr/bin/env bash
set -euo pipefail

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"

printf '%s\n' '== format =='
cargo fmt --all -- --check

printf '%s\n' '== clippy authoritative/headless =='
cargo clippy \
  -p progressus-worldgen \
  -p progressus-sim \
  -p progressus-app \
  -p progressus-headless \
  --all-targets -- -D warnings

printf '%s\n' '== client compile/clippy (no executable link) =='
cargo check -p progressus-client --all-targets
cargo clippy -p progressus-client --all-targets -- -D warnings

printf '%s\n' '== automated tests =='
cargo test \
  -p progressus-worldgen \
  -p progressus-sim \
  -p progressus-app \
  -p progressus-headless

if [[ "${PROGRESSUS_RUN_CLIENT_TESTS:-0}" == "1" ]]; then
  printf '%s\n' '== client tests (explicit heavy link) =='
  cargo test -p progressus-client
else
  printf '%s\n' '== client tests =='
  printf '%s\n' 'typechecked above; set PROGRESSUS_RUN_CLIENT_TESTS=1 to link/run them'
fi

printf '%s\n' '== dependency boundaries =='
./scripts/verify-core-dependency-boundary.sh

printf '%s\n' '== 100k idle/residency smoke =='
cargo run -q -p progressus-headless -- --seed 42 --ticks 100000

printf '%s\n' '== travel64 streaming smoke =='
cargo run -q -p progressus-headless -- --seed 0 --travel-chunks 64

printf '%s\n' '== 100k activity/persistence smoke =='
cargo run -q -p progressus-headless -- --seed 0 --activity-smoke
