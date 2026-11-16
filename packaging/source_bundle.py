#!/usr/bin/env python3
"""Make the application/locked-crate corresponding-source bundle, never publish.

--check verifies archive availability and notice coverage without packaging a
dirty working tree. Creation requires a clean reviewed commit, just like binaries.
Distribution-native libraries copied into AppImages need a separate source annex.
"""
import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tomllib

from package import ROOT, source_sha

sys.path.insert(0, str(ROOT / 'scripts'))
from third_party_licenses import generate, locked_archives


def verify_notices():
    expected = generate()
    if any(not (ROOT / p).is_file() or (ROOT / p).read_bytes() != data for p, data in expected.items()):
        raise ValueError('Licence inventory is stale; regenerate and review before packaging')


def add_committed_sources(sha, add):
    # Read immutable Git objects, never a working-tree path reopened after
    # the cleanliness check. This also preserves the committed executable bit.
    entries = subprocess.check_output(['git', '-C', str(ROOT), 'ls-tree', '-r', '-z', sha])
    expected, seen = {}, set()
    for record in entries.split(b'\0'):
        if not record:
            continue
        metadata, name = record.split(b'\t', 1)
        mode, kind, oid = metadata.decode().split()
        if kind != 'blob' or mode not in {'100644', '100755'}:
            raise ValueError('Unsupported committed source object')
        expected[name.decode()] = (mode, oid)
    with subprocess.Popen(['git', '-C', str(ROOT), 'archive', '--format=tar', sha],
                          stdout=subprocess.PIPE) as committed:
        with tarfile.open(fileobj=committed.stdout, mode='r|') as source:
            for member in source:
                if member.isdir():
                    continue
                relative = Path(member.name)
                if not member.isfile() or relative.is_absolute() or '..' in relative.parts:
                    raise ValueError('Non-regular/unsafe committed source path')
                if member.name not in expected or member.name in seen:
                    raise ValueError('Unexpected/duplicate committed source path')
                mode, oid = expected[member.name]
                data = source.extractfile(member).read()
                identity = (hashlib.sha1 if len(oid) == 40 else hashlib.sha256)(
                    f'blob {len(data)}\0'.encode() + data).hexdigest()
                if identity != oid:
                    raise ValueError('Exported source differs from committed blob')
                seen.add(member.name)
                add('application/' + member.name, data, mode == '100755')
        if committed.wait() != 0 or seen != set(expected):
            raise ValueError('Committed source export failed or omitted files')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    verify_notices()
    archives = list(locked_archives())
    if args.check and not args.output:
        print(f'Corresponding-source inputs verified: {len(archives)} checksum-verified crate archives')
        return
    if not args.output or args.check:
        parser.error('choose --check or --output PATH')
    sha = source_sha()  # Refuses dirty/untracked source; never invent a revision.
    committed_lock = tomllib.loads(subprocess.check_output(
        ['git', '-C', str(ROOT), 'show', sha + ':Cargo.lock']).decode())
    locked = {(p['name'], p['version'], p['checksum']) for p in committed_lock['package'] if p.get('source')}
    if locked != {(p['name'], p['version'], p['checksum']) for p, _ in archives}:
        raise ValueError('Source archives do not match the committed lockfile')
    committed_version = tomllib.loads(subprocess.check_output(
        ['git', '-C', str(ROOT), 'show', sha + ':Cargo.toml']).decode())['package']['version']
    prefix = f'DirectPaymentTimesheets-{committed_version}-{sha}'
    manifest = dict(source_sha=sha, version=committed_version,
                    scope='Application, build scripts and all locked registry sources; distribution-native annex required separately',
                    crates=[dict(name=p['name'], version=p['version'], sha256=p['checksum']) for p, _ in archives])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open('xb') as output, gzip.GzipFile(fileobj=output, mode='wb', mtime=0) as compressed, tarfile.open(fileobj=compressed, mode='w|') as tar:
        def add(name, data, executable=False):
            info = tarfile.TarInfo(prefix + '/' + name)
            info.mode = 0o755 if executable else 0o644
            info.size = len(data)
            tar.addfile(info, io.BytesIO(data))
        add_committed_sources(sha, add)
        for p, path in archives:
            data = path.read_bytes()
            if hashlib.sha256(data).hexdigest() != p['checksum']:
                raise ValueError('Corresponding-source archive changed after verification')
            add('crates/' + path.name, data)
        add('SOURCE-MANIFEST.json', (json.dumps(manifest, indent=2) + '\n').encode())
    print('Created corresponding-source bundle for ' + sha)


if __name__ == '__main__':
    main()
