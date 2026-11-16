#!/usr/bin/env python3
"""Shared, stdlib-only packaging metadata and validation. Never starts the app."""
import argparse
from contextlib import contextmanager
import hashlib
import io
import json
import os
import plistlib
from pathlib import Path
import re
import shutil
import subprocess
import struct
import stat
import tomllib
import uuid

ROOT = Path(__file__).resolve().parent.parent
TARGETS = {
    'x86_64-unknown-linux-gnu': ('amd64', 'x86_64', 'ELF64', 'Advanced Micro Devices X86-64'),
    'armv7-unknown-linux-gnueabihf': ('armhf', 'armhf', 'ELF32', 'ARM'),
    'aarch64-unknown-linux-gnu': ('arm64', 'aarch64', 'ELF64', 'AArch64'),
}
KINDS = {'deb', 'appimage', 'windows', 'macos'}


def macos_deployment_target(target):
    targets = json.loads((ROOT / 'packaging/toolchain.json').read_text())['macos_deployment_targets']
    if target not in targets:
        raise ValueError('Unsupported macOS target: ' + target)
    return targets[target]


def validate_macos(target, info_plist, deployment):
    expected = macos_deployment_target(target)
    with info_plist.open('rb') as source:
        if plistlib.load(source).get('LSMinimumSystemVersion') != expected:
            raise ValueError('Info.plist minimum must be ' + expected + ' for ' + target)
    # vtool must report exactly the selected minimum, not a higher SDK default.
    versions = re.findall(r'^\s*minos\s+(\S+)\s*$', deployment, re.M)
    if len(versions) != 1 or versions[0] not in {expected, expected + '.0'}:
        raise ValueError('Mach-O minos must be ' + expected + ' for ' + target)


def version():
    value = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
    if not re.fullmatch(r'\d+\.\d+\.\d+', value):
        raise ValueError('Packaging requires a numeric major.minor.patch version')
    return value


def digest(path):
    flags = os.O_RDONLY | getattr(os, 'O_BINARY', 0) | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_NONBLOCK', 0)
    with os.fdopen(os.open(path, flags), 'rb') as source:
        if not stat.S_ISREG(os.fstat(source.fileno()).st_mode):
            raise ValueError('Hash input must be a regular file')
        if os.name != 'nt':
            os.set_blocking(source.fileno(), True)
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


def notice_tree_files(directory):
    """Enumerate for rejection only; never use discovery as a copy allowlist."""
    files = set()
    try:
        metadata = directory.lstat()
    except FileNotFoundError:
        return files
    if (not stat.S_ISDIR(metadata.st_mode)
            or getattr(metadata, 'st_file_attributes', 0) & 0x400):
        raise ValueError('Non-regular/reparse notice directory refused')
    def traversal_error(error):
        raise error

    for current, directories, names in os.walk(directory, followlinks=False, onerror=traversal_error):
        for path in [Path(current), *(Path(current) / n for n in directories + names)]:
            metadata = path.lstat()
            if (stat.S_ISLNK(metadata.st_mode)
                    or getattr(metadata, 'st_file_attributes', 0) & 0x400
                    or not (stat.S_ISDIR(metadata.st_mode) or stat.S_ISREG(metadata.st_mode))):
                raise ValueError('Non-regular/reparse notice path refused')
        files.update((Path(current) / n).relative_to(directory) for n in names)
    return files


