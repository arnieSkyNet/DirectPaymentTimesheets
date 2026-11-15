"""Versioned positive-schema receipts. Never serialize child output or fixture bytes.

A digest checks canonical receipt bytes, not authenticity. Public delivery requires
separate approval. Retention metadata is a minimum policy, not a host lifetime.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import stat

SCHEMA = 'dpt-native-sqlite-receipt/v1'
CASES = ('normal', 'retry', 'unwind', 'terminal', 'panic_terminal', 'rollback', 'wal', 'stall')
PHASES = ('privacy_ready', 'handles_ready', 'busy_confirmed', 'terminal_ready', 'panic_ready',
          'pending_ready', 'wal_committed_only', 'stall_ready', 'completed', 'guards_dropped',
          'staging_released', 'unexpected_close_failure', 'unsupported_privacy')
CLEANUP = ('leader_reaped', 'namespace_empty', 'supervisor_handles_closed', 'exclusive_released')
ROLES = ('database', 'journal', 'wal', 'shm')
STATES = ('PASS', 'FAIL', 'UNVERIFIED', 'NOT_RUN')


class ReceiptRefusal(Exception):
    """Fixed output only, including validation failures."""


def require(ok):
    if not ok:
        raise ReceiptRefusal('receipt_refused')


def keys(value, names):
    require(type(value) is dict and set(value) == set(names))


def boolean(value):
    require(value is None or type(value) is bool)


def hex_value(value, size, nullable=False):
    require((nullable and value is None) or (type(value) is str and re.fullmatch('[0-9a-f]{%d}' % size, value) is not None and value != '0' * size))


def exit_code(value):
    require(value is None or (type(value) is int and -255 <= value <= 0xffffffff))


def empty_case():
    return dict(state='NOT_RUN', exit_code=None, timed_out=False,
                phases={name: None for name in PHASES}, cleanup={name: None for name in CLEANUP},
                recovery=dict(attempted=False, exit_code=None, timed_out=False, cleanup={name: None for name in CLEANUP}, privacy_ready=None, confirmed=None, integrity_ok=None,
                              rows='NONE', pre={}, post={}),
                source_verified=False, storage_closed=False)


def new(run, hashes):
    data = dict(schema=SCHEMA, run=run, hashes=hashes, cases={name: empty_case() for name in CASES},
                final=dict(state='UNVERIFIED', driver_exit=None, source_verified=False,
                           storage_closed=False, complete=False))
    validate(data)
    return data


def validate(data):
    keys(data, ('schema', 'run', 'hashes', 'cases', 'final'))
    require(data['schema'] == SCHEMA)
    run = data['run']
    keys(run, ('profile', 'run_id', 'attempt', 'S', 'W', 'L', 'architecture', 'python', 'rust', 'retention_days'))
    require(run['profile'] in ('github', 'molly-local'))
    require(type(run['run_id']) is str and re.fullmatch(r'[0-9]{1,20}', run['run_id']) is not None)
    require(type(run['attempt']) is int and run['attempt'] == 1)  # No retry attestation.
    require(type(run['retention_days']) is int and 90 <= run['retention_days'] <= 3650)
    for field in ('S', 'W'):
        hex_value(run[field], 40)
    hex_value(run['L'], 40, nullable=run['profile'] == 'molly-local')
    require(run['profile'] != 'github' or run['run_id'] != '0')
    require(run['architecture'] in ('windows-x86_64', 'macos-arm64', 'macos-x86_64'))
    require(run['profile'] != 'molly-local' or run['architecture'] == 'windows-x86_64')
    for field in ('python', 'rust'):
        require(type(run[field]) is str and re.fullmatch(r'[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}', run[field]) is not None)
    keys(data['hashes'], ('fixture_source', 'source_inventory', 'controller_inventory', 'manifest', 'executable'))
    for value in data['hashes'].values():
        hex_value(value, 64)
    keys(data['cases'], CASES)
    for name, case in data['cases'].items():
        keys(case, ('state', 'exit_code', 'timed_out', 'phases', 'cleanup', 'recovery', 'source_verified', 'storage_closed'))
        require(type(case['state']) is str and case['state'] in STATES)
        exit_code(case['exit_code'])
        require(type(case['timed_out']) is bool)
        for field in ('source_verified', 'storage_closed'):
            require(type(case[field]) is bool)
        keys(case['phases'], PHASES)
        keys(case['cleanup'], CLEANUP)
        for value in (*case['phases'].values(), *case['cleanup'].values()):
            boolean(value)
        recovery = case['recovery']
        keys(recovery, ('attempted', 'exit_code', 'timed_out', 'cleanup', 'privacy_ready', 'confirmed', 'integrity_ok', 'rows', 'pre', 'post'))
        exit_code(recovery['exit_code'])
        require(type(recovery['attempted']) is bool and type(recovery['timed_out']) is bool)
        keys(recovery['cleanup'], CLEANUP)
        for value in recovery['cleanup'].values():
            boolean(value)
        require(recovery['rows'] in ('NONE', 'BASELINE', 'WAL_COMMITTED'))
        for field in ('privacy_ready', 'confirmed', 'integrity_ok'):
            boolean(recovery[field])
        for field in ('pre', 'post'):
            require(type(recovery[field]) is dict and set(recovery[field]) <= set(ROLES))
            for value in recovery[field].values():
                keys(value, ('sha256', 'size'))
                hex_value(value['sha256'], 64)
                require(type(value['size']) is int and 0 <= value['size'] <= 2**40)
        cleanup = case['cleanup']
        known_cleanup = all(cleanup[k] is True for k in CLEANUP[:3]) and (not recovery['attempted'] or all(recovery['cleanup'][k] is True for k in CLEANUP[:3]))
        if case['state'] == 'NOT_RUN':
            require(case == empty_case())
        if case['state'] in ('PASS', 'FAIL'):
            require(known_cleanup)
            if run['architecture'] == 'windows-x86_64':
                require(cleanup['exclusive_released'] is True and (not recovery['attempted'] or recovery['cleanup']['exclusive_released'] is True))
        if case['state'] == 'PASS':
            require(case['source_verified'] and case['storage_closed'])
            require(cleanup['exclusive_released'] is (True if run['architecture'] == 'windows-x86_64' else None))
            phase = case['phases']
            require(phase['privacy_ready'] is True and phase['handles_ready'] is True)
            require(phase['unsupported_privacy'] is False and phase['unexpected_close_failure'] is False)
            if name in ('normal', 'retry', 'unwind'):
                require(case['exit_code'] == 0 and not case['timed_out'])
                require(all(phase[k] is True for k in ('completed', 'guards_dropped', 'staging_released')))
                if name == 'retry':
                    require(phase['busy_confirmed'] is True)
            else:
                require(all(phase[k] is False for k in ('completed', 'guards_dropped', 'staging_released')))
                if name == 'stall':
                    require(case['timed_out'] and phase['stall_ready'] is True and phase['terminal_ready'] is False)
                    require(case['exit_code'] == (1 if run['architecture'] == 'windows-x86_64' else -9))
                else:
                    require(case['exit_code'] == 86 and not case['timed_out'])
                    require(phase['busy_confirmed'] is True and phase['terminal_ready'] is True)
                    if name == 'panic_terminal':
                        require(phase['panic_ready'] is True)
                if name in ('rollback', 'wal'):
                    require(phase['pending_ready'] is True)
                    require(recovery['attempted'] and not recovery['timed_out'] and recovery['exit_code'] == 0 and all(recovery[k] is True for k in ('privacy_ready', 'confirmed', 'integrity_ok')))
                    require(recovery['cleanup']['exclusive_released'] is (True if run['architecture'] == 'windows-x86_64' else None))
                    require(recovery['rows'] == ('BASELINE' if name == 'rollback' else 'WAL_COMMITTED'))
                    require('database' in recovery['pre'] and 'database' in recovery['post'])
                    require(('journal' if name == 'rollback' else 'wal') in recovery['pre'])
                    if name == 'wal':
                        require(phase['wal_committed_only'] is True)
        if name not in ('rollback', 'wal'):
            require(recovery == empty_case()['recovery'])
    final = data['final']
    keys(final, ('state', 'driver_exit', 'source_verified', 'storage_closed', 'complete'))
    require(final['state'] in ('PASS', 'FAIL', 'UNVERIFIED'))
    exit_code(final['driver_exit'])
    for field in ('source_verified', 'storage_closed', 'complete'):
        require(type(final[field]) is bool)
    passed = all(case['state'] == 'PASS' for case in data['cases'].values())
    if final['state'] == 'PASS':
        require(passed and final['driver_exit'] == 0 and all(final[k] for k in ('source_verified', 'storage_closed', 'complete')))
    else:
        require(not final['complete'] and final['driver_exit'] != 0)
        if any(c['state'] == 'UNVERIFIED' for c in data['cases'].values()):
            require(final['state'] == 'UNVERIFIED')
    return data


def canonical(data):
    validate(data)
    return json.dumps(data, sort_keys=True, separators=(',', ':'), ensure_ascii=True, allow_nan=False).encode('ascii') + b'\n'


def parse(raw):
    require(type(raw) is bytes and len(raw) <= 65536)
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result)
            result[key] = value
        return result
    try:
        data = json.loads(raw, object_pairs_hook=pairs, parse_constant=lambda _: require(False))
        validate(data)
        return data
    except Exception:
        raise ReceiptRefusal('receipt_refused') from None


def summary(data):
    validate(data)
    rows = ['Native SQLite synthetic receipt', '', '| Case | State | Exit | Recovery exit |', '|---|---|---:|---:|']
    for name in CASES:
        case = data['cases'][name]
        rows.append('| %s | %s | %s | %s |' % (name, case['state'], case['exit_code'], case['recovery']['exit_code']))
    raw = canonical(data)
    rows += ['', 'Receipt SHA-256: ' + hashlib.sha256(raw).hexdigest(), '', '```json', raw.decode('ascii').strip(), '```', '']
    return '\n'.join(rows)


def write_local(data, path):
    """Create once in a caller-verified private root; never overwrite or follow."""
    from native_sqlite_fixture_storage import no_links, WindowsSecurity, mac_acl_empty
    raw = canonical(data)
    path = Path(path)
    require(path.suffix == '.json')
    no_links(path.parent)
    if os.name == 'nt':
        WindowsSecurity().check(path.parent, protected=True)
    else:
        info = path.parent.stat()
        require(info.st_uid == os.geteuid() and stat.S_IMODE(info.st_mode) == 0o700)
        if __import__('sys').platform == 'darwin':
            mac_acl_empty(path.parent)
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_NOINHERIT', 0)
    fd = os.open(path, flags, 0o600)
    with os.fdopen(fd, 'wb') as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())
    require(path.read_bytes() == raw)
    if os.name == 'nt':
        WindowsSecurity().check(path)
    else:
        info = path.stat()
        require(info.st_uid == os.geteuid() and stat.S_IMODE(info.st_mode) == 0o600 and info.st_nlink == 1)
        if __import__('sys').platform == 'darwin':
            mac_acl_empty(path)
    checksum = hashlib.sha256(raw).hexdigest()
    fd = os.open(path.with_suffix('.sha256'), flags, 0o600)
    with os.fdopen(fd, 'wb') as stream:
        stream.write((checksum + '\n').encode('ascii'))
        stream.flush()
        os.fsync(stream.fileno())
    if os.name != 'nt':
        directory = os.open(path.parent, os.O_RDONLY | getattr(os, 'O_DIRECTORY', 0) | getattr(os, 'O_NOFOLLOW', 0))
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    return checksum


def publish(data):
    """Private local copy + sanitized output only. No artifact API or repo writes."""
    from native_sqlite_fixture_storage import PrivateRoot
    canonical(data)  # Refuse before touching output paths or printing.
    storage = PrivateRoot(prefix='dpt-native-receipt-')
    storage.verify()
    storage.close()  # No success document until metadata-root handles close too.
    write_local(data, storage.path / 'receipt.json')
    target = os.environ.get('GITHUB_STEP_SUMMARY')
    if data['run']['profile'] == 'github':
        require(bool(target))
        # Trusted runner control file, never a fixture-controlled filename.
        with open(target, 'a', encoding='utf-8') as stream:
            stream.write(summary(data))
            stream.flush()
    print('native.sqlite.receipt=' + canonical(data).decode('ascii').strip(), flush=True)
    return storage.path / 'receipt.json'


def main(argv=None):
    import sys
    argv = sys.argv[1:] if argv is None else argv
    if not ((len(argv) == 2 and argv[0] in ('--verify', '--summary')) or (len(argv) == 3 and argv[0] == '--retain')):
        return 2
    from native_sqlite_fixture_storage import no_links
    path = no_links(Path(argv[1]))
    require(path.is_file() and path.stat().st_size <= 65536 and path.stat().st_nlink == 1)
    raw = path.read_bytes()
    data = parse(raw)
    require(raw == canonical(data))
    checksum = hashlib.sha256(raw).hexdigest()
    sidecar = no_links(path.with_suffix('.sha256'))
    require(sidecar.is_file() and sidecar.stat().st_size == 65 and sidecar.stat().st_nlink == 1)
    require(sidecar.read_text(encoding='ascii') == checksum + '\n')
    if argv[0] == '--retain':
        require(write_local(data, Path(argv[2])) == checksum)
    if argv[0] == '--summary':
        print(summary(data))
    else:
        print('native.sqlite.receipt: class=verified; sha256=' + checksum)
    return 0


if __name__ == '__main__':
    import sys
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    try:
        sys.exit(main())
    except Exception:
        print('native.sqlite.receipt: class=refused')
        sys.exit(1)
