#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=packaging/linux/common.sh
source "$(dirname -- "${BASH_SOURCE[0]}")/common.sh"
prepare_binary appimage
BUILD_ROOT="$(mktemp -d)"
trap 'rm -rf -- "$BUILD_ROOT"' EXIT
APPDIR="$BUILD_ROOT/DirectPaymentsTimesheets.AppDir"
python3 "$PROJECT_ROOT/packaging/linux/download-tools.py" "$APPIMAGE_ARCH" "$BUILD_ROOT/tools"
# Extract tools explicitly: no /dev/fuse, nested mounts or output-plugin downloads.
mkdir "$BUILD_ROOT/deploy" "$BUILD_ROOT/image"
(cd "$BUILD_ROOT/deploy" && "$BUILD_ROOT/tools/linuxdeploy" --appimage-extract >/dev/null)
(cd "$BUILD_ROOT/image" && "$BUILD_ROOT/tools/appimagetool" --appimage-extract >/dev/null)
install -Dm755 "$SOURCE_BINARY" "$BUILD_ROOT/direct-payment-timesheets"
"$STRIP" "$BUILD_ROOT/direct-payment-timesheets"
"$BUILD_ROOT/deploy/squashfs-root/AppRun" \
    --appdir "$APPDIR" --executable "$BUILD_ROOT/direct-payment-timesheets" \
    --desktop-file "$PROJECT_ROOT/packaging/linux/direct-payment-timesheets.desktop" \
    --icon-file "$PROJECT_ROOT/assets/direct-payment-timesheets.png"
python3 "$PROJECT_ROOT/packaging/package.py" licenses "$APPDIR/usr/share/doc/direct-payment-timesheets"
install -Dm644 "$BUILD_ROOT/tools/runtime-license" "$APPDIR/usr/share/doc/appimage-runtime/LICENSE"
# Preserve distribution notices for libraries linuxdeploy actually copied.
mkdir -p "$APPDIR/usr/share/doc/bundled-libraries"
while IFS= read -r -d '' library; do
    basename="$(basename "$library")"
    soname="$(readelf -d "$library" | awk '/\(SONAME\)/ {gsub(/[][]/, "", $5); print $5}')"
    system_library="$(ldconfig -p | awk -v name="${soname:-$basename}" '$1 == name && !found {print $NF; found=1}')"
    [[ -n "$system_library" ]] || { echo "Cannot locate bundled library provenance: $basename" >&2; exit 1; }
    owner="$(dpkg-query -S "$system_library" 2>/dev/null || dpkg-query -S "$(readlink -f "$system_library")")"
    owner="${owner%%: /*}"
    package="${owner%%:*}"
    test -f "/usr/share/doc/$package/copyright"
    cp "/usr/share/doc/$package/copyright" "$APPDIR/usr/share/doc/bundled-libraries/$package.copyright"
done < <(find "$APPDIR/usr/lib" -type f -name '*.so*' -print0)
OUTPUT_FILE="$OUTPUT_DIRECTORY/DirectPaymentTimesheets-${VERSION}-linux-${APPIMAGE_ARCH}.AppImage"
ARCH="$APPIMAGE_ARCH" VERSION="$VERSION" "$BUILD_ROOT/image/squashfs-root/AppRun" \
    --runtime-file "$BUILD_ROOT/tools/runtime" "$APPDIR" "$OUTPUT_FILE"
chmod 755 "$OUTPUT_FILE"
mkdir "$BUILD_ROOT/verify"
(cd "$BUILD_ROOT/verify" && "$OUTPUT_FILE" --appimage-extract >/dev/null)
EXTRACTED="$BUILD_ROOT/verify/squashfs-root"
python3 "$PROJECT_ROOT/packaging/package.py" elf "$TARGET" "$EXTRACTED/usr/bin/direct-payment-timesheets"
# Verify the outer runtime architecture without requiring Rust ARMv7 attributes.
readelf -h "$OUTPUT_FILE" > "$BUILD_ROOT/runtime-header"
case "$APPIMAGE_ARCH" in
    x86_64) grep -q 'Advanced Micro Devices X86-64' "$BUILD_ROOT/runtime-header" ;;
    aarch64) grep -q 'AArch64' "$BUILD_ROOT/runtime-header" ;;
    armhf) grep -q 'Machine:.*ARM' "$BUILD_ROOT/runtime-header"; grep -q 'ELF32' "$BUILD_ROOT/runtime-header" ;;
esac
test -x "$EXTRACTED/AppRun"
test -f "$EXTRACTED/usr/share/doc/direct-payment-timesheets/DejaVu-LICENSE.txt"
desktop-file-validate "$EXTRACTED/direct-payment-timesheets.desktop"
# Execute only the loader, never the application. Includes bundled search paths.
LD_LIBRARY_PATH="$EXTRACTED/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" ldd "$EXTRACTED/usr/bin/direct-payment-timesheets" > "$BUILD_ROOT/ldd"
cat "$BUILD_ROOT/ldd"
if grep -q 'not found' "$BUILD_ROOT/ldd"; then exit 1; fi
python3 "$PROJECT_ROOT/packaging/package.py" record "$OUTPUT_FILE" "$TARGET" appimage