def verify_license_inventory(inventory=None):
    # This reviewed allowlist is independently regenerated from checksum-verified
    # archives, not inferred from the manifest's self-declared notice maps.
    if inventory is None:
        inventory = json.loads((ROOT / 'packaging/notice-allowlist.json').read_text())
    if (not isinstance(inventory, dict) or set(inventory) != {'schema', 'lock_sha256', 'files'}
            or type(inventory.get('schema')) is not int or not isinstance(inventory.get('files'), dict)
            or not inventory['files']):
        raise ValueError('Missing required or malformed approval inventory')
    files = inventory['files']
    if inventory.get('schema') != 1 or inventory.get('lock_sha256') != digest(ROOT / 'Cargo.lock'):
        raise ValueError('Notice allowlist does not match the exact lockfile')
    required = {'LICENSE', 'THIRD_PARTY_LICENSES.md', 'assets/fonts/DejaVu-LICENSE.txt',
                'third-party/crates/manifest.json'}
    locked = {(p['name'], p['version'], p['checksum'])
              for p in tomllib.loads((ROOT / 'Cargo.lock').read_text())['package'] if p.get('source')}
    required.update('third-party/crates/' + n + '-' + v + '/Cargo.toml' for n, v, _ in locked)
    if not locked or not required.issubset(files):
        raise ValueError('Missing required notice inventory or attribution')
    allowed = set()
    for name, expected in files.items():
        if not isinstance(name, str) or not isinstance(expected, str):
            raise ValueError('Malformed approval inventory entry')
        relative = Path(name)
        if (relative.is_absolute() or '..' in relative.parts or relative.as_posix() != name
                or not re.fullmatch(r'[0-9a-f]{64}', expected)):
            raise ValueError('Unsafe notice allowlist entry')
        if name.startswith('third-party/'):
            allowed.add(relative.relative_to('third-party'))
        elif name not in required:
            raise ValueError('Unexpected notice allowlist entry')
    if notice_tree_files(ROOT / 'third-party') != allowed:
        raise ValueError('Missing or unexpected notice files; review exact allowlist')
    for name, expected in files.items():
        path = ROOT / name
        # Check each ancestor too: never follow a substituted notice directory.
        for component in (path, *path.parents):
            if component == ROOT:
                break
            metadata = component.lstat()
            if stat.S_ISLNK(metadata.st_mode) or getattr(metadata, 'st_file_attributes', 0) & 0x400:
                raise ValueError('Symlink/reparse notice path refused')
        if not stat.S_ISREG(path.lstat().st_mode) or digest(path) != expected:
            raise ValueError('Missing/modified upstream attribution; regenerate and review notices')
    records = json.loads((ROOT / 'third-party/crates/manifest.json').read_text())['packages']
    recorded = {(p['name'], p['version'], p['archive_sha256']) for p in records}
    if locked != recorded or len(records) != len(recorded):
        raise ValueError('Licence inventory does not match the exact lockfile')
    for package in records:
        prefix = 'third-party/crates/' + package['name'] + '-' + package['version'] + '/'
        required_notices = {name[len(prefix):]: value for name, value in files.items()
                            if name.startswith(prefix)}
        if not required_notices or package.get('notices') != required_notices:
            raise ValueError('Missing required upstream attribution in manifest')
    return sorted(allowed)


def approved_inventory(inventory=None):
    if inventory is None:
        inventory = json.loads((ROOT / 'packaging/notice-allowlist.json').read_text())
    verify_license_inventory(inventory)
    return inventory


def check_destination(path):
    """Reject every existing unsafe component before any destination write."""
    path = Path(os.path.abspath(path))
    for component in reversed((path, *path.parents)):
        try:
            metadata = component.lstat()
        except FileNotFoundError:
            continue
        if (stat.S_ISLNK(metadata.st_mode) or getattr(metadata, 'st_file_attributes', 0) & 0x400
                or not (stat.S_ISREG(metadata.st_mode) if component == path else stat.S_ISDIR(metadata.st_mode))):
            raise ValueError('Unsafe destination path refused before writing')
    return path


@contextmanager
def destination_parent(path):
    if os.name == 'nt':
        with locked_windows_paths([], directories=[path.parent], create=True):
            yield None
        return
    descriptor = os.open('/', os.O_RDONLY | os.O_DIRECTORY)
    try:
        for part in path.parent.parts[1:]:
            try:
                os.mkdir(part, 0o755, dir_fd=descriptor)
            except FileExistsError:
                pass
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = child
        yield descriptor
    finally:
        os.close(descriptor)


