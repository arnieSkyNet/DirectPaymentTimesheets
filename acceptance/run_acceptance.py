#!/usr/bin/env python3
"""Prepared native acceptance runner. Default mode only verifies staged files.

No terminal targets, watchdogs, custom VFS, SMTP or Keychain cases are executed.
Each exact test gets a fresh process; no process-kill/retry/cleanup machinery.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'candidate'
HERE = ROOT / 'acceptance'
TARGETS = {'x86_64-apple-darwin': ('x86_64', 'macos-15-intel'),
           'aarch64-apple-darwin': ('arm64', 'macos-15')}
BIN = 'direct_payment_timesheets'


def require(condition, reason):
    if not condition:
        raise RuntimeError(reason)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_json(path):
    return json.loads(path.read_text())


def present(path):
    # Broken links must not evade refusal checks.
    return path.exists() or path.is_symlink()


def record_matches(path, record):
    return (path.is_file() and not path.is_symlink()
            and digest(path) == record['sha256']
            and path.stat().st_size == record['size']
            and bool(path.stat().st_mode & 0o111) == record['executable'])


def publication_records():
    manifest = read_json(HERE / 'publication-manifest.json')
    require(set(manifest) == {'format', 'files'} and manifest['format'] == 1, 'publication_manifest_schema')
    records = manifest['files']
    require(len(records) == 123, 'publication_manifest_count')
    for name, record in records.items():
        path = PurePosixPath(name)
        require(not path.is_absolute() and str(path) == name and '..' not in path.parts
                and '.git' not in path.parts, 'publication_manifest_path')
        require(set(record) == {'sha256', 'size', 'executable'}, 'publication_record_schema')
    own_name = 'acceptance/publication-manifest.json'
    require(own_name not in records, 'publication_manifest_self_reference')
    return records


def verify_published_tree():
    records = publication_records()
    expected = set(records) | {'acceptance/publication-manifest.json'}
    directories = {str(parent) for name in expected for parent in PurePosixPath(name).parents if str(parent) != '.'}
    found = set()
    # Walk explicitly so only the root .git metadata directory is exempt.
    pending = [ROOT]
    while pending:
        parent = pending.pop()
        for path in parent.iterdir():
            name = path.relative_to(ROOT).as_posix()
            require(not path.is_symlink(), 'published_symlink_refused')
            if name == '.git':
                require(path.is_dir(), 'independent_git_directory_required')
                continue
            if path.is_dir():
                require(name in directories, 'unexpected_published_directory:' + name)
                pending.append(path)
            else:
                require(path.is_file() and name in expected, 'unexpected_published_file:' + name)
                found.add(name)
    require(found == expected, 'published_tree_inventory')
    for name, record in records.items():
        require(record_matches(ROOT / name, record), 'published_tree_hash_or_mode:' + name)
    return records


def git_env():
    # Read-only plumbing; no inherited GIT_DIR, worktree, index or config overrides.
    env = {k: os.environ[k] for k in ('PATH', 'HOME', 'USER', 'LANG', 'LC_ALL') if k in os.environ}
    env.update(GIT_OPTIONAL_LOCKS='0', GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL=os.devnull,
               GIT_NO_REPLACE_OBJECTS='1')
    return env


def git_read(root, args):
    return subprocess.run(['git', '-C', str(root)] + args, env=git_env(),
                          capture_output=True, check=True).stdout


def independent_git(root):
    root = root.resolve(strict=True)
    own = root / '.git'
    require(own.is_dir() and not own.is_symlink(), 'independent_git_directory_required')
    require(not present(own / 'commondir') and not present(own / 'objects/info/alternates'), 'shared_git_storage_refused')
    for path in own.rglob('*'):
        require(not path.is_symlink(), 'shared_git_metadata_symlink')
    top = Path(git_read(root, ['rev-parse', '--show-toplevel']).decode().strip()).resolve(strict=True)
    git_dir = Path(git_read(root, ['rev-parse', '--path-format=absolute', '--git-dir']).decode().strip()).resolve(strict=True)
    common = Path(git_read(root, ['rev-parse', '--path-format=absolute', '--git-common-dir']).decode().strip()).resolve(strict=True)
    require(top == root and git_dir == own and common == own, 'independent_git_identity')
    return own


def verify_checkout(expected_commit):
    require(re.fullmatch(r'[0-9a-f]{40}', expected_commit or '') is not None
            and expected_commit != '0' * 40, 'immutable_commit_required')
    records = verify_published_tree()
    independent_git(ROOT)
    head = git_read(ROOT, ['rev-parse', 'HEAD']).decode().strip()
    require(head == expected_commit, 'checkout_commit_mismatch')
    actual = {}
    for line in git_read(ROOT, ['ls-tree', '-rz', '--full-tree', 'HEAD']).split(b'\0'):
        if not line:
            continue
        info, name = line.split(b'\t', 1)
        mode, kind, oid = info.decode().split()
        require(kind == 'blob' and mode in ('100644', '100755'), 'git_tree_special_entry')
        require(name.decode() not in actual, 'duplicate_committed_path')
        actual[name.decode()] = (mode, oid)
    own = 'acceptance/publication-manifest.json'
    require(set(actual) == set(records) | {own}, 'committed_tree_inventory')
    index = {}
    for line in git_read(ROOT, ['ls-files', '--stage', '-z']).split(b'\0'):
        if not line:
            continue
        info, name = line.split(b'\t', 1)
        mode, oid, stage = info.decode().split()
        require(stage == '0', 'unmerged_publication_index')
        require(name.decode() not in index, 'duplicate_index_path')
        index[name.decode()] = (mode, oid)
    require(index == actual, 'publication_index_differs_from_HEAD')
    for name, (mode, oid) in actual.items():
        data = git_read(ROOT, ['cat-file', 'blob', oid])
        require(hashlib.sha256(data).hexdigest() == digest(ROOT / name), 'committed_blob_differs:' + name)
        require((mode == '100755') == bool((ROOT / name).stat().st_mode & 0o111), 'committed_mode_differs:' + name)
    return {'commit': head, 'git_dir': str(ROOT / '.git'), 'tree_files_verified': len(actual),
            'lockfile_sha256': digest(SOURCE / 'Cargo.lock')}


def reject_cargo_overrides(env):
    # Cargo searches every parent, not just the checkout root. Reject even a
    # non-workspace parent Cargo.toml and all implicit toolchain/config files.
    for directory in (SOURCE,) + tuple(SOURCE.parents):
        if directory != SOURCE:
            require(not present(directory / 'Cargo.toml') and not present(directory / 'Cargo.lock'), 'parent_cargo_manifest_or_lock:' + str(directory))
        require(not present(directory / 'rust-toolchain') and not present(directory / 'rust-toolchain.toml'), 'ancestor_toolchain_override')
        cargo_dir = directory / '.cargo'
        require(not cargo_dir.is_symlink(), 'cargo_config_directory_symlink')
        for name in ('config', 'config.toml'):
            require(not present(cargo_dir / name), 'ancestor_cargo_configuration:' + str(cargo_dir / name))
    homes = {Path(env.get('CARGO_HOME', str(Path.home() / '.cargo'))), Path(env.get('HOME', str(Path.home()))) / '.cargo'}
    for home in homes:
        require(not home.is_symlink(), 'cargo_home_symlink')
        for name in ('config', 'config.toml'):
            require(not present(home / name), 'cargo_home_configuration')
    allowed = {'CARGO_HOME', 'CARGO_TARGET_DIR', 'CARGO_INCREMENTAL', 'CARGO_BUILD_JOBS',
               'CARGO_PROFILE_TEST_DEBUG', 'RUSTUP_HOME', 'RUSTUP_TOOLCHAIN'}
    for name in env:
        require(not (name.startswith(('CARGO_', 'RUST')) and name not in allowed), 'cargo_or_rust_environment_override:' + name)
    require(env.get('RUSTUP_TOOLCHAIN') == '1.98.1', 'toolchain_override')


def verify_cargo_identity(run, env):
    reject_cargo_overrides(env)
    lock = SOURCE / 'Cargo.lock'
    expected_lock = read_json(HERE / 'source-manifest.json')['Cargo.lock']['sha256']
    require(digest(lock) == expected_lock and not lock.is_symlink(), 'sealed_lockfile_identity')
    value = run(['cargo', 'locate-project', '--workspace', '--manifest-path', str(SOURCE / 'Cargo.toml'),
                 '--message-format', 'plain'], 'workspace-identity')
    effective = Path(value.strip()).resolve(strict=True)
    require(effective == SOURCE / 'Cargo.toml', 'effective_workspace_root_mismatch')
    require(digest(effective.parent / 'Cargo.lock') == expected_lock, 'effective_workspace_lockfile_mismatch')
    return {'workspace_root': str(effective.parent), 'manifest': str(effective), 'lockfile_sha256': expected_lock}


def verify():
    verify_published_tree()
    manifest = read_json(HERE / 'source-manifest.json')
    prior = read_json(HERE / 'prior-114-build-inputs.json')
    require(len(manifest) == 117 and len(prior) == 114, 'source_inventory_count')
    found = set()
    for path in SOURCE.rglob('*'):
        require(not path.is_symlink(), 'source_symlink_refused')
        if path.is_file():
            found.add(path.relative_to(SOURCE).as_posix())
    require(found == set(manifest), 'source_inventory_drift')
    for name, record in manifest.items():
        rel = PurePosixPath(name)
        require(not rel.is_absolute() and '..' not in rel.parts, 'manifest_path')
        path = SOURCE / name
        require(digest(path) == record['sha256'] and path.stat().st_size == record['size'], 'source_hash_or_size')
        require(bool(path.stat().st_mode & 0o111) == record['executable'], 'source_executable_mode')
        if name in prior:
            require(prior[name] == record['sha256'], 'prior_inventory_drift')
    require(set(prior).issubset(manifest), 'prior_inventory_missing')
    require(not (SOURCE / '.cargo').exists() and not (ROOT / '.cargo').exists(), 'cargo_config_refused')
    tests = read_json(HERE / 'allowlist.json')
    names = [t['name'] for t in tests]
    require(len(names) == 43 and len(set(names)) == 43, 'allowlist_inventory')
    require((HERE / 'exact-tests.txt').read_text().splitlines() == names, 'allowlist_text_drift')
    allowed_prefixes = ('database_connection::tests::', 'database_failure::tests::',
                        'gui::shutdown_tests::', 'backup_service::tests::',
                        'database_recovery::tests::', 'database_recovery::stage5_tests::',
                        'private_storage::macos_acl::tests::', 'private_storage::macos_acl_tests::',
                        'private_storage::tests::')
    for test in tests:
        name = test['name']
        require(name.startswith(allowed_prefixes), 'test_namespace_refused')
        require(not any(s in name for s in ('terminal', 'watchdog', 'killed_', 'process_exit', 'corrupt_', 'credential_', 'smtp_')), 'excluded_test')
        text = (SOURCE / test['source']).read_text()
        require(re.search(r'#\[test\]\s+fn ' + re.escape(test['function']) + r'\s*\(', text) is not None, 'test_source_missing')
    return tests


def hosted(target):
    require(target in TARGETS, 'unsupported_target')
    arch, runner = TARGETS[target]
    require(platform.system() == 'Darwin' and platform.machine() == arch, 'native_architecture_required')
    require(os.environ.get('GITHUB_ACTIONS') == 'true' and os.environ.get('RUNNER_ENVIRONMENT') == 'github-hosted', 'hosted_runner_required')
    require(os.environ.get('RUNNER_OS') == 'macOS', 'runner_os')
    require(os.environ.get('DPT_EXPECTED_RUNNER') == runner, 'runner_matrix')
    require(os.environ.get('RUNNER_ARCH') == ('ARM64' if arch == 'arm64' else 'X64'), 'runner_architecture')
    require(os.environ.get('DPT_ACCEPTANCE_APPROVED') == 'true', 'explicit_execution_approval_required')
    return arch


def query(args):
    result = subprocess.run(args, capture_output=True, text=True, check=True)
    return result.stdout.strip()


def sdk_preflight(target):
    arch = hosted(target)
    require(query(['/usr/bin/uname', '-m']) == arch, 'uname_architecture')
    sdk = Path(query(['/usr/bin/xcrun', '--sdk', 'macosx', '--show-sdk-path'])).resolve(strict=True)
    clang = Path(query(['/usr/bin/xcrun', '--sdk', 'macosx', '--find', 'clang'])).resolve(strict=True)
    require(sdk.is_dir() and clang.is_file(), 'apple_sdk_tools')
    require((sdk / 'usr/include/sys/acl.h').is_file(), 'apple_acl_header')
    require((sdk / 'System/Library/Frameworks/Security.framework').is_dir(), 'apple_security_framework')
    query(['/usr/bin/clang', '--version'])
    return {'architecture': arch, 'target': target, 'sdk': str(sdk),
            'sdk_version': query(['/usr/bin/xcrun', '--sdk', 'macosx', '--show-sdk-version']),
            'xcode_version': query(['/usr/bin/xcodebuild', '-version'])}


def child_env(work, sdk):
    # Keep HOME unchanged. No GitHub token, secrets, application/terminal gates,
    # Rust wrappers/flags, credentials or arbitrary inherited SDK overrides.
    keep = ('PATH', 'HOME', 'USER', 'LOGNAME', 'LANG', 'LC_ALL', 'CARGO_HOME', 'RUSTUP_HOME')
    env = {k: os.environ[k] for k in keep if k in os.environ}
    env.update(TMPDIR=str(work / 'fixtures'), CARGO_TARGET_DIR=str(work / 'target'),
               CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2', CARGO_PROFILE_TEST_DEBUG='0',
               RUSTUP_TOOLCHAIN='1.98.1', SDKROOT=sdk, GIT_TERMINAL_PROMPT='0',
               DIRECTPAYMENTTIMESHEETS_HOME=str(work / 'unused-application-root'))
    reject_cargo_overrides(env)
    return env


def execute(target, tests):
    checkout = verify_checkout(os.environ.get('DPT_CANDIDATE_SHA'))
    details = sdk_preflight(target)
    temp = Path(os.environ['RUNNER_TEMP']).resolve(strict=True)
    work = Path(tempfile.mkdtemp(prefix='dpt-sqlite-acceptance-', dir=temp)).resolve()
    (work / 'fixtures').mkdir(mode=0o700)
    (work / 'logs').mkdir(mode=0o700)
    env = child_env(work, details['sdk'])
    sentinel = Path(env['DIRECTPAYMENTTIMESHEETS_HOME'])
    commands = []

    def run(args, label):
        require(not sentinel.exists(), 'unexpected_application_root')
        reject_cargo_overrides(env)
        result = subprocess.run(args, cwd=SOURCE, env=env, capture_output=True, text=True)
        (work / 'logs' / (label + '.stdout')).write_text(result.stdout)
        (work / 'logs' / (label + '.stderr')).write_text(result.stderr)
        commands.append({'label': label, 'args': args, 'exit_code': result.returncode})
        require(not sentinel.exists(), 'unexpected_application_root')
        require(result.returncode == 0, 'command_failed:' + label)
        return result.stdout

    version = run(['rustc', '-vV'], 'rustc')
    require('release: 1.98.1\n' in version, 'rust_version')
    require('host: ' + target + '\n' in version, 'rust_native_host')
    workspace = verify_cargo_identity(run, env)
    run(['cargo', 'build', '--locked', '--manifest-path', str(SOURCE / 'Cargo.toml'), '--target', target, '--bin', BIN], 'application-build')
    verify_cargo_identity(run, env)
    build = run(['cargo', 'test', '--locked', '--manifest-path', str(SOURCE / 'Cargo.toml'), '--target', target, '--bin', BIN,
                 '--no-run', '--message-format=json'], 'unit-test-build')
    verify_cargo_identity(run, env)
    artifacts = [json.loads(line) for line in build.splitlines() if line.startswith('{')]
    candidates = [r for r in artifacts if r.get('reason') == 'compiler-artifact'
                  and r.get('target', {}).get('name') == BIN
                  and r.get('target', {}).get('kind') == ['bin']
                  and r.get('profile', {}).get('test') is True and r.get('executable')]
    require(len(candidates) == 1, 'test_executable_inventory')
    executable = Path(candidates[0]['executable']).resolve(strict=True)
    application = work / 'target' / target / 'debug' / BIN
    require(executable.is_relative_to(work / 'target'), 'test_executable_path')
    for binary in (application, executable):
        require(query(['/usr/bin/lipo', '-archs', str(binary)]) == details['architecture'], 'binary_architecture')
    names = [t['name'] for t in tests]
    listing = run([str(executable), '--list', '--format', 'terse'], 'test-inventory')
    available = [line[:-6] for line in listing.splitlines() if line.endswith(': test')]
    ignored = run([str(executable), '--list', '--ignored', '--format', 'terse'], 'ignored-inventory')
    ignored_names = {line[:-6] for line in ignored.splitlines() if line.endswith(': test')}
    require(all(available.count(name) == 1 for name in names), 'allowlisted_test_missing_or_duplicate')
    require(not set(names) & ignored_names, 'allowlisted_test_ignored')
    passes = []
    for index, name in enumerate(names, 1):
        output = run([str(executable), name, '--exact', '--test-threads=1'], f'test-{index:02}')
        summaries = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', output)
        completed = re.findall(r'^test (.+) \.\.\. ok$', output, re.M)
        require(summaries == [('1', '0', '0')] and completed == [name], 'test_result_unconfirmed:' + name)
        passes.append(name)
        print('PASS ' + name, flush=True)
    verify()
    verify_checkout(checkout['commit'])
    verify_cargo_identity(run, env)
    receipt = dict(details, workspace=workspace, checkout=checkout, tests_passed=passes, test_count=len(passes),
                   source_manifest_sha256=digest(HERE / 'source-manifest.json'),
                   allowlist_sha256=digest(HERE / 'allowlist.json'),
                   runner_sha256=digest(Path(__file__)),
                   application_sha256=digest(application), test_executable_sha256=digest(executable),
                   source_commit=checkout['commit'],
                   controller_commit=os.environ.get('DPT_CONTROLLER_SHA'),
                   commands=commands, application_root_absent=not sentinel.exists(),
                   terminal_executed=False, native_smtp_keychain_executed=False,
                   limitation='Headless tests only; no native-close panic, crash durability, GUI or universal pathname confinement claim.')
    (work / 'RESULTS.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt, sort_keys=True))
    summary = os.environ.get('GITHUB_STEP_SUMMARY')
    if summary:
        with open(summary, 'a') as stream:
            stream.write('## SQLite native acceptance\n\n```json\n' + json.dumps(receipt, indent=2) + '\n```\n')
    # No recursive cleanup or process supervision; private synthetic files/logs
    # remain only on this disposable hosted runner until GitHub disposes it.


def main():
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--verify-only', action='store_true')
    mode.add_argument('--preflight', action='store_true')
    mode.add_argument('--execute', action='store_true')
    parser.add_argument('--target', choices=TARGETS)
    parser.add_argument('--expected-commit')
    args = parser.parse_args()
    tests = verify()
    if args.expected_commit:
        verify_checkout(args.expected_commit)
    if args.preflight or args.execute:
        require(args.expected_commit == os.environ.get('DPT_CANDIDATE_SHA') and args.expected_commit is not None, 'approved_commit_argument_required')
    if args.preflight:
        print(json.dumps(sdk_preflight(args.target), sort_keys=True))
    elif args.execute:
        execute(args.target, tests)
    else:
        print(json.dumps({'status': 'PREPARATION VERIFIED ONLY', 'snapshot_files': 117,
                          'inventoried_files': 114, 'allowlisted_tests': len(tests),
                          'compiled': False, 'acceptance_executed': False}, sort_keys=True))


if __name__ == '__main__':
    try:
        main()
    except (RuntimeError, subprocess.CalledProcessError, OSError) as error:
        # No child output, environment or fixture data is published on failure.
        print('ACCEPTANCE REFUSED: ' + type(error).__name__ + ': ' +
              (str(error) if isinstance(error, RuntimeError) else 'native prerequisite or tool failed'), file=sys.stderr)
        sys.exit(1)
