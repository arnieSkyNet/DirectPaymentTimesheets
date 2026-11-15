"""Manual hosted-macOS current-source acceptance. Never print captured secrets/errors."""
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import uuid

TESTS = (
    ('smtp_credentials::current_keychain_acceptance::native_password_lifecycle', False),
    ('microsoft_oauth::tests::native_synthetic_persistence_and_cleanup', True),
    ('microsoft_oauth::current_keychain_acceptance::native_token_value_rotation', False),
)
CANARIES = (b'MACOS_SYNTHETIC_PASSWORD_', b'MACOS_SYNTHETIC_ACCESS_',
            b'MACOS_SYNTHETIC_REFRESH_', b'ACCESS_SECRET', b'REFRESH_SECRET',
            b'SYNTHETIC_ONLY', b'SYNTHETIC_NATIVE_SECRET')


class Stop(Exception):
    def __init__(self, result, category):
        self.result, self.category = result, category


def capture(args, cwd=None, env=None, timeout=30):
    # Never forward command lines, arbitrary exceptions or captured diagnostics.
    with tempfile.TemporaryFile() as output:
        process = subprocess.Popen(args, cwd=cwd, env=env, stdout=output,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        try:
            code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=10)
            raise Stop('FAIL', 'timeout_cleanup_unknown') from None
        output.seek(0)
        return code, output.read()


def safe_run(args, cwd, env, timeout):
    code, data = capture(args, cwd, env, timeout)
    if any(c in data for c in CANARIES):
        raise Stop('FAIL', 'captured_output_secret_detected_diagnostics_withheld')
    if code:
        raise Stop('FAIL', 'operation_failed_diagnostics_withheld')
    return data


def require_host():
    if (sys.platform != 'darwin' or os.environ.get('GITHUB_ACTIONS') != 'true'
            or os.environ.get('RUNNER_ENVIRONMENT') != 'github-hosted'
            or platform.machine() != os.environ.get('DPT_EXPECTED_ARCH')):
        raise Stop('BLOCKED', 'requires_expected_hosted_macos_architecture')


def preflight(env):
    """Only a fresh UUID item; no default/search-list/unlock/policy changes."""
    account = 'probe-' + uuid.uuid4().hex
    service = 'DirectPaymentTimesheets.Tests.current-keychain.preflight'
    secret = 'MACOS_SYNTHETIC_PREFLIGHT_ONLY'
    key = ['/usr/bin/security']
    selector = ['-a', account, '-s', service]
    failure = None
    try:
        code, data = capture(key + ['add-generic-password'] + selector + ['-w', secret], env=env)
        if code:
            # Known native headless/authorization/environment restrictions only.
            if any(str(v).encode() in data for v in (-25308, -25293, -25291)) or any(
                    t in data for t in (b'User interaction is not allowed',
                                       b'authorization was denied',
                                       b'A default keychain could not be found')):
                raise Stop('BLOCKED', 'headless_keychain_access_unavailable')
            raise Stop('FAIL', 'keychain_preflight_create_failed')
        code, value = capture(key + ['find-generic-password'] + selector + ['-w'], env=env)
        if code or value.strip() != secret.encode():
            raise Stop('FAIL', 'keychain_preflight_readback_failed')
    except Stop as error:
        failure = (Stop('BLOCKED', 'headless_preflight_timeout_cleanup_unknown')
                   if error.category == 'timeout_cleanup_unknown' else error)
    finally:
        # Also attempt cleanup after a partially successful create/timeout.
        code, _ = capture(key + ['delete-generic-password'] + selector, env=env)
        absent, _ = capture(key + ['find-generic-password'] + selector, env=env)
        if absent != 44 or code not in (0, 44):
            raise Stop('FAIL', 'preflight_cleanup_unconfirmed')
    if failure:
        raise failure