def safe_copy(source, destination, expected=None, readonly=False):
    destination = check_destination(destination)
    with destination_parent(destination) as parent:
        temporary = '.dpts-' + uuid.uuid4().hex
        temp_path = destination.parent / temporary
        options = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_BINARY', 0)
        if parent is None:
            descriptor = os.open(temp_path, options, 0o600)
        else:
            descriptor = os.open(temporary, options | os.O_NOFOLLOW, 0o600, dir_fd=parent)
        try:
            with os.fdopen(descriptor, 'wb') as output:
                flags = os.O_RDONLY | getattr(os, 'O_BINARY', 0) | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_NONBLOCK', 0)
                data_input = isinstance(source, bytes)
                if not data_input:
                    check_destination(source)
                with (io.BytesIO(source) if data_input else os.fdopen(os.open(source, flags), 'rb')) as incoming:
                    metadata = None if data_input else os.fstat(incoming.fileno())
                    if metadata is not None and not stat.S_ISREG(metadata.st_mode):
                        raise ValueError('Non-regular copy input')
                    if os.name != 'nt' and not data_input:
                        os.set_blocking(incoming.fileno(), True)
                    hasher = hashlib.sha256()
                    for chunk in iter(lambda: incoming.read(1024 * 1024), b''):
                        hasher.update(chunk)
                        output.write(chunk)
                    checksum = hasher.hexdigest()
                    if expected is not None and checksum != expected:
                        raise ValueError('Copied input hash mismatch; packaging aborted')
                output.flush()
                os.fsync(output.fileno())
                if os.name != 'nt':
                    mode = 0o755 if metadata is not None and metadata.st_mode & 0o111 else 0o644
                    os.fchmod(output.fileno(), mode & ~0o222 if readonly else mode)
            # Recheck final links; atomic replacement never writes through them.
            check_destination(destination)
            if parent is None:
                os.replace(temp_path, destination)
            else:
                os.replace(temporary, destination.name, src_dir_fd=parent, dst_dir_fd=parent)
            return checksum
        finally:
            try:
                if parent is None:
                    temp_path.unlink()
                else:
                    os.unlink(temporary, dir_fd=parent)
            except FileNotFoundError:
                pass


def staged_notice_inventory(inventory):
    return {('DejaVu-LICENSE.txt' if name == 'assets/fonts/DejaVu-LICENSE.txt' else name): value
            for name, value in inventory['files'].items()}


