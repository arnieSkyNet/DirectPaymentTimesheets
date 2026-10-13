"""Mocks only: no child creation, signaling, SQLite or terminal execution."""
import errno
import os
from pathlib import Path
import unittest
from unittest.mock import MagicMock, Mock, patch

import native_sqlite_terminal_checks as native

SHA = "a" * 40


class Gates(unittest.TestCase):
    def env(self):
        return dict(DPT_SYNTHETIC_SQLITE_TERMINAL_TESTS="1", EXPECTED_SHA=SHA,
                    GITHUB_ACTIONS="true", GITHUB_REF="refs/heads/main",
                    NATIVE_RUNNER="windows-2022",
                    DPT_SYNTHETIC_REPORT_RISK="disposable-synthetic-only")

    def test_exact_gate_and_approved_runner(self):
        native.prerequisites(self.env(), "Windows", "AMD64", SHA)
        for value in (None, "0", "true", " 1", "1 "):
            env = self.env()
            if value is None:
                del env["DPT_SYNTHETIC_SQLITE_TERMINAL_TESTS"]
            else:
                env["DPT_SYNTHETIC_SQLITE_TERMINAL_TESTS"] = value
            with self.assertRaisesRegex(native.Refusal, "disabled"):
                native.prerequisites(env, "Windows", "AMD64", SHA)

    def test_source_runner_and_privacy_fail_closed(self):
        for key, value in (("EXPECTED_SHA", "b" * 40), ("GITHUB_REF", "refs/heads/security/v1.0.7-phase1"),
                           ("NATIVE_RUNNER", "macos-15"), ("DPT_SYNTHETIC_REPORT_RISK", "")):
            env = self.env()
            env[key] = value
            with self.assertRaises(native.Refusal):
                native.prerequisites(env, "Windows", "AMD64", SHA)
        with self.assertRaisesRegex(native.Refusal, "unsupported_platform"):
            native.prerequisites(self.env(), "Linux", "x86_64", SHA)

    def test_root_diagnostics_are_never_inherited(self):
        for key in ("DPT_WINDOWS_ROOT_DIAGNOSTICS", "DPT_WINDOWS_SYNTHETIC_ROOT_DIAGNOSTICS"):
            env = self.env()
            env[key] = "0"
            with self.assertRaisesRegex(native.Refusal, "root_diagnostics_refused"):
                native.prerequisites(env, "Windows", "AMD64", SHA)

    def test_macos_runner_architectures(self):
        for machine, runner in (("arm64", "macos-15"), ("x86_64", "macos-15-intel")):
            env = self.env()
            env["NATIVE_RUNNER"] = runner
            native.prerequisites(env, "Darwin", machine, SHA)

    def test_default_cli_never_queries_git_or_starts_children(self):
        with patch.object(native.sys, "argv", ["checks"]), patch.object(native.subprocess, "check_output") as git, patch.object(native.subprocess, "Popen") as spawn, patch("builtins.print"):
            self.assertEqual(native.main(), 2)
            git.assert_not_called()
            spawn.assert_not_called()

    def test_missing_manifest_refuses_before_execution(self):
        with patch.dict(native.os.environ, {}, clear=True):
            with self.assertRaisesRegex(native.Refusal, "fixture_inventory_mismatch"):
                native.approved_executable(SHA)


