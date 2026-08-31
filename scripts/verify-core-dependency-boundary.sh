#!/usr/bin/env bash
set -euo pipefail

boundary_tree="$(cargo tree -p progressus-app --edges normal,build,dev --prefix none)"

if grep -Eiq '^bevy([_-]|[[:space:]]|$)' <<<"${boundary_tree}"; then
    echo "error: Bevy dependency found below the progressus-app boundary" >&2
    echo "${boundary_tree}" >&2
    exit 1
fi

echo "core dependency boundary: no Bevy packages below progressus-app"