def verify_notice_stage(destination, inventory=None):
    inventory = approved_inventory(inventory)
    expected = staged_notice_inventory(inventory)
    allowed = {Path(name).relative_to('third-party') for name in expected
               if name.startswith('third-party/')}
    if notice_tree_files(destination / 'third-party') != allowed:
        raise ValueError('Missing or unexpected staged notice files')
    for name, checksum in expected.items():
        path = destination / name
        for component in (path, *path.parents):
            metadata = component.lstat()
            if stat.S_ISLNK(metadata.st_mode) or getattr(metadata, 'st_file_attributes', 0) & 0x400:
                raise ValueError('Symlink/reparse staged notice path refused')
            if component == destination:
                break
        # Opening must not block if a path is replaced by a FIFO after lstat.
        descriptor = os.open(path, os.O_RDONLY | getattr(os, 'O_BINARY', 0)
                             | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_NONBLOCK', 0))
        with os.fdopen(descriptor, 'rb') as source:
            if not stat.S_ISREG(os.fstat(source.fileno()).st_mode):
                raise ValueError('Staged notice hash/type mismatch; packaging aborted')
            if os.name != 'nt':
                os.set_blocking(source.fileno(), True)
            if hashlib.file_digest(source, 'sha256').hexdigest() != checksum:
                raise ValueError('Staged notice hash/type mismatch; packaging aborted')
    return expected


def licenses(destination):
    inventory = approved_inventory()
    expected = staged_notice_inventory(inventory)
    allowed = {Path(name).relative_to('third-party') for name in expected if name.startswith('third-party/')}
    if notice_tree_files(destination / 'third-party') - allowed:
        raise ValueError('Unexpected files in notice staging destination')
    for name in expected:
        check_destination(destination / name)
    for name, checksum in inventory['files'].items():
        target = 'DejaVu-LICENSE.txt' if name == 'assets/fonts/DejaVu-LICENSE.txt' else name
        safe_copy(ROOT / name, destination / target, checksum)
    verify_notice_stage(destination, inventory)


@contextmanager
def locked_windows_paths(paths, directories=(), create=False):
    """Keep NSIS input files and their path ancestors immutable while compiling.

    Files deny write/delete sharing. Directory handles deny rename/delete but
    allow ordinary directory access. OPEN_REPARSE_POINT avoids following a
    substituted final component; validation runs again with these handles held.
    """
    if os.name != 'nt':
        raise ValueError('Locked NSIS staging requires native Windows')
    import ctypes
    from ctypes import wintypes
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    create = kernel.CreateFileW
    create.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
                       ctypes.c_void_p, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
    create.restype = wintypes.HANDLE
    close = kernel.CloseHandle
    close.argtypes = [wintypes.HANDLE]
    close.restype = wintypes.BOOL
    paths = [Path(os.path.abspath(path)) for path in paths]
    directories = set(directories) | {parent for path in paths for parent in path.parents}
    directories |= {parent for path in list(directories) for parent in path.parents}
    handles = []
    try:
        for path, directory in ([(p, True) for p in sorted(directories, key=lambda p: len(p.parts))]
                                + [(p, False) for p in paths]):
            if directory and create:
                path.mkdir(exist_ok=True)
            # OPEN_EXISTING; directories use BACKUP_SEMANTICS too.
            handle = create(str(path), 0 if directory else 0x80000000,
                            3 if directory else 1, None, 3,
                            0x00200000 | (0x02000000 if directory else 0), None)
            if handle == ctypes.c_void_p(-1).value:
                raise ctypes.WinError(ctypes.get_last_error())
            handles.append(handle)
            metadata = path.lstat()
            if (stat.S_ISLNK(metadata.st_mode)
                    or getattr(metadata, 'st_file_attributes', 0) & 0x400
                    or (not stat.S_ISDIR(metadata.st_mode) if directory else not stat.S_ISREG(metadata.st_mode))):
                raise ValueError('Non-regular/reparse NSIS staging path refused')
        yield
    finally:
        for handle in reversed(handles):
            close(handle)


@contextmanager
def locked_windows_notice_stage(destination, inventory, inputs=None):
    inventory = approved_inventory(inventory)
    paths = [destination / name for name in staged_notice_inventory(inventory)]
    paths.extend(Path(name) for name in (inputs or {}))
    with locked_windows_paths(paths):
        yield


def verify_inputs(inputs):
    if not isinstance(inputs, dict) or not inputs:
        raise ValueError('Missing NSIS input inventory')
    for name, expected in inputs.items():
        check_destination(Path(name))
        if not isinstance(expected, str) or not re.fullmatch(r'[0-9a-f]{64}', expected) or digest(name) != expected:
            raise ValueError('NSIS input hash mismatch')


def run_verified_notice_command(destination, command, input_manifest=None):
    if not command or input_manifest is None:
        raise ValueError('Missing NSIS command/input inventory')
    inventory = approved_inventory()
    inputs = json.loads(input_manifest.read_text())
    with locked_windows_notice_stage(destination, inventory, {**inputs, str(input_manifest): digest(input_manifest)}):
        verify_inputs(inputs)
        verify_notice_stage(destination, inventory)
        subprocess.run(command, check=True)
        verify_inputs(inputs)
        verify_notice_stage(destination, inventory)


def snapshot_artifact(source, destination, expected=None):
    if destination.exists():
        raise ValueError('Artifact snapshot must be new')
    return safe_copy(source, destination, expected, readonly=True)


@contextmanager
def artifact_output_lock(destination):
    lock = check_destination(destination.with_name('.' + destination.name + '.packaging.lock'))
    with destination_parent(lock) as parent:
        flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0)
        descriptor = os.open(lock if parent is None else lock.name, flags, 0o600,
                             **({} if parent is None else {'dir_fd': parent}))
        try:
            yield
        finally:
            os.close(descriptor)
            if parent is None:
                lock.unlink()
            else:
                os.unlink(lock.name, dir_fd=parent)


