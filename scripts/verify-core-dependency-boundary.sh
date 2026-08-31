#!/usr/bin/env bash
set -euo pipefail

boundary_tree="$(cargo tree -p progressus-headless --edges normal,build,dev --prefix none)"
client_direct_tree="$(cargo tree -p progressus-client --depth 1 --edges normal,build,dev --prefix none)"

if grep -Eiq '^bevy([_-]|[[:space:]]|$)' <<<"${boundary_tree}"; then
    echo "error: Bevy dependency found in the headless application chain" >&2
    echo "${boundary_tree}" >&2
    exit 1
fi

if ! grep -Eq '^bevy v' <<<"${client_direct_tree}"; then
    echo "error: progressus-client must depend on Bevy" >&2
    exit 1
fi

if ! grep -Eq '^progressus-app v' <<<"${client_direct_tree}"; then
    echo "error: progressus-client must depend on progressus-app" >&2
    exit 1
fi

if grep -Eq '^progressus-(sim|worldgen) v' <<<"${client_direct_tree}"; then
    echo "error: progressus-client directly depends on a lower authoritative crate" >&2
    echo "${client_direct_tree}" >&2
    exit 1
fi

if grep -Evq '^(progressus-client|bevy|progressus-app) v' <<<"${client_direct_tree}"; then
    echo "error: progressus-client direct dependencies must be Bevy and progressus-app only" >&2
    echo "${client_direct_tree}" >&2
    exit 1
fi

echo "headless dependency boundary: no Bevy packages in the application chain"
echo "client dependency boundary: direct dependencies are Bevy and progressus-app only"
