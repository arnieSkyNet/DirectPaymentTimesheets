#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
SOURCE_BINARY="${PROJECT_ROOT}/target/release/direct_payment_timesheets"
OUTPUT_DIRECTORY="${PROJECT_ROOT}/dist"

cd "${PROJECT_ROOT}"

VERSION="$(
    sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml |
        head -n 1
)"
ARCHITECTURE="$(uname -m)"

if [[ -z "${VERSION}" ]]; then
    printf 'ERROR: Could not read the package version from Cargo.toml\n' >&2
    exit 1
fi

if [[ "${ARCHITECTURE}" != "x86_64" ]]; then
    printf 'ERROR: This script currently builds only x86_64 AppImages\n' >&2
    exit 1
fi

LINUXDEPLOY="${LINUXDEPLOY:-$(command -v linuxdeploy || true)}"

if [[ -z "${LINUXDEPLOY}" || ! -x "${LINUXDEPLOY}" ]]; then
    printf 'ERROR: Set LINUXDEPLOY to an executable linuxdeploy file\n' >&2
    exit 1
fi

printf 'Building Direct Payments Timesheets %s AppImage\n' "${VERSION}"
cargo build --release

BUILD_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/direct-payment-timesheets-appimage.XXXXXX")"
APPDIR="${BUILD_ROOT}/DirectPaymentsTimesheets.AppDir"
OUTPUT_FILE="${OUTPUT_DIRECTORY}/Direct_Payments_Timesheets-${VERSION}-x86_64.AppImage"

cleanup() {
    rm -rf -- "${BUILD_ROOT}"
}
trap cleanup EXIT

mkdir -p "${OUTPUT_DIRECTORY}"
rm -f -- "${OUTPUT_FILE}"

LINUXDEPLOY_COMMAND=("${LINUXDEPLOY}")
if [[ "${LINUXDEPLOY}" == *.AppImage ]]; then
    LINUXDEPLOY_COMMAND+=("--appimage-extract-and-run")
fi

cd "${BUILD_ROOT}"

VERSION="${VERSION}" \
OUTPUT="${OUTPUT_FILE}" \
"${LINUXDEPLOY_COMMAND[@]}" \
    --appdir "${APPDIR}" \
    --executable "${SOURCE_BINARY}" \
    --desktop-file "${PROJECT_ROOT}/packaging/linux/direct-payment-timesheets.desktop" \
    --icon-file "${PROJECT_ROOT}/assets/direct-payment-timesheets.png" \
    --output appimage

if [[ ! -f "${OUTPUT_FILE}" ]]; then
    GENERATED_FILE="$(
        find "${BUILD_ROOT}" -maxdepth 1 -type f -name '*.AppImage' |
            head -n 1
    )"

    if [[ -z "${GENERATED_FILE}" ]]; then
        printf 'ERROR: linuxdeploy did not create an AppImage\n' >&2
        exit 1
    fi

    mv -- "${GENERATED_FILE}" "${OUTPUT_FILE}"
fi

chmod 755 "${OUTPUT_FILE}"

printf '\nCreated AppImage:\n%s\n' "${OUTPUT_FILE}"
file "${OUTPUT_FILE}"
sha256sum "${OUTPUT_FILE}"
