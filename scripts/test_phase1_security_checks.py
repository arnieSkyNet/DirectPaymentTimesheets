"""Test the runner's failure accounting without running cargo or touching data."""
import contextlib
import io
import os
import subprocess
import unittest
import tempfile
import sys
from pathlib import Path
from unittest.mock import Mock, patch

import phase1_security_checks as checks

LISTING = "phase1_windows_actual_new_object_owners_match_current_token: test\n"


class RunnerTests(unittest.TestCase):
    def invoke(self, run):
        with patch.object(checks.platform, "system", return_value="Windows"), \
                patch.object(checks.os, "chdir"), \
                patch.dict(os.environ, {"EXPECTED_SHA": "a" * 40}), \
                patch.object(checks.subprocess, "check_output", return_value="a" * 40), \
                patch.object(checks.subprocess, "run", side_effect=run), \
                patch.object(checks, "run_windows_job", side_effect=lambda args, env: run(args, env=env, contained=True)), \
                contextlib.redirect_stdout(io.StringIO()), \
                contextlib.redirect_stderr(io.StringIO()):
            return checks.main()

    def test_zero_matching_tests_cannot_pass(self):
        def run(args, **kwargs):
            return subprocess.CompletedProcess(args, 0, "", "")
        self.assertEqual(self.invoke(run), 1)

    def test_failure_does_not_hide_later_groups(self):
        executed = []
        def run(args, **kwargs):
            if "--list" in args:
                return subprocess.CompletedProcess(args, 0, LISTING, "")
            if "test" in args:
                executed.append(args[4])
                return subprocess.CompletedProcess(args, int(args[4] == "phase1"))
            return subprocess.CompletedProcess(args, 0)
        self.assertEqual(self.invoke(run), 1)
        self.assertEqual(executed, list(checks.GROUPS))

    def test_success_requires_every_discovered_group_to_pass(self):
        def run(args, **kwargs):
            return subprocess.CompletedProcess(args, 0, LISTING, "")
        self.assertEqual(self.invoke(run), 0)

    def test_missing_native_marker_cannot_pass(self):
        def run(args, **kwargs):
            return subprocess.CompletedProcess(args, 0, "generic_fixture: test\n", "")
        self.assertEqual(self.invoke(run), 1)

    def test_diagnostics_are_exact_opt_in_and_phase1_only(self):
        for setting in ["0", "true", "1"]:
            executed = []
            def run(args, **kwargs):
                if "--list" in args:
                    return subprocess.CompletedProcess(args, 0, LISTING, "")
                if "test" in args:
                    executed.append((args, kwargs))
                return subprocess.CompletedProcess(args, 0)
            with patch.dict(os.environ, {
                "DPT_WINDOWS_SYNTHETIC_ROOT_DIAGNOSTICS": setting,
                "DPT_WINDOWS_ROOT_DIAGNOSTICS": "1",
            }):
                self.assertEqual(self.invoke(run), 0)
            for args, kwargs in executed:
                enabled = setting == "1" and args[4] == "phase1"
                self.assertEqual(kwargs["env"].get("DPT_WINDOWS_ROOT_DIAGNOSTICS"),
                                 "1" if enabled else None)
                self.assertEqual("--nocapture" in args, enabled)
                self.assertEqual(kwargs.get("contained", False), enabled)

    def test_timeout_does_not_hide_later_groups(self):
        executed = []
        def run(args, **kwargs):
            if "--list" in args:
                return subprocess.CompletedProcess(args, 0, LISTING, "")
            if "test" in args:
                executed.append(args[4])
                if args[4] == "phase1":
                    raise subprocess.TimeoutExpired(args, 600)
            return subprocess.CompletedProcess(args, 0)
        with patch.dict(os.environ, {"DPT_WINDOWS_SYNTHETIC_ROOT_DIAGNOSTICS": "1"}):
            self.assertEqual(self.invoke(run), 1)
        self.assertEqual(executed, list(checks.GROUPS))

    def test_containment_failure_stops_before_later_groups(self):
        executed = []
        def run(args, **kwargs):
            if "--list" in args:
                return subprocess.CompletedProcess(args, 0, LISTING, "")
            if "test" in args:
                executed.append(args[4])
                raise checks.ContainmentError("windows.job: class=cleanup_unconfirmed")
            return subprocess.CompletedProcess(args, 0)
        with patch.dict(os.environ, {"DPT_WINDOWS_SYNTHETIC_ROOT_DIAGNOSTICS": "1"}):
            self.assertEqual(self.invoke(run), 1)
        self.assertEqual(executed, ["phase1"])

    def test_wrong_revision_stops_before_compilation(self):
        with patch.object(checks.platform, "system", return_value="Windows"), \
                patch.object(checks.os, "chdir"), \
                patch.dict(os.environ, {"EXPECTED_SHA": "a" * 40}), \
                patch.object(checks.subprocess, "check_output", return_value="b" * 40), \
                patch.object(checks.subprocess, "run") as run, \
                contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(checks.main(), 2)
            run.assert_not_called()


