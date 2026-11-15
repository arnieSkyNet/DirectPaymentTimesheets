#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=packaging/linux/common.sh
source "$(dirname -- "${BASH_SOURCE[0]}")/common.sh"
prepare_binary deb
BUILD_ROOT="$(mktemp -d)"
trap 'rm -rf -- "$BUILD_ROOT"' EXIT
PACKAGE_ROOT="$BUILD_ROOT/package"
install -Dm755 "$SOURCE_BINARY" "$PACKAGE_ROOT/usr/bin/direct-payment-timesheets"
"$STRIP" "$PACKAGE_ROOT/usr/bin/direct-payment-timesheets"
install -Dm644 "$PROJECT_ROOT/assets/direct-payment-timesheets.png" "$PACKAGE_ROOT/usr/share/icons/hicolor/512x512/apps/direct-payment-timesheets.png"
install -Dm644 "$PROJECT_ROOT/packaging/linux/direct-payment-timesheets.desktop" "$PACKAGE_ROOT/usr/share/applications/direct-payment-timesheets.desktop"
python3 "$PROJECT_ROOT/packaging/package.py" licenses "$PACKAGE_ROOT/usr/share/doc/direct-payment-timesheets"
cp "$PACKAGE_ROOT/usr/share/doc/direct-payment-timesheets/LICENSE" "$PACKAGE_ROOT/usr/share/doc/direct-payment-timesheets/copyright"
mkdir -p "$BUILD_ROOT/debian" "$PACKAGE_ROOT/DEBIAN"
printf 'Source: direct-payment-timesheets\nSection: office\nPriority: optional\nMaintainer: Mark Worsdall <atrayzee@gmail.com>\n\nPackage: direct-payment-timesheets\nArchitecture: %s\nDescription: Direct Payments Timesheets\n' "$DEB_ARCH" > "$BUILD_ROOT/debian/control"
# Run in the target Bookworm userspace: shlibdeps resolves actual target libraries.
DEPENDS="$(cd "$BUILD_ROOT" && dpkg-shlibdeps -O -e"$PACKAGE_ROOT/usr/bin/direct-payment-timesheets")"
DEPENDS="${DEPENDS#shlibs:Depends=}"
[[ -n "$DEPENDS" ]] || { echo 'No shared-library dependencies detected' >&2; exit 1; }
cat > "$PACKAGE_ROOT/DEBIAN/control" <<CONTROL
Package: direct-payment-timesheets
Version: $VERSION
Section: office
Priority: optional
Architecture: $DEB_ARCH
Maintainer: Mark Worsdall <atrayzee@gmail.com>
Homepage: https://github.com/ArnieSkyNet/DirectPaymentTimesheets
Installed-Size: $(du -sk "$PACKAGE_ROOT" | cut -f1)
Depends: $DEPENDS, ca-certificates, libx11-6, libx11-xcb1, libxcb1, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libegl1, libgl1, xdg-utils, xdg-desktop-portal
Recommends: xdg-desktop-portal-gtk | xdg-desktop-portal-kde
Description: Manage direct payment payroll timesheets
 Desktop application for preparing and managing payroll documents.
CONTROL
OUTPUT_FILE="$OUTPUT_DIRECTORY/direct-payment-timesheets_${VERSION}_${DEB_ARCH}.deb"
dpkg-deb --root-owner-group --build "$PACKAGE_ROOT" "$OUTPUT_FILE"
[[ "$(dpkg-deb -f "$OUTPUT_FILE" Architecture)" == "$DEB_ARCH" ]]
[[ "$(dpkg-deb -f "$OUTPUT_FILE" Version)" == "$VERSION" ]]
dpkg-deb -x "$OUTPUT_FILE" "$BUILD_ROOT/verify"
python3 "$PROJECT_ROOT/packaging/package.py" elf "$TARGET" "$BUILD_ROOT/verify/usr/bin/direct-payment-timesheets"
test -f "$BUILD_ROOT/verify/usr/share/doc/direct-payment-timesheets/DejaVu-LICENSE.txt"
test -d "$BUILD_ROOT/verify/usr/share/doc/direct-payment-timesheets/third-party"
python3 "$PROJECT_ROOT/packaging/package.py" record "$OUTPUT_FILE" "$TARGET" deb
