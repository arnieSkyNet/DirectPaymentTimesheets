#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
# Child-only removal: the development executable rejects all home overrides.
# A normal executable rejects this flag rather than opening production storage.
exec env -u DIRECTPAYMENTTIMESHEETS_HOME \
  ./target/x86_64-pc-windows-msvc/debug/direct_payment_timesheets.exe \
  --isolated-windows-development