def assert_fixture_handle_released(path):
    try:
        path.unlink()  # The native fixture opened without FILE_SHARE_DELETE.
    except OSError as error:
        raise AssertionError(
            f"windows.synthetic.handle_release: class=refused; code={getattr(error, 'winerror', error.errno)}"
        ) from None


class CleanupConfirmationTests(unittest.TestCase):
    def test_delayed_leader_then_descendant_termination(self):
        clock = [0.0]
        def wait(leader, milliseconds):
            self.assertEqual(leader, 1)
            self.assertEqual(milliseconds, 10000)
            clock[0] += 2  # Native wait completes later, within the budget.
            return 0
        def sleep(seconds): clock[0] += seconds
        active = Mock(side_effect=[1, 0])
        with patch.object(checks.time, 'monotonic', side_effect=lambda: clock[0]), \
                patch.object(checks.time, 'sleep', side_effect=sleep):
            checks.confirm_job_cleanup(1, wait, active)
        self.assertEqual(active.call_count, 2)

    def test_successful_reap_and_empty_job(self):
        wait, active = Mock(return_value=0), Mock(return_value=0)
        checks.confirm_job_cleanup(1, wait, active)
        self.assertGreater(wait.call_args.args[1], 0)
        self.assertLessEqual(wait.call_args.args[1], 10000)
        active.assert_called_once()

    def test_wait_failure_timeout_and_unexpected_are_refused(self):
        for outcome, category in [(0xffffffff, 'leader_wait_failed'),
                                  (258, 'leader_wait_timeout'), (128, 'leader_wait_unexpected')]:
            active = Mock()
            with patch.object(checks.ctypes, 'get_last_error', return_value=6, create=True):
                with self.assertRaisesRegex(checks.ContainmentError, category):
                    checks.confirm_job_cleanup(1, Mock(return_value=outcome), active)
            active.assert_not_called()

    def test_surviving_descendant_exhausts_shared_deadline(self):
        with patch.object(checks.time, 'monotonic', side_effect=[0, 0, 1, 10]), \
                patch.object(checks.time, 'sleep'):
            with self.assertRaisesRegex(checks.ContainmentError, 'cleanup_unconfirmed'):
                checks.confirm_job_cleanup(1, Mock(return_value=0), Mock(return_value=1))

    def test_inaccessible_accounting_is_not_success(self):
        with self.assertRaisesRegex(checks.ContainmentError, 'api_failure'):
            checks.confirm_job_cleanup(1, Mock(return_value=0),
                                       Mock(side_effect=checks.ContainmentError('windows.job: class=api_failure; code=5')))

    def test_handle_release_refusal_is_not_success(self):
        path = Mock()
        path.unlink.side_effect = PermissionError(13, 'synthetic private text')
        with self.assertRaisesRegex(AssertionError, 'class=refused; code=13') as error:
            assert_fixture_handle_released(path)
        self.assertNotIn('synthetic private text', str(error.exception))


