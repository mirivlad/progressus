#!/usr/bin/env bash
set -euo pipefail

boundary_tree="$(cargo tree -p progressus-headless --edges normal,build,dev --prefix none)"

if grep -Eiq '^bevy([_-]|[[:space:]]|$)' <<<"${boundary_tree}"; then
    echo "error: Bevy dependency found in the headless application chain" >&2
    echo "${boundary_tree}" >&2
    exit 1
fi

echo "headless dependency boundary: no Bevy packages in the application chain"