def promote_verified(source, destination, target, kind, checksum):
    if not re.fullmatch(r'[0-9a-f]{64}', checksum) or digest(source) != checksum:
        raise ValueError('Verified artifact changed before promotion')
    with artifact_output_lock(destination):
        for path in [destination, destination.with_name(destination.name + '.json'), destination.with_name(destination.name + '.sha256')]:
            check_destination(path)
        safe_copy(source, destination, checksum)
        record(destination, target, kind, checksum)


def verify_windows_payload(extracted, expected):
    if isinstance(expected, Path):
        expected = json.loads(expected.read_text())
    if not isinstance(expected, dict) or not expected:
        raise ValueError('Missing Windows payload inventory')
    notices = verify_notice_stage(extracted)
    if any(expected.get(name) != value for name, value in notices.items()):
        raise ValueError('Windows payload inventory omits approved notices')
    actual = {p.as_posix() for p in notice_tree_files(extracted)}
    if actual != set(expected):
        raise ValueError('Unexpected or missing installer payload')
    for name, checksum in expected.items():
        relative = Path(name)
        if (relative.is_absolute() or '..' in relative.parts or relative.as_posix() != name
                or not isinstance(checksum, str) or not re.fullmatch(r'[0-9a-f]{64}', checksum)):
            raise ValueError('Unsafe Windows payload inventory')
        check_destination(extracted / relative)
        if digest(extracted / relative) != checksum:
            raise ValueError('Installer payload hash mismatch')


