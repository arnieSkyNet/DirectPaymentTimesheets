#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=packaging/linux/common.sh
source "$(dirname -- "${BASH_SOURCE[0]}")/common.sh"
cd "$PROJECT_ROOT"
if [[ "$TARGET" == armv7-unknown-linux-gnueabihf ]]; then
    export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc
    export CC_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-gcc
    export CXX_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-g++
    export AR_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-ar
    export PKG_CONFIG_ALLOW_CROSS=1
    export PKG_CONFIG_LIBDIR_armv7_unknown_linux_gnueabihf=/usr/lib/arm-linux-gnueabihf/pkgconfig:/usr/share/pkgconfig
    export PKG_CONFIG_SYSROOT_DIR_armv7_unknown_linux_gnueabihf=/
fi
rustup target add "$TARGET"
mkdir -p "$PROJECT_ROOT/target/packages/$TARGET"
# Test executables are captured separately and run in the native/emulated target container.
CARGO_TARGET_DIR="$PROJECT_ROOT/target/packages/$TARGET/tests" cargo test --locked --target "$TARGET" --no-run --message-format=json > "$PROJECT_ROOT/target/packages/$TARGET/tests.jsonl"
for kind in deb appimage; do
    unset SOURCE_BINARY
    prepare_binary "$kind"
done
