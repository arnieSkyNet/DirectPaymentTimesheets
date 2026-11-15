"""Build receipt from a previously supervised --no-run build. Never run a test.

Run from the clean source checkout with this script from the pinned controller.
The printed manifest digest must be retained separately and supplied to the driver.
"""
import json
import os
from pathlib import Path
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from native_sqlite_terminal_checks import Refusal, digest, source_inventory, verify_controller, verify_source_fixture
from native_sqlite_fixture_storage import no_links


def build_manifest(build_log):
    verify_controller()
    sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True, timeout=5).strip()
    if sha != os.environ.get('EXPECTED_SHA') or sha == '0' * 40:
        raise Refusal('unapproved_source')
    if subprocess.check_output(['git', 'status', '--porcelain=v1', '--untracked-files=all'], timeout=5).strip():
        raise Refusal('unapproved_source')
    target = no_links(Path.cwd() / 'target').resolve(strict=True)
    log = no_links(Path(build_log)).resolve(strict=True)
    if not log.is_relative_to(target):
        raise Refusal('fixture_inventory_mismatch')
    records = [json.loads(line) for line in log.read_text().splitlines()]
    artifacts = [r for r in records if r.get('reason') == 'compiler-artifact'
                 and r.get('target', {}).get('name') == 'native_sqlite_terminal_fixtures']
    if len(artifacts) != 1:
        raise Refusal('fixture_inventory_mismatch')
    record = artifacts[0]
    if record['target']['kind'] != ['test'] or record.get('profile', {}).get('test') is not True:
        raise Refusal('fixture_inventory_mismatch')
    exe = no_links(Path(record.get('executable', ''))).resolve(strict=True)
    if not exe.is_relative_to(target) or not exe.is_file() or exe.stat().st_nlink != 1:
        raise Refusal('fixture_inventory_mismatch')
    inventory = source_inventory()
    verify_source_fixture(Path.cwd(), sha, inventory)
    return dict(sha=sha, source=inventory, executable=str(exe),
                executable_sha256=digest(exe), target='native_sqlite_terminal_fixtures')


def main():
    if len(sys.argv) != 2:
        raise Refusal('fixture_inventory_mismatch')
    data = build_manifest(sys.argv[1])
    path = Path('target/native-sqlite-manifest.json')
    with path.open('x', encoding='utf-8') as stream:
        json.dump(data, stream, sort_keys=True, indent=2)
    print(digest(path))


if __name__ == '__main__':
    try:
        main()
    except Exception:
        print('native.sqlite: class=manifest_refused', file=sys.stderr)
        sys.exit(1)