def source_snapshot(root, destination, env):
    expected = os.environ.get('DPT_EXPECTED_SHA', '')
    if not re.fullmatch(r'[0-9a-f]{40}', expected):
        raise Stop('FAIL', 'invalid_expected_commit')
    actual = safe_run(['git', 'rev-parse', 'HEAD'], root, env, 10).decode().strip()
    if actual != expected or safe_run(['git', 'status', '--porcelain', '--untracked-files=all'], root, env, 10).strip():
        raise Stop('FAIL', 'checkout_not_exact_clean_dispatched_commit')
    names = safe_run(['git', 'ls-files', '-z'], root, env, 10).decode().strip('\0').split('\0')
    manifest = {}
    for name in names:
        relative = Path(name)
        if relative.is_absolute() or '..' in relative.parts:
            raise Stop('FAIL', 'unsafe_tracked_path')
        path = root / relative
        if path.is_symlink() or not path.is_file():
            raise Stop('FAIL', 'source_symlink_or_special_file')
        data = path.read_bytes()
        manifest[name] = hashlib.sha256(data).hexdigest()
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, target)
        if target.read_bytes() != data:
            raise Stop('FAIL', 'snapshot_copy_mismatch')
    digest = hashlib.sha256(json.dumps(manifest, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
    return actual, digest, manifest


def apply_overlays(snapshot):
    hashes = {}
    for module, fixture in (('smtp_credentials', 'smtp'), ('microsoft_oauth', 'oauth')):
        target = snapshot / 'src' / (module + '.rs')
        overlay = (snapshot / 'scripts' / ('macos_current_' + fixture + '_fixture.rs')).read_bytes()
        original = target.read_bytes()
        if b'mod current_keychain_acceptance' in original:
            raise Stop('FAIL', 'overlay_already_present')
        target.write_bytes(original + b'\n' + overlay)
        hashes[fixture] = hashlib.sha256(overlay).hexdigest()
    return hashes


def main():
    require_host()
    root = Path(__file__).resolve().parents[1]
    env = {k: v for k, v in os.environ.items() if k in (
        'HOME', 'PATH', 'TMPDIR', 'SDKROOT', 'DEVELOPER_DIR', 'RUSTUP_HOME', 'CARGO_HOME')}
    env.update(RUSTUP_TOOLCHAIN='1.98.1', CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2',
               CARGO_PROFILE_TEST_DEBUG='0', DPT_MACOS_CURRENT_ACCEPTANCE='1')
    # Hosted ephemeral user's normal Keychain, test namespaces only. No policy repair.
    with tempfile.TemporaryDirectory(prefix='dpt-current-keychain-', dir=os.environ['RUNNER_TEMP']) as work:
        work = Path(work).resolve()
        source = work / 'source'
        source.mkdir(mode=0o700)
        sha, digest, manifest = source_snapshot(root, source, env)
        preflight(env)
        overlays = apply_overlays(source)
        private_tmp = work / 'profiles'
        private_tmp.mkdir(mode=0o700)
        env['TMPDIR'] = str(private_tmp)
        env['CARGO_TARGET_DIR'] = str(work / 'target')
        build = safe_run(['cargo', 'test', '--locked', '--bin', 'direct_payment_timesheets',
                          '--no-run', '--message-format=json'], source, env, 2400)
        artifacts = [r for line in build.splitlines() if line.startswith(b'{')
                     for r in [json.loads(line)] if r.get('reason') == 'compiler-artifact'
                     and r.get('target', {}).get('name') == 'direct_payment_timesheets'
                     and r.get('profile', {}).get('test') and r.get('executable')]
        if len(artifacts) != 1:
            raise Stop('FAIL', 'test_binary_inventory_mismatch')
        binary = Path(artifacts[0]['executable']).resolve()
        if not binary.is_relative_to(work / 'target') or not binary.is_file():
            raise Stop('FAIL', 'unexpected_test_binary')
        listed = safe_run([str(binary), '--list'], source, env, 30)
        for test, ignored in TESTS:
            if listed.splitlines().count((test + ': test').encode()) != 1:
                raise Stop('FAIL', 'exact_required_test_missing')
            args = [str(binary), '--exact', test, '--test-threads=1'] + (['--ignored'] if ignored else [])
            result = safe_run(args, source, env, 180)
            if b'1 passed; 0 failed; 0 ignored;' not in result:
                raise Stop('FAIL', 'required_native_test_not_completed')
            print('PASS: ' + test)
        for name, original_hash in manifest.items():
            if hashlib.sha256((root / name).read_bytes()).hexdigest() != original_hash:
                raise Stop('FAIL', 'original_checkout_changed')
        print(json.dumps(dict(result='PASS', source_sha=sha, source_manifest_sha256=digest,
                              binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                              overlay_sha256=overlays, architecture=platform.machine(),
                              native_parent_tests=len(TESTS), provider_network=False)))
        print('NOT TESTED: real locked/denied Keychain, GUI prompts, signing/update ACLs, reboot persistence, live OAuth or SMTP.')


if __name__ == '__main__':
    try:
        main()
    except Stop as error:
        print(error.result + ': ' + error.category)
        sys.exit(2 if error.result == 'BLOCKED' else 1)
    except Exception:
        print('FAIL: unexpected_failure_diagnostics_withheld')
        sys.exit(1)
