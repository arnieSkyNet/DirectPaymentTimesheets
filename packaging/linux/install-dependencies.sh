#!/usr/bin/env bash
# Execute only inside a disposable Debian 12 container; never against the host.
set -euo pipefail
source /etc/os-release
[[ "$ID" == debian && "$VERSION_ID" == 12 ]] || { echo 'Debian 12 container required' >&2; exit 1; }
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends build-essential ca-certificates curl git python3 \
    pkg-config dpkg-dev binutils file squashfs-tools patchelf desktop-file-utils libssl-dev \
    libx11-6 libx11-xcb1 libxcb1 libxkbcommon0 libxkbcommon-x11-0 libwayland-client0 \
    libegl1 libgl1 xdg-utils xdg-desktop-portal xdg-desktop-portal-gtk
# Only the target test container needs a runnable compiler. Debian's native
# compiler exercises the standalone cfg gate; release binaries still use pinned
# Rust 1.98.1 in the separate build container. No application is built with this.
if [[ "${1:-}" == test-runtime ]]; then
    apt-get install -y --no-install-recommends rustc
fi
if [[ "${1:-}" == cross-armv7 ]]; then
    dpkg --add-architecture armhf
    apt-get update
    apt-get install -y --no-install-recommends gcc-arm-linux-gnueabihf g++-arm-linux-gnueabihf binutils-arm-linux-gnueabihf libssl-dev:armhf
fi
