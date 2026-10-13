"""Test the runner's failure accounting without running cargo or touching data."""
import contextlib
import io
import os
import subprocess
import unittest
import tempfile
import sys
from pathlib import Path
from unittest.mock import patch

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
            (fixture / "held").unlink()  # No FILE_SHARE_DELETE: succeeds only after handle release.

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
