#!/usr/bin/env python3
"""Shared, stdlib-only packaging metadata and validation. Never starts the app."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import struct
import tomllib

ROOT = Path(__file__).resolve().parent.parent
TARGETS = {
    'x86_64-unknown-linux-gnu': ('amd64', 'x86_64', 'ELF64', 'Advanced Micro Devices X86-64'),
    'armv7-unknown-linux-gnueabihf': ('armhf', 'armhf', 'ELF32', 'ARM'),
    'aarch64-unknown-linux-gnu': ('arm64', 'aarch64', 'ELF64', 'AArch64'),
}
KINDS = {'deb', 'appimage', 'windows', 'macos'}


def version():
    value = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
    if not re.fullmatch(r'\d+\.\d+\.\d+', value):
        raise ValueError('Packaging requires a numeric major.minor.patch version')
    return value


def digest(path):
    with Path(path).open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def validate_elf(text, target):
    _, _, bits, machine = TARGETS[target]
    if not re.search(r'Class:\s+' + bits + r'\b', text) or not re.search(r'Machine:\s+' + re.escape(machine) + r'\s*$', text, re.M):
        raise ValueError(f'Wrong ELF architecture: expected {target}')
    if target.startswith('armv7'):
        if 'hard-float ABI' not in text or not re.search(r'Tag_CPU_arch:\s+v7\b', text):
            raise ValueError('ARM32 must be ARMv7 EABI hard-float')
        if not re.search(r'Tag_ABI_VFP_args:\s+VFP registers', text):
            raise ValueError('ARM32 must pass floating-point arguments in VFP registers')
    requirements = [tuple(map(int, v.split('.'))) for v in re.findall(r'\bGLIBC_(\d+\.\d+(?:\.\d+)?)\b', text)]
    if any(v > (2, 36) for v in requirements):
        raise ValueError('Binary exceeds the Debian 12 GLIBC 2.36 baseline')


WINDOWS_MACHINES = {'x86_64-pc-windows-msvc': 0x8664, 'aarch64-pc-windows-msvc': 0xAA64}


def validate_pe(binary, target):
    data = Path(binary).read_bytes()
    try:
        offset = struct.unpack_from('<I', data, 0x3c)[0]
        machine = struct.unpack_from('<H', data, offset + 4)[0]
        magic = struct.unpack_from('<H', data, offset + 24)[0]
        subsystem = struct.unpack_from('<H', data, offset + 24 + 68)[0]
        if (data[:2] != b'MZ' or data[offset:offset + 4] != b'PE\0\0'
                or machine != WINDOWS_MACHINES[target] or magic != 0x20B or subsystem != 2):
            raise ValueError('Expected a PE32+ Windows GUI executable for ' + target)
    except (struct.error, KeyError) as error:
        raise ValueError('Invalid Windows executable or target') from error


def licenses(destination):
    destination.mkdir(parents=True, exist_ok=True)
    for name in ['LICENSE', 'THIRD_PARTY_LICENSES.md']:
        shutil.copy2(ROOT / name, destination / name)
    shutil.copy2(ROOT / 'assets/fonts/DejaVu-LICENSE.txt', destination)
    shutil.copytree(ROOT / 'third-party', destination / 'third-party', dirs_exist_ok=True)


def source_sha():
    source = subprocess.check_output(['git', '-c', f'safe.directory={ROOT}', '-C', str(ROOT), 'rev-parse', 'HEAD'], text=True).strip()
    if os.environ.get('SOURCE_SHA', source) != source:
        raise ValueError('Checkout differs from captured source SHA')
    return source


def binary_provenance(binary, target, kind, verify=False):
    path = binary.with_name(binary.name + '.build.json')
    expected = dict(source_sha=source_sha(), version=version(), target=target, installation_kind=kind, sha256=digest(binary))
    if verify:
        data = json.loads(path.read_text())
        if any(data.get(key) != value for key, value in expected.items()):
            raise ValueError('Wrong source, target, installation kind or modified binary')
    else:
        rust = subprocess.check_output(['rustc', '--version'], text=True).strip()
        pin = json.loads((ROOT / 'packaging/toolchain.json').read_text())['rust']
        if not rust.startswith('rustc ' + pin + ' '):
            raise ValueError('Build requires pinned Rust ' + pin)
        expected['rustc'] = rust
        path.write_text(json.dumps(expected, indent=2) + '\n')


def record(artifact, target, kind):
    if kind not in KINDS or not artifact.is_file():
        raise ValueError('Missing artifact or unsupported installation kind')
    source = source_sha()
    if expected_packages(version()).get(artifact.name) != (target, kind):
        raise ValueError('Artifact filename does not match version, target and installation kind')
    metadata = {
        'source_sha': source, 'version': version(), 'target': target,
        'installation_kind': kind, 'file': artifact.name, 'sha256': digest(artifact),
        'rust_version': json.loads((ROOT / 'packaging/toolchain.json').read_text())['rust'], 'run_id': os.environ.get('GITHUB_RUN_ID'),
        'packaging_tools': json.loads((ROOT / 'packaging/toolchain.json').read_text()),
        'appimage_tool_assets': json.loads((ROOT / 'packaging/linux/tools.json').read_text()) if kind == 'appimage' else None,
    }
    artifact.with_name(artifact.name + '.json').write_text(json.dumps(metadata, indent=2) + '\n')
    artifact.with_name(artifact.name + '.sha256').write_text(f"{metadata['sha256']}  {artifact.name}\n")


def expected_packages(v):
    result = {}
    for target, (deb, arch, _, _) in TARGETS.items():
        result[f'direct-payment-timesheets_{v}_{deb}.deb'] = (target, 'deb')
        result[f'DirectPaymentTimesheets-{v}-linux-{arch}.AppImage'] = (target, 'appimage')
    for arch, target in [('x86_64', 'x86_64-pc-windows-msvc'), ('arm64', 'aarch64-pc-windows-msvc')]:
        result[f'DirectPaymentTimesheets-{v}-windows-{arch}-setup.exe'] = (target, 'windows')
    for arch, target in [('arm64', 'aarch64-apple-darwin'), ('x86_64', 'x86_64-apple-darwin')]:
        result[f'DirectPaymentTimesheets-{v}-macos-{arch}.dmg'] = (target, 'macos')
    return result


def collect(directory, source, v):
    expected = expected_packages(v)
    found = {}
    paths = []
    for path in directory.rglob('*'):
        if path.suffix not in {'.deb', '.AppImage', '.exe', '.dmg'}:
            continue
        if path.name not in expected or path.name in found:
            raise ValueError(f'Unexpected or duplicate package: {path.name}')
        data = json.loads(path.with_name(path.name + '.json').read_text())
        target, kind = expected[path.name]
        if (data['source_sha'], data['version'], data['target'], data['installation_kind'], data['file'], data['sha256']) != (source, v, target, kind, path.name, digest(path)):
            raise ValueError(f'Artifact provenance/checksum mismatch: {path.name}')
        checksum = path.with_name(path.name + '.sha256').read_text()
        if checksum != f"{data['sha256']}  {path.name}\n":
            raise ValueError('Checksum sidecar mismatch: ' + path.name)
        found[path.name] = data
        paths.append(path)
    if set(found) != set(expected):
        raise ValueError(f'Missing packages: {sorted(set(expected) - set(found))}')
    for path in paths:
        for item in [path, path.with_name(path.name + '.json'), path.with_name(path.name + '.sha256')]:
            destination = directory / item.name
            if item != destination:
                if destination.exists():
                    raise ValueError('Duplicate collection file: ' + item.name)
                shutil.move(item, destination)
    (directory / 'rehearsal-manifest.json').write_text(json.dumps(list(found.values()), indent=2) + '\n')
    (directory / 'SHA256SUMS').write_text(''.join(f"{found[n]['sha256']}  {n}\n" for n in sorted(found)))


def main():
    parser = argparse.ArgumentParser(__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('version')
    for command in ['stamp-binary', 'verify-binary']:
        p = sub.add_parser(command); p.add_argument('binary', type=Path); p.add_argument('target'); p.add_argument('kind', choices=KINDS)
    p = sub.add_parser('licenses'); p.add_argument('destination', type=Path)
    p = sub.add_parser('elf'); p.add_argument('target', choices=TARGETS); p.add_argument('binary', type=Path)
    p = sub.add_parser('pe'); p.add_argument('target', choices=WINDOWS_MACHINES); p.add_argument('binary', type=Path)
    p = sub.add_parser('record'); p.add_argument('artifact', type=Path); p.add_argument('target'); p.add_argument('kind', choices=KINDS)
    p = sub.add_parser('collect'); p.add_argument('directory', type=Path); p.add_argument('source'); p.add_argument('version')
    args = parser.parse_args()
    if args.command == 'version': print(version())
    elif args.command == 'licenses': licenses(args.destination)
    elif args.command == 'elf':
        text = subprocess.check_output(['readelf', '-h', '-A', '--version-info', str(args.binary)], text=True)
        validate_elf(text, args.target)
    elif args.command in ['stamp-binary', 'verify-binary']:
        binary_provenance(args.binary, args.target, args.kind, verify=args.command == 'verify-binary')
    elif args.command == 'pe': validate_pe(args.binary, args.target)
    elif args.command == 'record': record(args.artifact, args.target, args.kind)
    elif args.command == 'collect': collect(args.directory, args.source, args.version)


if __name__ == '__main__':
    main()