def prepare_windows_build(binary, compiler, extractor, build, publish, target):
    inventory = approved_inventory()
    provenance = binary_provenance(binary, target, 'windows', verify=True)
    if build.exists():
        raise ValueError('Windows build directory must be unique and new')
    check_destination(build / '.build-root')
    marker = build / '.build-root'
    with destination_parent(marker) as parent:
        descriptor = os.open(marker if parent is None else marker.name, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600,
                             **({} if parent is None else {'dir_fd': parent}))
        os.close(descriptor)
    stage = build / 'notices'
    licenses(stage)
    inputs, payload = {}, staged_notice_inventory(inventory)

    def emit(name, data):
        path = build / name
        checksum = hashlib.sha256(data).hexdigest()
        safe_copy(data, path, checksum)
        inputs[str(path)] = checksum
        return path

    staged_binary = build / 'inputs/DirectPaymentTimesheets.exe'
    safe_copy(binary, staged_binary, provenance['sha256'])
    validate_pe(staged_binary, target)
    inputs[str(staged_binary)] = payload['DirectPaymentTimesheets.exe'] = provenance['sha256']
    controls = {}
    for source, name in [('packaging/windows/installer.nsi', 'installer.nsi'),
                         ('packaging/package.py', 'package.py'),
                         ('assets/direct-payment-timesheets.ico', 'direct-payment-timesheets.ico')]:
        data = subprocess.check_output(['git', '-C', str(ROOT), 'show', provenance['source_sha'] + ':' + source])
        controls[name] = emit('inputs/' + name, data)
    icon = controls['direct-payment-timesheets.ico']
    payload['direct-payment-timesheets.ico'] = inputs[str(icon)]
    # NSIS x64/WinVer macros use this pinned toolchain's System plug-in.
    plugin = compiler.parent / 'Plugins/x86-unicode/System.dll'
    inputs[str(plugin)] = payload['$PLUGINSDIR/System.dll'] = digest(plugin)
    # Include templates, stubs and resources also influence compiled code.
    for directory in ['Include', 'Stubs', 'Plugins', 'Contrib']:
        base = compiler.parent / directory
        for relative in notice_tree_files(base):
            inputs[str(base / relative)] = digest(base / relative)
    inputs[str(compiler)] = digest(compiler)
    inputs[str(extractor)] = digest(extractor)
    for source in ['Cargo.lock', 'packaging/notice-allowlist.json']:
        inputs[str(ROOT / source)] = digest(ROOT / source)
    install, uninstall, directories = [], [], set()
    for name in sorted(n for n in payload if n.startswith('third-party/')):
        relative = Path(name).relative_to('third-party')
        parent = relative.parent
        output_dir = '$INSTDIR\\third-party' + ('' if str(parent) == '.' else '\\' + str(parent).replace('$', '$$'))
        install += ['SetOutPath "' + output_dir + '"', 'File "' + str(stage / name).replace('$', '$$') + '"']
        uninstall.append('Delete "$INSTDIR\\' + str(Path(name)).replace('$', '$$') + '"')
        while str(parent) != '.':
            directories.add(parent)
            parent = parent.parent
    for directory in sorted(directories, key=lambda p: (-len(p.parts), str(p))):
        uninstall.append('RMDir "$INSTDIR\\third-party\\' + str(directory).replace('$', '$$') + '"')
    uninstall.append('RMDir "$INSTDIR\\third-party"')
    emit('third-party-install.nsh', ('\n'.join(install) + '\n').encode())
    emit('third-party-uninstall.nsh', ('\n'.join(uninstall) + '\n').encode())
    candidate = build / 'candidate' / publish.name
    check_destination(candidate)
    with destination_parent(candidate):
        pass
    command = [str(compiler), '/NOCONFIG', '/INPUTCHARSET', 'UTF8',
               '/DARCHITECTURE=' + ('arm64' if target.startswith('aarch64') else 'x86_64'),
               '/DSOURCE_BINARY=' + str(staged_binary), '/DPROJECT_ROOT=' + str(ROOT),
               '/DBUILD_DIR=' + str(build), '/DNOTICE_STAGE=' + str(stage),
               '/DICON_FILE=' + str(icon), '/DPACKAGE_HELPER=' + str(controls['package.py']),
               '/DPYTHON_EXE=' + os.sys.executable, '/DVERSION=' + version(),
               '/DOUTPUT_FILE=' + str(candidate), str(controls['installer.nsi'])]
    spec = dict(schema=1, files=inputs, payload=payload, command=command, stage=str(stage),
                artifact=str(candidate), publish=str(publish), target=target,
                extractor=str(extractor), generated=str(build / 'generated-uninstaller.json'))
    data = (json.dumps(spec, indent=2, sort_keys=True) + '\n').encode()
    path = build / 'build.json'
    checksum = hashlib.sha256(data).hexdigest()
    safe_copy(data, path, checksum)
    return checksum


def capture_uninstaller(source, destination):
    # Called synchronously by NSIS !uninstfinalize, not by installer execution.
    checksum = digest(source)
    data = (json.dumps({'Uninstall.exe': checksum}) + '\n').encode()
    safe_copy(data, destination, hashlib.sha256(data).hexdigest())
    print('DPTS_UNINSTALLER_SHA256:' + checksum, flush=True)


def capture_installer(source):
    print('DPTS_INSTALLER_SHA256:' + digest(source), flush=True)


