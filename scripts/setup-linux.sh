#!/usr/bin/env bash
set -euo pipefail
# ALSA is the Linux audio build interface even on PipeWire/PulseAudio desktops.
if command -v pkg-config >/dev/null && pkg-config --exists alsa; then
    exit 0
fi
printf '%s\n' 'Progressus: installing missing Linux audio build dependencies.'
privilege=()
if [[ "$(id -u)" != 0 ]]; then
    if ! command -v sudo >/dev/null; then
        printf '%s\n' 'sudo is required to install system packages. Run scripts/setup-linux.sh as root.' >&2
        exit 1
    fi
    privilege=(sudo)
fi
if command -v apt-get >/dev/null; then
    "${privilege[@]}" apt-get update
    "${privilege[@]}" apt-get install -y pkg-config libasound2-dev
elif command -v dnf >/dev/null; then
    "${privilege[@]}" dnf install -y pkgconf-pkg-config alsa-lib-devel
elif command -v pacman >/dev/null; then
    "${privilege[@]}" pacman -S --needed --noconfirm pkgconf alsa-lib
elif command -v zypper >/dev/null; then
    "${privilege[@]}" zypper --non-interactive install pkg-config alsa-devel
else
    printf '%s\n' 'Unsupported package manager: install pkg-config and the ALSA development package for this distribution.' >&2
    exit 1
fi
pkg-config --exists alsa || {
    printf '%s\n' 'ALSA metadata still unavailable after package installation; pkg-config --modversion alsa must succeed.' >&2
    exit 1
}
