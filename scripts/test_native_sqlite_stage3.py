"""Synthetic files and mocked native APIs only. Never execute terminal children."""
import contextlib
import ctypes
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile
import types
import unittest
from unittest.mock import MagicMock, Mock, patch

import native_sqlite_terminal_checks as n
import native_sqlite_fixture_storage as storage
import phase1_security_checks as jobs

SHA = 'a' * 40


class LocalProfile(unittest.TestCase):
    def env(self):
        return dict(EXPECTED_SHA=SHA, DPT_NATIVE_SQLITE_PROFILE='molly-local',
                    NATIVE_RUNNER='molly-local', DPT_NATIVE_SQLITE_HOST='IMOLLY',
                    DPT_NATIVE_SQLITE_LOCAL_RISK='Molly-disposable-synthetic-terminal-only',
                    DPT_SYNTHETIC_SQLITE_TERMINAL_TESTS='1',
                    DPT_SYNTHETIC_REPORT_RISK='disposable-synthetic-only')

    def check(self, env, system='Windows', machine='AMD64', node='IMOLLY', bits=8):
        with patch.object(n.platform, 'node', return_value=node), patch.object(n.struct, 'calcsize', return_value=bits):
            n.prerequisites(env, system, machine, SHA)

    def test_genuine_local_profile_needs_no_github_identity(self):
        self.check(self.env())

    def test_local_host_architecture_pointer_and_risk_refusals(self):
        for kwargs in ({'node': 'Florence'}, {'system': 'Darwin'}, {'machine': 'ARM64'}, {'bits': 4}):
            with self.subTest(kwargs=kwargs), self.assertRaises(n.Refusal):
                self.check(self.env(), **kwargs)
        for key in ('DPT_NATIVE_SQLITE_HOST', 'DPT_NATIVE_SQLITE_LOCAL_RISK', 'DPT_SYNTHETIC_REPORT_RISK'):
            env = self.env()
            del env[key]
            with self.subTest(key=key), self.assertRaises(n.Refusal):
                self.check(env)

    def test_imolly_os_name_and_exact_environment_gate(self):
        for node in ('IMOLLY', 'imolly', 'iMolly'):
            with self.subTest(node=node):
                self.check(self.env(), node=node)
        for node in ('Molly', 'Florence', 'IMOLLY.other', 'IMOLLY ', 'OTHER'):
            with self.subTest(node=node), self.assertRaisesRegex(n.Refusal, 'unapproved_host'):
                self.check(self.env(), node=node)
        for value in ('Molly', 'imolly', 'IMOLLY ', 'OTHER', ''):
            env = self.env()
            env['DPT_NATIVE_SQLITE_HOST'] = value
            with self.subTest(value=value), self.assertRaisesRegex(n.Refusal, 'unapproved_host'):
                self.check(env, node='IMOLLY')

    def test_spoofed_environment_cannot_replace_os_identity(self):
        with patch.object(n, 'execute') as execute, patch.object(n, 'terminal_preflight') as controls:
            with self.assertRaisesRegex(n.Refusal, 'unapproved_host'):
                self.check(self.env(), node='Molly')
            execute.assert_not_called()
            controls.assert_not_called()

    def test_imolly_unsupported_platform_and_bitness_refuse(self):
        for kwargs in ({'machine': 'ARM64'}, {'machine': 'x86'}, {'machine': 'x86_64'},
                       {'system': 'Linux'}, {'system': 'Darwin'}, {'bits': 4}):
            with self.subTest(kwargs=kwargs), self.assertRaisesRegex(n.Refusal, 'unsupported_platform'):
                self.check(self.env(), **kwargs)

    def test_every_github_variable_refuses_local_execution(self):
        for key in ('GITHUB_ACTIONS', 'GITHUB_REF', 'GITHUB_TOKEN'):
            env = self.env()
            env[key] = 'synthetic'
            with self.assertRaisesRegex(n.Refusal, 'local_github_environment_refused'):
                self.check(env)

    def test_local_child_has_no_github_or_sensitive_inheritance(self):
        env = self.env()
        env.update(PATH='unsafe', SMTP_PASSWORD='synthetic', AWS_ACCESS_KEY_ID='synthetic',
                   DYLD_INSERT_LIBRARIES='unsafe', RUSTFLAGS='unsafe', HOME='unsafe', TEMP='unsafe')
        with patch.dict(os.environ, env, clear=True):
            child = n.fixture_env(Path('/private/synthetic'), 'normal', SHA)
        self.assertFalse(any(key.startswith('GITHUB_') for key in child))
        for key in ('PATH', 'SMTP_PASSWORD', 'AWS_ACCESS_KEY_ID', 'DYLD_INSERT_LIBRARIES', 'RUSTFLAGS', 'HOME'):
            self.assertNotIn(key, child)
        self.assertEqual(child['TEMP'], '/private/synthetic')
        self.assertEqual(child['DPT_NATIVE_SQLITE_PROFILE'], 'molly-local')

    def test_github_child_requires_actual_parent_values(self):
        with patch.dict(os.environ, {'NATIVE_RUNNER': 'macos-15'}, clear=True):
            with self.assertRaises(KeyError):
                n.fixture_env(Path('/synthetic'), 'normal', SHA)