def compile_windows_build(manifest, expected):
    data = manifest.read_bytes()
    if not re.fullmatch(r'[0-9a-f]{64}', expected) or hashlib.sha256(data).hexdigest() != expected:
        raise ValueError('Windows build manifest changed')
    spec = json.loads(data)
    if spec.get('schema') != 1:
        raise ValueError('Malformed Windows build manifest')
    inputs = {**spec['files'], str(manifest): expected}
    inventory = approved_inventory()
    stage = Path(spec['stage'])
    with locked_windows_notice_stage(stage, inventory, inputs):
        verify_inputs(inputs)
        verify_notice_stage(stage, inventory)
        result = subprocess.run(spec['command'], check=True, stdout=subprocess.PIPE, text=True)
        generated = re.findall(r'^DPTS_UNINSTALLER_SHA256:([0-9a-f]{64})\r?$', result.stdout, re.M)
        if len(generated) != 1:
            raise ValueError('Missing/ambiguous generated uninstaller digest')
        container = re.findall(r'^DPTS_INSTALLER_SHA256:([0-9a-f]{64})\r?$', result.stdout, re.M)
        if len(container) != 1:
            raise ValueError('Missing/ambiguous compiled installer digest')
        verify_inputs(inputs)
        verify_notice_stage(stage, inventory)
        snapshot = manifest.parent / 'verified' / Path(spec['artifact']).name
        checksum = snapshot_artifact(Path(spec['artifact']), snapshot, container[0])
        extracted = manifest.parent / 'extracted'
        with locked_windows_paths([snapshot]):
            subprocess.run([spec['extractor'], 'x', '-y', '-o' + str(extracted), str(snapshot)], check=True)
            verify_windows_payload(extracted, {**spec['payload'], 'Uninstall.exe': generated[0]})
            if digest(snapshot) != checksum:
                raise ValueError('Installer snapshot changed during extraction')
            promote_verified(snapshot, Path(spec['publish']), spec['target'], 'windows', checksum)


def source_sha():
    status = subprocess.check_output(['git', '--no-optional-locks', '-c', f'safe.directory={ROOT}',
        '-C', str(ROOT), 'status', '--porcelain=v1', '--untracked-files=all'], text=True)
    if status:
        # Never serialize filenames from a private working tree into metadata.
        raise ValueError('Packaging requires an exact clean committed source snapshot; review and commit changes before release packaging')
    source = subprocess.check_output(['git', '-c', f'safe.directory={ROOT}', '-C', str(ROOT), 'rev-parse', 'HEAD'], text=True).strip()
    if os.environ.get('SOURCE_SHA', source) != source:
        raise ValueError('Checkout differs from captured source SHA')
    return source


def binary_provenance(binary, target, kind, verify=False):
    verify_license_inventory()
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
    return expected


