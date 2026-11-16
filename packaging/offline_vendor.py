#!/usr/bin/env python3
"""Prepare an offline Cargo vendor tree from a corresponding-source bundle.

Never downloads or executes source. Destination must be new. Verify every crate
archive and every member name before creating files; never follow archive links.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import tarfile


def prepare(manifest, archives, destination):
    plans = []
    names = set()
    for package in manifest['crates']:
        name = package['name'] + '-' + package['version']
        if not re.fullmatch(r'[A-Za-z0-9_.+-]+', name) or name in names:
            raise ValueError('Unsafe/duplicate crate identity')
        names.add(name)
        path = archives / (name + '.crate')
        data = path.read_bytes()
        if path.is_symlink() or hashlib.sha256(data).hexdigest() != package['sha256']:
            raise ValueError('Source archive checksum mismatch: ' + name)
        with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as tar:
            files = []
            for member in tar:
                relative = PurePosixPath(member.name).relative_to(name)
                if ('..' in relative.parts or relative.is_absolute()
                        or '\\' in str(relative) or ':' in str(relative)):
                    raise ValueError('Unsafe source member')
                if member.isdir():
                    continue
                if not member.isfile():
                    raise ValueError('Source archive links/special files are refused')
                files.append(str(relative))
            if len(set(files)) != len(files) or '.cargo-checksum.json' in files:
                raise ValueError('Duplicate/reserved source member')
        plans.append((package, name, data, files))
    destination.mkdir(parents=True, exist_ok=False)
    for package, name, data, files in plans:
        checksums = {}
        with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as tar:
            for relative in files:
                member_data = tar.extractfile(name + '/' + relative).read()
                target = destination / name / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                with target.open('xb') as output:
                    output.write(member_data)
                target.chmod(0o755 if tar.getmember(name + '/' + relative).mode & 0o111 else 0o644)
                checksums[relative] = hashlib.sha256(member_data).hexdigest()
        (destination / name / '.cargo-checksum.json').write_text(json.dumps(
            dict(package=package['sha256'], files=checksums), sort_keys=True) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('manifest', type=Path)
    parser.add_argument('archives', type=Path)
    parser.add_argument('destination', type=Path)
    args = parser.parse_args()
    prepare(json.loads(args.manifest.read_text()), args.archives, args.destination)
    print('Offline vendor tree prepared. Configure Cargo source.crates-io.replace-with')
    print('and source.<replacement>.directory to this exact destination; build --locked --offline.')


if __name__ == '__main__':
    main()
