#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
PACKAGE_NAME="direct-payment-timesheets"
SOURCE_BINARY="${PROJECT_ROOT}/target/release/direct_payment_timesheets"
OUTPUT_DIRECTORY="${PROJECT_ROOT}/dist"

cd "${PROJECT_ROOT}"

VERSION="$(
    sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml |
        head -n 1
)"
ARCHITECTURE="$(dpkg --print-architecture)"
LIBC_VERSION="$(
    dpkg-query -W -f='${Version}' libc6 |
        cut -d- -f1
)"

if [[ -z "${VERSION}" ]]; then
    printf 'ERROR: Could not read the package version from Cargo.toml\n' >&2
    exit 1
fi

printf 'Building Direct Payments Timesheets %s for %s\n' \
    "${VERSION}" "${ARCHITECTURE}"

cargo build --release

BUILD_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/direct-payment-timesheets-deb.XXXXXX")"
PACKAGE_ROOT="${BUILD_ROOT}/${PACKAGE_NAME}"
OUTPUT_FILE="${OUTPUT_DIRECTORY}/${PACKAGE_NAME}_${VERSION}_${ARCHITECTURE}.deb"

cleanup() {
    rm -rf -- "${BUILD_ROOT}"
}
trap cleanup EXIT

install -Dm755 \
    "${SOURCE_BINARY}" \
    "${PACKAGE_ROOT}/usr/bin/direct-payment-timesheets"

strip "${PACKAGE_ROOT}/usr/bin/direct-payment-timesheets"

install -Dm644 \
    assets/direct-payment-timesheets.png \
    "${PACKAGE_ROOT}/usr/share/icons/hicolor/512x512/apps/direct-payment-timesheets.png"

install -Dm644 \
    packaging/linux/direct-payment-timesheets.desktop \
    "${PACKAGE_ROOT}/usr/share/applications/direct-payment-timesheets.desktop"

install -Dm644 \
    LICENSE \
    "${PACKAGE_ROOT}/usr/share/doc/${PACKAGE_NAME}/copyright"

install -Dm644 \
    THIRD_PARTY_LICENSES.md \
    "${PACKAGE_ROOT}/usr/share/doc/${PACKAGE_NAME}/THIRD_PARTY_LICENSES.md"

INSTALLED_SIZE="$(du -sk "${PACKAGE_ROOT}" | cut -f1)"

install -d -m755 "${PACKAGE_ROOT}/DEBIAN"

printf '%s\n' \
    "Package: ${PACKAGE_NAME}" \
    "Version: ${VERSION}" \
    "Section: office" \
    "Priority: optional" \
    "Architecture: ${ARCHITECTURE}" \
    "Maintainer: Mark Worsdall <atrayzee@gmail.com>" \
    "Homepage: https://github.com/ArnieSkyNet/DirectPaymentTimesheets" \
    "Installed-Size: ${INSTALLED_SIZE}" \
    "Depends: libc6 (>= ${LIBC_VERSION}), libssl3 | libssl3t64, libgcc-s1, zlib1g, libzstd1, ca-certificates" \
    "Description: Manage direct payment payroll timesheets" \
    " Desktop application for preparing, generating and managing" \
    " direct payment payroll timesheets and related payroll documents." \
    > "${PACKAGE_ROOT}/DEBIAN/control"

mkdir -p "${OUTPUT_DIRECTORY}"
rm -f -- "${OUTPUT_FILE}"

dpkg-deb \
    --root-owner-group \
    --build \
    "${PACKAGE_ROOT}" \
    "${OUTPUT_FILE}"

printf '\nCreated package:\n%s\n' "${OUTPUT_FILE}"