def record(artifact, target, kind, verified_sha256=None):
    if kind not in KINDS or not artifact.is_file():
        raise ValueError('Missing artifact or unsupported installation kind')
    if verified_sha256 is None:
        raise ValueError('Recording requires the verified artifact digest')
    if verified_sha256 is not None and digest(artifact) != verified_sha256:
        raise ValueError('Verified artifact changed before recording')
    source = source_sha()
    if expected_packages(version()).get(artifact.name) != (target, kind):
        raise ValueError('Artifact filename does not match version, target and installation kind')
    metadata = {
        'source_sha': source, 'version': version(), 'target': target,
        'installation_kind': kind, 'file': artifact.name, 'sha256': verified_sha256 or digest(artifact),
        'rust_version': json.loads((ROOT / 'packaging/toolchain.json').read_text())['rust'], 'run_id': os.environ.get('GITHUB_RUN_ID'),
        'packaging_tools': json.loads((ROOT / 'packaging/toolchain.json').read_text()),
        'appimage_tool_assets': json.loads((ROOT / 'packaging/linux/tools.json').read_text()) if kind == 'appimage' else None,
    }
    if kind == 'macos':
        metadata['macos_deployment_target'] = macos_deployment_target(target)
    metadata_path = artifact.with_name(artifact.name + '.json')
    checksum_path = artifact.with_name(artifact.name + '.sha256')
    check_destination(metadata_path)
    check_destination(checksum_path)
    safe_copy((json.dumps(metadata, indent=2) + '\n').encode(), metadata_path)
    safe_copy(f"{metadata['sha256']}  {artifact.name}\n".encode(), checksum_path)
    if digest(artifact) != metadata['sha256']:
        raise ValueError('Artifact changed while recording provenance')


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
        if kind == 'macos' and data.get('macos_deployment_target') != macos_deployment_target(target):
            raise ValueError('macOS deployment target mismatch: ' + path.name)
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
    p = sub.add_parser('macos-target'); p.add_argument('target')
    p = sub.add_parser('macos'); p.add_argument('target'); p.add_argument('info_plist', type=Path); p.add_argument('deployment', type=Path)
    for command in ['stamp-binary', 'verify-binary']:
        p = sub.add_parser(command); p.add_argument('binary', type=Path); p.add_argument('target'); p.add_argument('kind', choices=KINDS)
    sub.add_parser('notice-files')
    p = sub.add_parser('verify-notice-stage'); p.add_argument('destination', type=Path)
    p = sub.add_parser('run-verified-notice-command'); p.add_argument('destination', type=Path); p.add_argument('input_manifest', type=Path); p.add_argument('argv', nargs=argparse.REMAINDER)
    p = sub.add_parser('prepare-windows-build'); p.add_argument('binary', type=Path); p.add_argument('compiler', type=Path); p.add_argument('extractor', type=Path); p.add_argument('build', type=Path); p.add_argument('publish', type=Path); p.add_argument('target')
    p = sub.add_parser('compile-windows-build'); p.add_argument('manifest', type=Path); p.add_argument('checksum')
    p = sub.add_parser('capture-installer'); p.add_argument('source', type=Path)
    p = sub.add_parser('capture-uninstaller'); p.add_argument('source', type=Path); p.add_argument('destination', type=Path)
    p = sub.add_parser('snapshot-artifact'); p.add_argument('source', type=Path); p.add_argument('destination', type=Path)
    p = sub.add_parser('promote-verified'); p.add_argument('source', type=Path); p.add_argument('destination', type=Path); p.add_argument('target'); p.add_argument('kind', choices=KINDS); p.add_argument('checksum')
    p = sub.add_parser('verify-windows-payload'); p.add_argument('extracted', type=Path); p.add_argument('manifest', type=Path)
    p = sub.add_parser('licenses'); p.add_argument('destination', type=Path)
    p = sub.add_parser('elf'); p.add_argument('target', choices=TARGETS); p.add_argument('binary', type=Path)
    p = sub.add_parser('pe'); p.add_argument('target', choices=WINDOWS_MACHINES); p.add_argument('binary', type=Path)
    p = sub.add_parser('record'); p.add_argument('artifact', type=Path); p.add_argument('target'); p.add_argument('kind', choices=KINDS)
    p = sub.add_parser('collect'); p.add_argument('directory', type=Path); p.add_argument('source'); p.add_argument('version')
    args = parser.parse_args()
    if args.command == 'version': print(version())
    elif args.command == 'macos-target': print(macos_deployment_target(args.target))
    elif args.command == 'macos': validate_macos(args.target, args.info_plist, args.deployment.read_text())
    elif args.command == 'notice-files': print(json.dumps([p.as_posix() for p in verify_license_inventory()]))
    elif args.command == 'verify-notice-stage': verify_notice_stage(args.destination)
    elif args.command == 'run-verified-notice-command': run_verified_notice_command(args.destination, args.argv, args.input_manifest)
    elif args.command == 'prepare-windows-build': print(prepare_windows_build(args.binary, args.compiler, args.extractor, args.build, args.publish, args.target))
    elif args.command == 'compile-windows-build': compile_windows_build(args.manifest, args.checksum)
    elif args.command == 'capture-installer': capture_installer(args.source)
    elif args.command == 'capture-uninstaller': capture_uninstaller(args.source, args.destination)
    elif args.command == 'snapshot-artifact': print(snapshot_artifact(args.source, args.destination))
    elif args.command == 'promote-verified': promote_verified(args.source, args.destination, args.target, args.kind, args.checksum)
    elif args.command == 'verify-windows-payload': verify_windows_payload(args.extracted, args.manifest)
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