class Receipts(unittest.TestCase):
    def test_stall_requires_ready_and_final_cleanup_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ('privacy_ready', 'handles_ready', 'stall_ready'):
                (root / name).touch()
            error = subprocess.TimeoutExpired('synthetic', 3)
            with self.assertRaises(n.Refusal):
                n.assess_timeout(root, 'stall', error, 'Darwin')
            receipt = dict(final_status=-9, reaped=True, descendants_gone=True, handles_closed=True)
            error.cleanup_receipt = receipt
            n.assess_timeout(root, 'stall', error, 'Darwin')
            for key in receipt:
                error.cleanup_receipt = {**receipt, key: None}
                with self.subTest(key=key), self.assertRaises(n.Refusal):
                    n.assess_timeout(root, 'stall', error, 'Darwin')
            error.cleanup_receipt = receipt
            for name in ('completed', 'guards_dropped', 'staging_released', 'unexpected_close_failure', 'terminal_ready'):
                (root / name).touch()
                with self.subTest(name=name), self.assertRaises(n.Refusal):
                    n.assess_timeout(root, 'stall', error, 'Darwin')
                (root / name).unlink()
            (root / 'stall_ready').unlink()
            with self.assertRaises(n.Refusal):
                n.assess_timeout(root, 'stall', error, 'Darwin')

    def test_hashes_capture_changed_removed_and_retained_recovery_material(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            stage = root / 'stage'
            stage.mkdir()
            (stage / 'synthetic.sqlite').write_bytes(b'synthetic before')
            (stage / 'synthetic.sqlite-wal').write_bytes(b'synthetic committed WAL')
            before = n.evidence_hashes(root)
            (stage / 'synthetic.sqlite').write_bytes(b'synthetic after')
            (stage / 'synthetic.sqlite-wal').unlink()
            after = n.evidence_hashes(root)
            self.assertNotEqual(before['stage/synthetic.sqlite'], after['stage/synthetic.sqlite'])
            self.assertNotIn('stage/synthetic.sqlite-wal', after)
            n.write_receipt(root, dict(pre_recovery=before, post_recovery=after))
            self.assertEqual(json.loads((root / 'receipt.json').read_text())['pre_recovery'], before)
            self.assertNotIn('receipt.json', n.evidence_hashes(root))

    def test_wal_needs_committed_main_file_witness(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'stage').mkdir()
            for name, data in [('pending_ready', b'yes'), ('stage/synthetic.sqlite', b'synthetic'), ('stage/synthetic.sqlite-wal', b'x' * 64)]:
                (root / name).write_bytes(data)
            with self.assertRaisesRegex(n.Refusal, 'wal_witness_missing'):
                n.recovery_material(root, 'wal')
            (root / 'wal_committed_only').touch()
            n.recovery_material(root, 'wal')


class ImmutableArtifacts(unittest.TestCase):
    def test_source_inventory_refuses_symlink_and_unmerged_stage(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'source'
            path.write_bytes(b'synthetic source')
            blob = hashlib.sha1(b'blob 16\0synthetic source').hexdigest().encode()
            record = b'100644 ' + blob + b' 0\t' + os.fsencode(path) + b'\0'
            with patch.object(n.subprocess, 'check_output', return_value=record):
                self.assertEqual(n.source_inventory()[path.as_posix()]['sha256'], n.digest(path))
                path.write_bytes(b'changed despite clean Git status')
                with self.assertRaisesRegex(n.Refusal, 'unapproved_source'):
                    n.source_inventory()
                path.write_bytes(b'synthetic source')
            for mode, stage in ((b'120000', b'0'), (b'100644', b'1')):
                bad = mode + b' ' + b'a' * 40 + b' ' + stage + b'\t' + os.fsencode(path) + b'\0'
                with patch.object(n.subprocess, 'check_output', return_value=bad), self.assertRaises(n.Refusal):
                    n.source_inventory()

    def test_executable_manifest_source_and_binary_digest_fail_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'target').mkdir()
            exe = root / 'target' / 'synthetic-executable'
            exe.write_bytes(b'NOT AN EXECUTABLE: synthetic hash fixture')
            manifest = root / 'target' / 'manifest.json'
            inventory = {'synthetic.rs': {'mode': '100644', 'sha256': 'b' * 64}}
            data = dict(sha=SHA, source=inventory, executable=str(exe), executable_sha256=n.digest(exe), target='native_sqlite_terminal_fixtures')
            manifest.write_text(json.dumps(data))
            env = dict(DPT_NATIVE_SQLITE_MANIFEST=str(manifest), DPT_NATIVE_SQLITE_MANIFEST_SHA256=n.digest(manifest))
            with patch.dict(os.environ, env, clear=True), patch.object(n.Path, 'cwd', return_value=root), patch.object(n, 'verify_controller'), patch.object(n, 'source_inventory', return_value=inventory), patch.object(n, 'verify_source_fixture'), patch.object(n.subprocess, 'check_output', return_value=b''):
                self.assertEqual(n.approved_executable(SHA), exe)
                exe.write_bytes(b'tampered synthetic bytes')
                with self.assertRaisesRegex(n.Refusal, 'fixture_inventory_mismatch'):
                    n.approved_executable(SHA)
                exe.write_bytes(b'NOT AN EXECUTABLE: synthetic hash fixture')
                with patch.object(n, 'source_inventory', return_value={}):
                    with self.assertRaisesRegex(n.Refusal, 'unapproved_source'):
                        n.approved_executable(SHA)
                manifest.write_text('{}')
                with self.assertRaisesRegex(n.Refusal, 'fixture_inventory_mismatch'):
                    n.approved_executable(SHA)

    def test_controller_requires_nonzero_approved_clean_commit(self):
        for pin in ('', '0' * 40, 'not a SHA'):
            with patch.dict(os.environ, {'EXPECTED_CONTROLLER_SHA': pin}, clear=True), self.assertRaises(n.Refusal):
                n.verify_controller()
        with patch.dict(os.environ, {'EXPECTED_CONTROLLER_SHA': SHA}, clear=True):
            for result in ([SHA, b'dirty'], ['b' * 40, b'']):
                with patch.object(n.subprocess, 'check_output', side_effect=result), self.assertRaises(n.Refusal):
                    n.verify_controller()


class PrivateStorage(unittest.TestCase):
    def test_symlink_and_reparse_ancestors_refuse(self):
        path = Path('/synthetic/private')
        with patch.object(Path, 'lstat', return_value=types.SimpleNamespace(st_mode=stat.S_IFDIR, st_file_attributes=0x400)), self.assertRaises(storage.StorageRefusal):
            storage.no_links(path)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'real').mkdir()
            (root / 'link').symlink_to(root / 'real', target_is_directory=True)
            with self.assertRaises(storage.StorageRefusal):
                storage.no_links(root / 'link')

    def test_mac_private_tree_mode_identity_hardlinks_and_acl(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            root.chmod(0o700)
            item = root / 'synthetic'
            item.write_bytes(b'synthetic')
            item.chmod(0o600)
            owner = storage.PrivateRoot.__new__(storage.PrivateRoot)
            owner.path, owner.security = root, None
            owner.handle = os.open(root, os.O_RDONLY)
            owner.identity = root.stat().st_dev, root.stat().st_ino
            try:
                with patch.object(storage, 'mac_acl_empty') as acl, patch.object(storage.os, 'geteuid', return_value=root.stat().st_uid):
                    owner.verify()
                    self.assertEqual(acl.call_count, 2)
                    item.chmod(0o644)
                    with self.assertRaisesRegex(storage.StorageRefusal, 'permissions'):
                        owner.verify()
                    item.chmod(0o600)
                    os.link(item, root / 'hardlink')
                    with self.assertRaisesRegex(storage.StorageRefusal, 'object'):
                        owner.verify()
                    (root / 'hardlink').unlink()
                    acl.side_effect = storage.StorageRefusal('fixture_acl_refused')
                    with self.assertRaises(storage.StorageRefusal):
                        owner.verify()
                    acl.side_effect = None
                    owner.identity = (0, 0)
                    with self.assertRaisesRegex(storage.StorageRefusal, 'identity'):
                        owner.verify()
            finally:
                os.close(owner.handle)

    def test_mac_acl_errors_entries_and_free_errors_refuse(self):
        for result, error, release in ((0, 0, 0), (-1, 5, 0), (-1, 22, -1)):
            lib = Mock()
            lib.acl_get_file.return_value = 123
            lib.acl_valid.return_value = 0
            lib.acl_get_entry.return_value = result
            lib.acl_free.return_value = release
            with patch.object(storage.ctypes, 'CDLL', return_value=lib), patch.object(storage.ctypes, 'get_errno', return_value=error), self.assertRaises(storage.StorageRefusal):
                storage.mac_acl_empty(Path('/synthetic'))
        lib.acl_free.return_value = 0
        with patch.object(storage.ctypes, 'CDLL', return_value=lib), patch.object(storage.ctypes, 'get_errno', return_value=22):
            storage.mac_acl_empty(Path('/synthetic'))


class WindowsJobMock(unittest.TestCase):
    """Exercise real ctypes supervisor branches with ABI-correct fake Win32 APIs."""
    def run_job(self, *, isolated=True, timeout=True, close_failure=False):
        from ctypes import wintypes
        w = types.ModuleType('ctypes.wintypes')
        w.__dict__.update(wintypes.__dict__)
        w.DWORD, w.UINT, w.BOOL = ctypes.c_uint32, ctypes.c_uint32, ctypes.c_int32
        kernel = Mock()
        kernel.CreateJobObjectW.return_value = 10
        kernel.GetCurrentProcess.return_value = 11
        kernel.SetInformationJobObject.return_value = 1
        kernel.UpdateProcThreadAttribute.return_value = 1
        kernel.TerminateJobObject.return_value = 1
        kernel.GetStdHandle.return_value = 60
        kernel.CreateFileW.return_value = 70
        kernel.ResumeThread.return_value = 1
        kernel.WaitForSingleObject.side_effect = [258 if timeout else 0, 0]
        kernel.CloseHandle.side_effect = lambda handle: not (close_failure and handle == 21)
        def initialize(pointer, count, flags, length):
            length._obj.value = 256
            return bool(pointer)
        kernel.InitializeProcThreadAttributeList.side_effect = initialize
        counter = iter((100, 101, 102))
        def duplicate(current, source, target, out, access, inherit, flags):
            out._obj.value = next(counter)
            return 1
        kernel.DuplicateHandle.side_effect = duplicate
        def create(*args):
            out = args[-1]._obj
            out.process, out.thread, out.pid = 20, 21, 1000
            return 1
        kernel.CreateProcessW.side_effect = create
        def member(process, job, out):
            out._obj.value = 1
            return 1
        kernel.IsProcessInJob.side_effect = member
        def query(job, info, out, length, returned):
            out._obj.active = 0
            return 1
        kernel.QueryInformationJobObject.side_effect = query
        def exit_code(handle, out):
            out._obj.value = 1 if timeout else 0
            return 1
        kernel.GetExitCodeProcess.side_effect = exit_code
        with patch.dict('sys.modules', {'ctypes.wintypes': w}), patch.object(ctypes, 'wintypes', w), patch.object(jobs.os, 'name', 'nt'), patch.object(ctypes, 'WinDLL', return_value=kernel, create=True), patch.object(ctypes, 'get_last_error', return_value=122, create=True), patch.object(jobs.shutil, 'which', return_value='synthetic.exe'):
            if close_failure:
                with self.assertRaisesRegex(jobs.ContainmentError, 'handle_close_failure'):
                    jobs.run_windows_job(['synthetic.exe'], {}, timeout=3, isolate_stdio=isolated)
            elif timeout:
                with self.assertRaises(subprocess.TimeoutExpired) as raised:
                    jobs.run_windows_job(['synthetic.exe'], {}, timeout=3, isolate_stdio=isolated)
                self.assertEqual(raised.exception.cleanup_receipt, dict(final_status=1, reaped=True, descendants_gone=True, handles_closed=True))
            else:
                self.assertEqual(jobs.run_windows_job(['synthetic.exe'], {}, isolate_stdio=isolated).returncode, 0)
        return kernel

    def test_null_standard_handles_and_timeout_final_cleanup_receipt(self):
        kernel = self.run_job()
        kernel.GetStdHandle.assert_not_called()
        self.assertEqual(kernel.CreateFileW.call_count, 3)
        self.assertTrue(all(call.args[0] == 'NUL' for call in kernel.CreateFileW.call_args_list))
        self.assertEqual(kernel.CloseHandle.call_count, 9)  # 3 sources + thread/process + 3 duplicates + job

    def test_handle_close_failure_overrides_timeout_success(self):
        self.run_job(close_failure=True)

    def test_phase1_default_still_duplicates_parent_handles(self):
        kernel = self.run_job(isolated=False, timeout=False)
        self.assertEqual(kernel.GetStdHandle.call_count, 3)
        kernel.CreateFileW.assert_not_called()

class WindowsDaclMock(unittest.TestCase):
    def test_owner_protected_dacl_and_each_ace_are_required(self):
        from ctypes import wintypes as w
        security = storage.WindowsSecurity.__new__(storage.WindowsSecurity)
        security.w, security.ptr, security.user = w, ctypes.c_void_p, 'S-1-5-21-synthetic'
        acl = ctypes.create_string_buffer(8)
        ctypes.c_ushort.from_buffer(acl, 4).value = 1
        ace = ctypes.create_string_buffer(24)
        ace[0], ace[2] = b'\x00', b'\x18'
        owner = ctypes.create_string_buffer(16)
        def get(path, kind, flags, owner_out, unused, acl_out, unused2, descriptor_out):
            owner_out._obj.value = ctypes.addressof(owner)
            acl_out._obj.value = ctypes.addressof(acl)
            descriptor_out._obj.value = 99
            return 0
        def get_ace(acl_pointer, index, out):
            out._obj.value = ctypes.addressof(ace)
            return 1
        protected = [0x1000]
        def control(descriptor, flags, revision):
            flags._obj.value = protected[0]
            return 1
        security.get, security.ace, security.control = get, get_ace, control
        security.free = Mock(return_value=None)
        sid = [security.user]
        security.sid = lambda pointer: security.user if getattr(pointer, 'value', pointer) == ctypes.addressof(owner) else sid[0]
        security.check(Path('/synthetic'), protected=True)
        sid[0] = 'S-1-1-0'  # Everyone
        with self.assertRaises(storage.StorageRefusal):
            security.check(Path('/synthetic'), protected=True)
        sid[0] = security.user
        protected[0] = 0
        with self.assertRaises(storage.StorageRefusal):
            security.check(Path('/synthetic'), protected=True)
        protected[0] = 0x1000
        ace[0] = b'\x05'  # object ACE/unknown format
        with self.assertRaises(storage.StorageRefusal):
            security.check(Path('/synthetic'), protected=True)
        self.assertEqual(security.free.call_count, 4)


class WorkflowGuards(unittest.TestCase):
    def test_separate_workflow_is_disabled_pinned_and_read_only(self):
        import yaml
        root = Path(__file__).resolve().parents[1]
        path = root / '.github/workflows/sqlite-terminal-native.yml'
        # BaseLoader retains YAML 1.2's 'on' key and scalar spellings.
        workflow = yaml.load(path.read_text(), Loader=yaml.BaseLoader)
        self.assertEqual(set(workflow['on']), {'workflow_dispatch'})
        opt = workflow['on']['workflow_dispatch']['inputs']['controlled_sqlite_terminal_tests']
        self.assertEqual((opt['default'], opt['type']), ('false', 'boolean'))
        self.assertEqual(workflow['permissions'], {'contents': 'read'})
        job = workflow['jobs']['native']
        self.assertIn("github.ref == 'refs/heads/main'", job['if'])
        self.assertEqual(job['strategy']['matrix']['runner'], ['macos-15', 'macos-15-intel'])
        pins = [job['env'][name] for name in ('EXPECTED_SHA', 'EXPECTED_CONTROLLER_SHA')]
        for pin in pins:
            self.assertRegex(pin, r'^[0-9a-f]{40}$')
        self.assertEqual(pins[0] == '0'*40, pins[1] == '0'*40)
        checkouts = {step['with']['path']: step['with']['ref'] for step in job['steps'] if step.get('uses', '').startswith('actions/checkout@')}
        self.assertEqual(checkouts, {'source': pins[0], 'controller': pins[1]})
        actions = [step for step in job['steps'] if 'uses' in step]
        for step in actions:
            self.assertRegex(step['uses'], r'@[0-9a-f]{40}$')
            self.assertNotIn('upload', step['uses'])
            if step['uses'].startswith('actions/checkout@'):
                self.assertEqual(step['with']['persist-credentials'], 'false')
                self.assertRegex(step['with']['ref'], r'^[0-9a-f]{40}$')
        self.assertNotIn('secrets.', path.read_text())
        for step in job['steps']:
            if 'run' in step:
                subprocess.run(['bash', '-n'], input=step['run'], text=True, check=True, capture_output=True)
        old = (root / '.github/workflows/phase1-security-native.yml').read_text()
        self.assertIn('test "$TERMINAL_REQUESTED" = false', old)

    def test_eight_case_contract_and_native_wal_witness_remain_explicit(self):
        self.assertEqual(n.CASES, ('normal', 'retry', 'unwind', 'terminal', 'panic_terminal', 'rollback', 'wal', 'stall'))
        # The reviewed source is admitted from S, not copied into W/main.
        self.assertRegex(n.REVIEWED_FIXTURE_SHA256, r'^[0-9a-f]{64}$')
        self.assertEqual(n.FIXTURE, 'tests/native_sqlite_terminal_fixtures.rs')

class MacTimeoutReceipt(unittest.TestCase):
    def exercise(self, *, cleanup_error=None, close_error=None):
        observer = Mock(exited=False, forked=False)
        observer.observe.return_value = False
        observer.close.side_effect = close_error
        child = Mock(pid=100, returncode=None)
        flags = types.SimpleNamespace(**{name: 1 for name in ('kqueue', 'kevent', 'KQ_FILTER_PROC', 'KQ_EV_ADD', 'KQ_EV_ENABLE', 'KQ_EV_ERROR', 'KQ_NOTE_EXIT', 'KQ_NOTE_FORK')})
        with contextlib.ExitStack() as stack:
            stack.enter_context(patch.object(n.platform, 'system', return_value='Darwin'))
            stack.enter_context(patch.object(n, 'select', flags))
            stack.enter_context(patch.object(n.signal, 'getsignal', return_value=n.signal.SIG_DFL))
            stack.enter_context(patch.object(n.subprocess, 'Popen', return_value=child))
            stack.enter_context(patch.object(n, 'ProcessExit', return_value=observer))
            stack.enter_context(patch.object(n.os, 'getpgid', return_value=100))
            stack.enter_context(patch.object(n.os, 'getsid', return_value=100))
            stack.enter_context(patch.object(n.os, 'getpgrp', return_value=1))
            stack.enter_context(patch.object(n.time, 'monotonic', side_effect=[0, 0, 4]))
            cleanup = stack.enter_context(patch.object(n, 'cleanup_group', return_value=-9, side_effect=cleanup_error))
            if cleanup_error or close_error:
                with self.assertRaises(n.Refusal):
                    n.mac_group(['synthetic'], {}, Mock())
            else:
                with self.assertRaises(subprocess.TimeoutExpired) as raised:
                    n.mac_group(['synthetic'], {}, MagicMock())
                self.assertEqual(raised.exception.cleanup_receipt, dict(final_status=-9, reaped=True, descendants_gone=True, handles_closed=True))
            cleanup.assert_called_once()
            observer.close.assert_called_once()

    def test_timeout_receipt_requires_reap_and_closed_observer(self):
        self.exercise()

    def test_failed_reap_and_failed_observer_close_cannot_escape_as_timeout_success(self):
        self.exercise(cleanup_error=n.Refusal('unsupported_cleanup'))
        self.exercise(close_error=n.Refusal('unsupported_cleanup'))


class TerminalOrchestration(unittest.TestCase):
    def test_windows_controls_use_private_sanitized_null_stdio_profile(self):
        import test_phase1_security_checks as guards
        owner = Mock(path=Path('/synthetic-private-control'))
        original = Mock(return_value='synthetic result')
        def controls():
            self.assertNotIn('GITHUB_TOKEN', os.environ)
            self.assertEqual(tempfile.tempdir, str(owner.path))
            guards.checks.run_windows_job(['synthetic.exe'], dict(SystemRoot='system', PATH='unsafe', DPT_TEST_CHILD='synthetic', SMTP_PASSWORD='synthetic'), timeout=10)
        with patch.object(n, 'PrivateRoot', return_value=owner), patch.object(guards.checks, 'run_windows_job', original), patch.object(n, 'windows_preflight', side_effect=controls), patch.dict(os.environ, {'SystemRoot': 'system', 'GITHUB_TOKEN': 'synthetic'}, clear=True):
            n.terminal_preflight('Windows')
        args, kwargs = original.call_args
        self.assertEqual(args[1], dict(SystemRoot='system', DPT_TEST_CHILD='synthetic', TEMP=str(owner.path), TMP=str(owner.path)))
        self.assertEqual(kwargs, dict(timeout=10, isolate_stdio=True))
        owner.verify.assert_called_once()
        owner.close.assert_called_once()

class ManifestProducer(unittest.TestCase):
    def test_manifest_extracts_only_one_no_run_artifact_and_hashes_without_execution(self):
        import native_sqlite_terminal_manifest as maker
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / 'target'
            target.mkdir()
            executable = target / 'synthetic-test'
            executable.write_bytes(b'NOT EXECUTABLE')
            log = target / 'build.jsonl'
            artifact = dict(reason='compiler-artifact', target=dict(name='native_sqlite_terminal_fixtures', kind=['test']), profile=dict(test=True), executable=str(executable))
            for records, valid in (([artifact], True), ([], False), ([artifact, artifact], False), ([{**artifact, 'profile': {'test': False}}], False), ([{**artifact, 'target': {'name': 'native_sqlite_terminal_fixtures', 'kind': ['bin']}}], False)):
                log.write_text('\n'.join(json.dumps(r) for r in records))
                with patch.dict(os.environ, {'EXPECTED_SHA': SHA}, clear=True), patch.object(maker.Path, 'cwd', return_value=root), patch.object(maker, 'verify_controller'), patch.object(maker.subprocess, 'check_output', side_effect=[SHA, b'']), patch.object(maker, 'source_inventory', return_value={'synthetic': 'inventory'}), patch.object(maker, 'verify_source_fixture'), patch.object(maker.subprocess, 'run') as run:
                    if valid:
                        result = maker.build_manifest(log)
                        self.assertEqual(result['executable_sha256'], hashlib.sha256(b'NOT EXECUTABLE').hexdigest())
                        self.assertEqual(result['source'], {'synthetic': 'inventory'})
                    else:
                        with self.assertRaises(n.Refusal):
                            maker.build_manifest(log)
                    run.assert_not_called()

    def test_manifest_refuses_unapproved_or_dirty_source_before_reading_build(self):
        import native_sqlite_terminal_manifest as maker
        with patch.dict(os.environ, {'EXPECTED_SHA': SHA}, clear=True), patch.object(maker, 'verify_controller'):
            for outputs in (['b' * 40], [SHA, b'dirty']):
                with patch.object(maker.subprocess, 'check_output', side_effect=outputs), self.assertRaises(n.Refusal):
                    maker.build_manifest(Path('/never-read'))


if __name__ == '__main__':
    unittest.main()