@unittest.skipUnless(os.name == "nt", "Requires native Windows Job Objects")
class NativeContainmentTests(unittest.TestCase):
    def test_child_grandchild_timeout_releases_exclusive_handle(self):
        # Native proof, not a mocked subprocess. Only fresh fixture paths in memory.
        with tempfile.TemporaryDirectory() as root:
            fixture = Path(root)
            grandchild = """
import ctypes, os, time
from pathlib import Path
k = ctypes.WinDLL('kernel32', use_last_error=True)
k.CreateFileW.argtypes = [ctypes.c_wchar_p, ctypes.c_uint32, ctypes.c_uint32,
                         ctypes.c_void_p, ctypes.c_uint32, ctypes.c_uint32, ctypes.c_void_p]
k.CreateFileW.restype = ctypes.c_void_p
h = k.CreateFileW(os.environ['DPT_TEST_LOCK'], 0x40000000, 0, None, 2, 0x80, None)
assert h != ctypes.c_void_p(-1).value
Path(os.environ['DPT_TEST_READY']).write_text('ready')
time.sleep(60)
"""
            child = """
import os, subprocess, sys, time
subprocess.Popen([sys.executable, '-I', '-S', '-c', os.environ['DPT_TEST_GRANDCHILD']])
time.sleep(60)
"""
            env = os.environ.copy()
            env.update(DPT_TEST_LOCK=str(fixture / "held"),
                       DPT_TEST_READY=str(fixture / "ready"), DPT_TEST_GRANDCHILD=grandchild,
                       DPT_TEST_CHILD=child)
            leader = "import os,subprocess,sys,time; subprocess.Popen([sys.executable,'-I','-S','-c',os.environ['DPT_TEST_CHILD']]); time.sleep(60)"
            with self.assertRaises(subprocess.TimeoutExpired):
                checks.run_windows_job([sys.executable, "-I", "-S", "-c", leader], env, timeout=10)
            # run_windows_job also requires ActiveProcesses==0 and a reaped leader.
            self.assertTrue((fixture / "ready").exists(), "Grandchild must reach held-handle state")
            assert_fixture_handle_released(fixture / "held")

    def test_breakaway_is_refused(self):
        code = """
import subprocess, sys
args = [sys.executable, '-I', '-S', '-c', 'import time; time.sleep(60)']
# Same executable, arguments, environment and handle settings; only flags differ.
# Establish a real ordinary launch before accepting any negative result.
control = subprocess.Popen(args)
try:
    if control.poll() is not None:
        sys.exit(2)
finally:
    control.kill()
    control.wait(timeout=5)
try:
    escaped = subprocess.Popen(args, creationflags=subprocess.CREATE_BREAKAWAY_FROM_JOB)
except OSError as error:
    # ERROR_ACCESS_DENIED: job does not permit CREATE_BREAKAWAY_FROM_JOB.
    # Other errors (including unsupported/nested-runner failures) fail this fixture.
    sys.exit(0 if error.winerror == 5 else 3)
try:
    escaped.kill()
    escaped.wait(timeout=5)
finally:
    sys.exit(1)
"""
        result = checks.run_windows_job([sys.executable, "-I", "-S", "-c", code],
                                       os.environ.copy(), timeout=15)
        self.assertEqual(result.returncode, 0,
                         "windows.synthetic.breakaway: class=control_or_denial_failure")

    def test_normal_exit_cleans_descendant(self):
        env = os.environ.copy()
        code = "import subprocess,sys; subprocess.Popen([sys.executable,'-I','-S','-c','import time; time.sleep(60)'])"
        result = checks.run_windows_job([sys.executable, "-I", "-S", "-c", code], env, timeout=5)
        self.assertEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