class Supervisor(unittest.TestCase):
    def setUp(self):
        # Make mocked Unix paths portable without adding missing APIs to the
        # real Windows os module or ever invoking a real signal operation.
        fake_os = MagicMock()
        for name, fallback in (("P_PID", 1), ("WEXITED", 4), ("WNOHANG", 1),
                               ("WNOWAIT", 0x1000000), ("CLD_EXITED", 1),
                               ("CLD_KILLED", 2), ("CLD_DUMPED", 3)):
            setattr(fake_os, name, getattr(os, name, fallback))
        self.addCleanup(patch.stopall)
        patch.object(native, "os", fake_os).start()
        patch.object(native, "signal", Mock(SIGKILL=9)).start()

    def test_esrch_is_empty_but_permission_failure_is_not(self):
        with patch.object(native.os, "killpg", side_effect=OSError(errno.ESRCH, "private")):
            self.assertFalse(native.group_exists(100))
        with patch.object(native.os, "killpg", side_effect=OSError(errno.EPERM, "private")):
            with self.assertRaisesRegex(native.Refusal, "unsupported_cleanup"):
                native.group_exists(100)

    def test_parent_group_cannot_be_killed(self):
        child = Mock(pid=100)
        with patch.object(native.os, "getpgrp", return_value=100), patch.object(native.os, "killpg") as kill:
            with self.assertRaises(native.Refusal):
                native.cleanup_group(child, 100)
            kill.assert_not_called()

    def test_verified_group_kill_and_reap(self):
        child = Mock(pid=100)
        child.poll.return_value = -9
        with patch.object(native.os, "getpgrp", return_value=101), patch.object(native, "group_exists", side_effect=[True, False]), patch.object(native, "exit_observation", return_value=-9), patch.object(native.os, "killpg") as kill:
            native.cleanup_group(child, 100)
            kill.assert_called_once_with(100, native.signal.SIGKILL)

    def test_survivor_never_passes_cleanup(self):
        child = Mock(pid=100)
        child.poll.return_value = None
        with patch.object(native.os, "getpgrp", return_value=101), patch.object(native, "group_exists", return_value=True), patch.object(native, "exit_observation", return_value=None), patch.object(native.os, "killpg"), patch.object(native.time, "monotonic", side_effect=[0, 3]):
            with self.assertRaisesRegex(native.Refusal, "unsupported_cleanup"):
                native.cleanup_group(child, 100)

    def test_terminal_status_without_markers_is_refused(self):
        root = MagicMock(spec=Path)
        node = root.__truediv__.return_value
        node.is_file.return_value = False
        node.exists.return_value = False
        with self.assertRaises(native.Refusal):
            native.assess(root, "terminal", native.STATUS)

    def test_expected_status_requires_busy_and_terminal_phases(self):
        root = MagicMock(spec=Path)
        def node(name):
            item = Mock()
            item.is_file.return_value = name == "privacy_ready"
            item.exists.return_value = False
            return item
        root.__truediv__.side_effect = node
        with self.assertRaisesRegex(native.Refusal, "terminal_unconfirmed"):
            native.assess(root, "terminal", native.STATUS)

    def test_signal_or_timeout_status_cannot_pass(self):
        root = MagicMock(spec=Path)
        def node(name):
            item = Mock()
            item.is_file.return_value = name in ("privacy_ready", "busy_confirmed", "terminal_ready")
            item.exists.return_value = False
            return item
        root.__truediv__.side_effect = node
        for status in (-6, -9, 1, 101):
            with self.assertRaisesRegex(native.Refusal, "terminal_unconfirmed"):
                native.assess(root, "terminal", status)

    def test_waitid_keeps_pid_reserved(self):
        observation = Mock(si_pid=100, si_code=native.os.CLD_EXITED, si_status=86)
        with patch.object(native.os, "waitid", return_value=observation) as wait:
            self.assertEqual(native.exit_observation(100), 86)
            wait.assert_called_once_with(native.os.P_PID, 100, native.os.WEXITED | native.os.WNOHANG | native.os.WNOWAIT)

    def test_inaccessible_waitid_is_not_exit(self):
        with patch.object(native.os, "waitid", side_effect=OSError(errno.ECHILD, "private")):
            with self.assertRaisesRegex(native.Refusal, "unsupported_cleanup"):
                native.exit_observation(100)

    def test_mac_isolation_precedes_barrier_and_cleanup(self):
        child = Mock(pid=100)
        root = MagicMock(spec=Path)
        with patch.object(native.platform, "system", return_value="Darwin"), patch.object(native.subprocess, "Popen", return_value=child) as spawn, patch.object(native.os, "getpgid", return_value=100), patch.object(native.os, "getsid", return_value=100), patch.object(native.os, "getpgrp", return_value=101), patch.object(native, "exit_observation", return_value=0), patch.object(native, "cleanup_group") as cleanup:
            self.assertEqual(native.mac_group(["fixture"], {}, root), 0)
            self.assertTrue(spawn.call_args.kwargs["start_new_session"])
            root.__truediv__.assert_called_with("go")
            cleanup.assert_called_once_with(child, 100)

    def test_mac_timeout_requires_cleanup_before_escape(self):
        child = Mock(pid=100)
        root = MagicMock(spec=Path)
        with patch.object(native.platform, "system", return_value="Darwin"), patch.object(native.subprocess, "Popen", return_value=child), patch.object(native.os, "getpgid", return_value=100), patch.object(native.os, "getsid", return_value=100), patch.object(native.os, "getpgrp", return_value=101), patch.object(native, "exit_observation", return_value=None), patch.object(native.time, "monotonic", side_effect=[0, 0, 4]), patch.object(native.time, "sleep"), patch.object(native, "cleanup_group", side_effect=native.Refusal("unsupported_cleanup")):
            with self.assertRaisesRegex(native.Refusal, "unsupported_cleanup"):
                native.mac_group(["fixture"], {}, root)

    def test_mac_wrong_group_does_not_release_barrier_or_kill_group(self):
        child = Mock(pid=100)
        child.poll.return_value = 0
        root = MagicMock(spec=Path)
        with patch.object(native.platform, "system", return_value="Darwin"), patch.object(native.subprocess, "Popen", return_value=child), patch.object(native.os, "getpgid", return_value=101), patch.object(native, "exit_observation", return_value=None), patch.object(native, "cleanup_group") as cleanup:
            with self.assertRaisesRegex(native.Refusal, "unsupported_cleanup"):
                native.mac_group(["fixture"], {}, root)
            root.__truediv__.assert_not_called()
            child.kill.assert_called_once()
            cleanup.assert_not_called()

    def test_windows_prerequisite_failure_or_skip_blocks(self):
        # Patch the runner result; never execute native containment fixtures here.
        import test_phase1_security_checks as guards
        for skip in (False, True):
            def run(suite, result):
                result.testsRun = 3
                if skip:
                    result.skipped.append((None, "unsupported"))
                else:
                    result.failures.append((None, "failed"))
            with patch.object(unittest.TestSuite, "run", run):
                with self.assertRaises(native.Refusal):
                    native.windows_preflight()


if __name__ == "__main__":
    unittest.main()
