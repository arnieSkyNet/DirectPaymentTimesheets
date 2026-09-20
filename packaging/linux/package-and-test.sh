#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=packaging/linux/common.sh
source "$(dirname -- "${BASH_SOURCE[0]}")/common.sh"
cd "$PROJECT_ROOT"
# This script runs inside the target userspace, including ARMv7 under QEMU.
# Do not let cached binaries for the wrong target be relabelled.
[[ "$(dpkg --print-architecture)" == "$DEB_ARCH" ]] || { echo 'Packaging userspace does not match explicit target' >&2; exit 1; }
export STRIP=strip SKIP_BUILD=1
python3 - "$PROJECT_ROOT/target/packages/$TARGET/tests.jsonl" <<'PY'
import json, subprocess, sys
executables = [m['executable'] for line in open(sys.argv[1]) if (m := json.loads(line)).get('reason') == 'compiler-artifact' and m.get('profile', {}).get('test') and m.get('executable')]
if not executables:
    raise SystemExit('No target tests found')
for executable in executables:
    subprocess.run([executable], check=True)
PY
bash packaging/linux/build-deb.sh
bash packaging/linux/build-appimage.sh
