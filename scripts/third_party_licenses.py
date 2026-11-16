#!/usr/bin/env python3
"""Generate/check notices from checksum-verified locked .crate archives, offline.

Run cargo fetch --locked first. No crate code is executed. The generated tree
retains nested native notices too; Cargo.toml wrapper licences are insufficient.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
NOTICE = re.compile(r'^(licen[sc]e|copying|copyright|notice|authors)(?:$|[._-])', re.I)


LEGACY_NOTICES = (
    'third-party/fonts/Hack-Regular.txt',
    'third-party/fonts/OFL.txt',
    'third-party/fonts/UFL.txt',
    'third-party/fonts/emoji-icon-font-mit-license.txt',
    'third-party/licenses/0BSD.txt',
    'third-party/licenses/Apache-2.0.txt',
    'third-party/licenses/BSD-2-Clause.txt',
    'third-party/licenses/BSD-3-Clause.txt',
    'third-party/licenses/BSL-1.0.txt',
    'third-party/licenses/CC0-1.0.txt',
    'third-party/licenses/ISC.txt',
    'third-party/licenses/MIT.txt',
    'third-party/licenses/MPL-2.0.txt',
    'third-party/licenses/Unicode-3.0.txt',
    'third-party/licenses/Unlicense.txt',
    'third-party/licenses/Zlib.txt',
    'third-party/native/XZ-libLZMA-COPYING',
    'third-party/native/Zstandard-LICENSE',
    'third-party/native/bzip2-1.0.8-LICENSE',
    'third-party/native/libsqlite3-sys-MIT.txt',
    'third-party/native/native-tls-MIT.txt',
    'third-party/native/openssl-crate-MIT.txt',
    'third-party/native/openssl-sys-MIT.txt',
    'third-party/native/zstd-sys-bindings-BSD-3-Clause.txt',
    'third-party/notices/cfg_aliases-0.2.2-NOTICES.md',
    'third-party/notices/subsetter-0.2.6-NOTICE',
    'third-party/notices/svg2pdf-0.13.0-NOTICE',
)


def notice_allowlist(root, output):
    """Independent release input, regenerated only from verified upstream archives."""
    files = {path.as_posix(): hashlib.sha256(data).hexdigest() for path, data in output.items()}
    for name in (*LEGACY_NOTICES, "LICENSE", "assets/fonts/DejaVu-LICENSE.txt"):
        files[name] = hashlib.sha256((root / name).read_bytes()).hexdigest()
    return (json.dumps(dict(schema=1, lock_sha256=hashlib.sha256(
        (root / "Cargo.lock").read_bytes()).hexdigest(), files=files),
        indent=2, sort_keys=True) + "\n").encode()


def locked_archives(root=ROOT, cargo_home=None):
    cargo_home = Path(cargo_home or os.environ.get('CARGO_HOME', Path.home() / '.cargo'))
    packages = tomllib.loads((root / 'Cargo.lock').read_text())['package']
    for p in sorted(packages, key=lambda p: (p['name'], p['version'])):
        if not p.get('source'):
            continue
        if p['source'] != 'registry+https://github.com/rust-lang/crates.io-index':
            raise ValueError('Unreviewed dependency source: ' + p['name'])
        name = p['name'] + '-' + p['version']
        candidates = list((cargo_home / 'registry/cache').glob('*/' + name + '.crate'))
        verified = [q for q in candidates if hashlib.sha256(q.read_bytes()).hexdigest() == p['checksum']]
        if not verified:
            raise ValueError('Missing/checksum-invalid archive: ' + name + '; run cargo fetch --locked')
        yield p, verified[0]


def generate(root=ROOT, cargo_home=None):
    output, manifest, groups = {}, [], {}
    for p, archive in locked_archives(root, cargo_home):
        name = p['name'] + '-' + p['version']
        archive_bytes = archive.read_bytes()
        if hashlib.sha256(archive_bytes).hexdigest() != p['checksum']:
            raise ValueError('Archive changed after verification: ' + name)
        with tarfile.open(fileobj=io.BytesIO(archive_bytes), mode='r:gz') as tar:
            meta = tomllib.loads(tar.extractfile(name + '/Cargo.toml').read().decode())['package']
            expression = meta.get('license', 'SEE licence-file')
            notices = {}
            for member in tar.getmembers():
                relative = PurePosixPath(member.name).relative_to(name)
                if relative.is_absolute() or '..' in relative.parts:
                    raise ValueError('Unsafe crate member: ' + name)
                if not member.isfile() or not (NOTICE.match(relative.name)
                        or str(relative) == 'Cargo.toml'
                        or len(relative.parts) == 1 and relative.name.lower().startswith('readme')
                        or str(relative) == meta.get('license-file')):
                    continue
                if '\\' in str(relative) or ':' in str(relative):
                    raise ValueError('Unsafe cross-platform notice path: ' + name)
                data = tar.extractfile(member).read()
                # Retain exact bytes, including upstream non-UTF8 attribution.
                target = Path('third-party/crates') / name / str(relative)
                output[target] = data
                notices[str(relative)] = hashlib.sha256(data).hexdigest()
            if not notices and not meta.get('license'):
                raise ValueError('No licence declaration or notice: ' + name)
            manifest.append(dict(name=p['name'], version=p['version'], license=expression,
                                 archive_sha256=p['checksum'], notices=notices))
            groups.setdefault(expression, []).append(name)
    output[Path('third-party/crates/manifest.json')] = (
        json.dumps(dict(schema=1, packages=manifest), indent=2, sort_keys=True) + '\n').encode()
    intro = '''# Third-party licences and notices

Generated by `python3 scripts/third_party_licenses.py --write` from every exact
registry archive in Cargo.lock, verified against its Cargo checksum. Recheck with
`python3 scripts/third_party_licenses.py --check` after `cargo fetch --locked`.
This includes optional, development and all-target entries, not just host builds.

DirectPaymentTimesheets is GPL-3.0-or-later; see LICENSE. Upstream code and fonts
retain their own terms. For OR expressions select a GPL-compatible alternative;
AND terms all apply. An inventory is not a grant to remove copyright notices.
Exact upstream notices (including nested native material), manifest author/licence
declarations and upstream README attribution, with their hashes, are
in third-party/crates/manifest.json and the corresponding package directories.
Earlier font/native notices in third-party/fonts, native and notices are retained.

## Native components and licence choices

- libdbus-sys vendors D-Bus 1.14.4. Select GPL-2.0-or-later, exercised as GPLv3
  alongside this application, rather than treating the Rust wrapper's MIT/Apache
  expression as the C library's licence. Its complete COPYING and copyright
  material are retained below its crate directory. Include its exact source.
- Bundled SQLite comes from libsqlite3-sys; retain its wrapper MIT notice and
  SQLite's public-domain dedication in the exact crate source.
- Ring includes BoringSSL and other upstream code with additional ISC/Apache/MIT
  notices. Preserve its complete collected notice tree, not only a generic text.
- zstd-sys, bzip2-sys, lzma-sys and openssl-src contain native source notices.
  A locked optional archive does not mean it is linked into every package.
- native-tls uses platform TLS. Inspect the exact shipped/linked implementation;
  copied AppImage libraries require their distribution notices AND applicable
  source delivery. Host system libraries are separate from bundled payloads.
- DejaVu, egui default fonts, and renderer assets keep their upstream font and
  embedded-asset terms, including reserved names and notice requirements.

## Corresponding source

See docs/CORRESPONDING-SOURCE.md and packaging/source_bundle.py. A release needs
the exact clean reviewed application source, build scripts, locked crate archives
and applicable bundled-native source; an old binary cannot be relabelled as a new
revision. Generic homepage links do not establish source correspondence.

## Complete locked registry inventory

'''
    inventory = intro + f'{len(manifest)} exact registry package/version entries.\n\n'
    for expression, names in sorted(groups.items()):
        inventory += f'### `{expression}` ({len(names)})\n\n'
        inventory += '\n'.join('- `' + name + '`' for name in names) + '\n\n'
    output[Path('THIRD_PARTY_LICENSES.md')] = inventory.encode()
    output[Path('packaging/notice-allowlist.json')] = notice_allowlist(root, output)
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', action='store_true')
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    if args.write == args.check:
        parser.error('choose exactly one of --write or --check')
    expected = generate()
    existing = {p.relative_to(ROOT) for p in (ROOT / 'third-party/crates').rglob('*') if p.is_file()}
    stale = existing - expected.keys()
    if stale:
        raise SystemExit('Stale generated notice files require review: ' + ', '.join(map(str, sorted(stale))))
    different = []
    for relative, data in expected.items():
        path = ROOT / relative
        if args.write:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        elif not path.is_file() or path.read_bytes() != data:
            different.append(str(relative))
    if different:
        raise SystemExit('Missing/changed notices: ' + ', '.join(different))
    print(f'Licence {"generation" if args.write else "verification"}: {len(expected)} files, all locked registry archives verified')


if __name__ == '__main__':
    main()
