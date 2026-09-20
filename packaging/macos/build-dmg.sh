#!/usr/bin/env bash
set -euo pipefail
PROJECT_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$PROJECT_ROOT"
: "${TARGET:?Set the macOS Rust target}"
case "$TARGET" in
    aarch64-apple-darwin) ARCH=arm64 ;;
    x86_64-apple-darwin) ARCH=x86_64 ;;
    *) echo "Unsupported macOS target: $TARGET" >&2; exit 1 ;;
esac
[[ "$(uname -m)" == "$ARCH" ]] || { echo 'Use a native runner for each macOS architecture' >&2; exit 1; }
export MACOSX_DEPLOYMENT_TARGET=12.0 DPT_INSTALLATION_KIND=macos
export CARGO_TARGET_DIR="$PROJECT_ROOT/target/packages/$TARGET/macos"
VERSION="$(python3 packaging/package.py version)"
cargo test --locked --target "$TARGET"
cargo build --release --locked --target "$TARGET"
OUTPUT_DIRECTORY="$PROJECT_ROOT/dist/$TARGET"
mkdir -p "$OUTPUT_DIRECTORY"
BUILD_ROOT="$(mktemp -d)"
trap 'rm -rf -- "$BUILD_ROOT"' EXIT
STAGE="$BUILD_ROOT/dmg"
APP="$STAGE/Direct Payments Timesheets.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
install -m755 "$CARGO_TARGET_DIR/$TARGET/release/direct_payment_timesheets" "$APP/Contents/MacOS/direct_payment_timesheets"
python3 packaging/package.py licenses "$APP/Contents/Resources"
# App icon uses the checked-in artwork; PDF and egui fonts are embedded already.
ICONSET="$BUILD_ROOT/direct-payment-timesheets.iconset"
mkdir "$ICONSET"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" assets/direct-payment-timesheets.png --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" assets/direct-payment-timesheets.png --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil --convert icns "$ICONSET" --output "$APP/Contents/Resources/direct-payment-timesheets.icns"
python3 - "$APP/Contents/Info.plist" "$VERSION" <<'PY'
import plistlib, sys
with open(sys.argv[1], 'wb') as f:
    plistlib.dump({
        'CFBundleIdentifier': 'io.github.arnieskynet.DirectPaymentTimesheets',
        'CFBundleName': 'Direct Payments Timesheets',
        'CFBundleDisplayName': 'Direct Payments Timesheets',
        'CFBundleExecutable': 'direct_payment_timesheets',
        'CFBundlePackageType': 'APPL',
        'CFBundleShortVersionString': sys.argv[2],
        'CFBundleVersion': sys.argv[2],
        'CFBundleIconFile': 'direct-payment-timesheets.icns',
        'LSMinimumSystemVersion': '12.0',
        'NSHighResolutionCapable': True,
        'NSPrincipalClass': 'NSApplication',
    }, f)
PY
plutil -lint "$APP/Contents/Info.plist"
BINARY="$APP/Contents/MacOS/direct_payment_timesheets"
[[ "$(lipo -archs "$BINARY")" == "$ARCH" ]]
otool -L "$BINARY" > "$BUILD_ROOT/libraries.txt"
# Frameworks and dylibs must come from macOS, never Homebrew or a runner path.
python3 - "$BUILD_ROOT/libraries.txt" <<'PY'
import sys
for line in open(sys.argv[1]).readlines()[1:]:
    dependency = line.strip().split(' (', 1)[0]
    if not dependency.startswith(('/usr/lib/', '/System/Library/')):
        raise SystemExit('Non-system macOS dependency: ' + dependency)
PY
xcrun vtool -show-build "$BINARY" > "$BUILD_ROOT/deployment.txt"
grep -Eq 'minos 12\.0($|\.0$)' "$BUILD_ROOT/deployment.txt"
# No Developer ID / notarisation. Ad-hoc seal is needed for ARM64 execution and
# must be created after all bundle modifications; it confers no publisher trust.
codesign --force --sign - "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"
codesign -dv "$APP" 2> "$BUILD_ROOT/signature.txt"
grep -q 'Signature=adhoc' "$BUILD_ROOT/signature.txt"
ln -s /Applications "$STAGE/Applications"
OUTPUT_FILE="$OUTPUT_DIRECTORY/DirectPaymentTimesheets-${VERSION}-macos-${ARCH}.dmg"
hdiutil create -volname "Direct Payments Timesheets $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$OUTPUT_FILE"
hdiutil verify "$OUTPUT_FILE"
# Inspect the actual downloadable image without launching its contents.
MOUNT="$BUILD_ROOT/mount"
mkdir "$MOUNT"
hdiutil attach -readonly -nobrowse -mountpoint "$MOUNT" "$OUTPUT_FILE"
trap 'hdiutil detach "$MOUNT" >/dev/null 2>&1 || true; rm -rf -- "$BUILD_ROOT"' EXIT
plutil -lint "$MOUNT/Direct Payments Timesheets.app/Contents/Info.plist"
codesign --verify --deep --strict "$MOUNT/Direct Payments Timesheets.app"
[[ "$(lipo -archs "$MOUNT/Direct Payments Timesheets.app/Contents/MacOS/direct_payment_timesheets")" == "$ARCH" ]]
test -f "$MOUNT/Direct Payments Timesheets.app/Contents/Resources/DejaVu-LICENSE.txt"
test -L "$MOUNT/Applications"
hdiutil detach "$MOUNT"
trap 'rm -rf -- "$BUILD_ROOT"' EXIT
python3 packaging/package.py record "$OUTPUT_FILE" "$TARGET" macos
