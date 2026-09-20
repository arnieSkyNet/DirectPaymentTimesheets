#!/usr/bin/env bash
# Sourced by Linux build/package scripts. Architecture is NEVER inferred from host.
set -euo pipefail
PROJECT_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
: "${TARGET:?Set an explicit Rust TARGET}"
case "$TARGET" in
    x86_64-unknown-linux-gnu) DEB_ARCH=amd64; APPIMAGE_ARCH=x86_64; DEFAULT_STRIP="strip" ;;
    armv7-unknown-linux-gnueabihf) DEB_ARCH=armhf; APPIMAGE_ARCH=armhf; DEFAULT_STRIP=arm-linux-gnueabihf-strip ;;
    aarch64-unknown-linux-gnu) DEB_ARCH=arm64; APPIMAGE_ARCH=aarch64; DEFAULT_STRIP="strip" ;;
    *) echo "Unsupported Linux target: $TARGET" >&2; exit 1 ;;
esac
STRIP="${STRIP:-$DEFAULT_STRIP}"
OUTPUT_DIRECTORY="${OUTPUT_DIRECTORY:-${PROJECT_ROOT}/dist/${TARGET}}"
VERSION="$(python3 "$PROJECT_ROOT/packaging/package.py" version)"
export LC_ALL=C DEB_ARCH APPIMAGE_ARCH VERSION STRIP

prepare_binary() {
    local kind="$1"
    local output_root="$PROJECT_ROOT/target/packages/$TARGET/$kind"
    SOURCE_BINARY="${SOURCE_BINARY:-$output_root/$TARGET/release/direct_payment_timesheets}"
    if [[ "${SKIP_BUILD:-0}" != 1 ]]; then
        [[ "$SOURCE_BINARY" == "$output_root/$TARGET/release/direct_payment_timesheets" ]] || {
            echo 'Use SKIP_BUILD=1 for an externally supplied SOURCE_BINARY' >&2; exit 1;
        }
        (cd "$PROJECT_ROOT" && DPT_INSTALLATION_KIND="$kind" CARGO_TARGET_DIR="$output_root" cargo build --release --locked --target "$TARGET")
        python3 "$PROJECT_ROOT/packaging/package.py" stamp-binary "$SOURCE_BINARY" "$TARGET" "$kind"
    fi
    [[ -f "$SOURCE_BINARY" ]] || { echo "Missing $kind binary: $SOURCE_BINARY" >&2; exit 1; }
    python3 "$PROJECT_ROOT/packaging/package.py" verify-binary "$SOURCE_BINARY" "$TARGET" "$kind"
    python3 "$PROJECT_ROOT/packaging/package.py" elf "$TARGET" "$SOURCE_BINARY"
    mkdir -p "$OUTPUT_DIRECTORY"
}
