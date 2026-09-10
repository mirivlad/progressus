#!/usr/bin/env bash
# First-run entry point: install missing native build dependencies, then run Cargo.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
if [[ "$(uname -s)" == Linux ]]; then
    "./scripts/setup-linux.sh"
fi
exec cargo run -p progressus-client -- "$@"
