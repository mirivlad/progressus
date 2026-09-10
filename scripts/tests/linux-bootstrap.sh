#!/usr/bin/env bash
# No host package installation: every external command uses an isolated fake PATH.
set -euo pipefail
bootstrap="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)/setup-linux.sh"
sandbox=$(mktemp -d)
trap 'rm -rf -- "$sandbox"' EXIT
export BOOTSTRAP_LOG="$sandbox/log" BOOTSTRAP_READY="$sandbox/ready"
mkdir "$sandbox/bin"
cat > "$sandbox/bin/id" <<'SH'
#!/bin/bash
echo 1000
SH
cat > "$sandbox/bin/sudo" <<'SH'
#!/bin/bash
printf 'sudo %s\n' "$*" >> "$BOOTSTRAP_LOG"
exec "$@"
SH
cat > "$sandbox/bin/pkg-config" <<'SH'
#!/bin/bash
[[ -f "$BOOTSTRAP_READY" ]]
SH
cat > "$sandbox/bin/apt-get" <<'SH'
#!/bin/bash
printf 'apt-get %s\n' "$*" >> "$BOOTSTRAP_LOG"
if [[ "$1" == install ]]; then : > "$BOOTSTRAP_READY"; fi
SH
chmod +x "$sandbox/bin/"*
PATH="$sandbox/bin" /bin/bash "$bootstrap"
[[ $(cat "$BOOTSTRAP_LOG") == $'sudo apt-get update\napt-get update\nsudo apt-get install -y pkg-config libasound2-dev\napt-get install -y pkg-config libasound2-dev' ]]
cp "$BOOTSTRAP_LOG" "$sandbox/before"
PATH="$sandbox/bin" /bin/bash "$bootstrap"
cmp "$BOOTSTRAP_LOG" "$sandbox/before"
rm "$BOOTSTRAP_READY"
cat > "$sandbox/bin/apt-get" <<'SH'
#!/bin/bash
exit 17
SH
if PATH="$sandbox/bin" /bin/bash "$bootstrap"; then
    echo 'Failed package-manager action must stop setup' >&2
    exit 1
fi
[[ ! -f "$BOOTSTRAP_READY" ]]
printf '%s\n' 'Linux dependency bootstrap tests passed (installation, no-op, failure).'
