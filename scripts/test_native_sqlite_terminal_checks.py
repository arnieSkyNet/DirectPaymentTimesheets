"""Mocks only: no child creation, signaling, SQLite or terminal execution."""
import errno
import json
from pathlib import Path
import unittest
from unittest.mock import MagicMock, Mock, patch

import native_sqlite_terminal_checks as native

SHA = "a" * 40


class Gates(unittest.TestCase):
    def test_safe_build_recipe_has_only_compile_and_listing_arguments(self):
        data = json.loads((Path(__file__).resolve().parents[1] / "tests/native_sqlite_safe_build_manifest.json").read_text())
        self.assertEqual(data["build"], ["cargo", "test", "--locked", "--test", "native_sqlite_terminal_fixtures", "--no-run", "--message-format=json"])
        self.assertEqual(data["inventory_arguments"], ["--list", "--ignored"])
        self.assertEqual(data["inventory_nonempty_lines"], ["native_child: test", "1 test, 0 benchmarks"])
        self.assertEqual(data["containment"][-1], "--containment-only")
        self.assertFalse(data["terminal_execution"])
        self.assertFalse(data["root_diagnostics"])

    def test_safe_mode_macos_uses_only_control_preflight(self):
        for machine, runner in (("arm64", "macos-15"), ("x86_64", "macos-15-intel")):
            env = self.env()
            env.update(NATIVE_RUNNER=runner, DPT_NATIVE_SQLITE_SAFE_CONTAINMENT="1")
            del env["DPT_SYNTHETIC_SQLITE_TERMINAL_TESTS"]
            with patch.dict(native.os.environ, env, clear=True), patch.object(native.platform, "system", return_value="Darwin"), patch.object(native.platform, "machine", return_value=machine), patch.object(native.subprocess, "check_output", side_effect=[SHA, ""]), patch.object(native, "mac_preflight") as controls, patch.object(native, "execute") as execute, patch("builtins.print"):
                self.assertEqual(native.containment_only(), 0)
                controls.assert_called_once_with()
                execute.assert_not_called()
                self.assertNotIn("DPT_SYNTHETIC_SQLITE_TERMINAL_TESTS", native.os.environ)

    def test_combined_modes_refuse_before_any_execution(self):
        with patch.object(native.sys, "argv", ["checks", "--containment-only", "--run"]), patch.object(native, "execute") as execute, patch.object(native.subprocess, "check_output") as git, patch("builtins.print"):
            self.assertEqual(native.main(), 2)
            execute.assert_not_called()
            git.assert_not_called()

    def test_safe_mode_is_independent_and_never_loads_sqlite(self):
        env = self.env()
        env.update(DPT_NATIVE_SQLITE_SAFE_CONTAINMENT="1",
                   DPT_NATIVE_SQLITE_CASE="panic_terminal")
        with patch.dict(native.os.environ, env, clear=True), patch.object(native.sys, "argv", ["checks", "--containment-only"]), patch.object(native.platform, "system", return_value="Windows"), patch.object(native.platform, "machine", return_value="AMD64"), patch.object(native.subprocess, "check_output", side_effect=[SHA, ""]), patch.object(native, "windows_preflight") as controls, patch.object(native, "approved_executable") as executable, patch.object(native, "execute") as execute, patch.object(native, "fixture_env") as fixture_env, patch("builtins.print"):
            self.assertEqual(native.main(), 0)
            controls.assert_called_once_with()
            executable.assert_not_called()
            execute.assert_not_called()
            fixture_env.assert_not_called()

    def test_safe_mode_exact_gate(self):
        for value in (None, "0", "true", " 1", "1 "):
            env = {} if value is None else {"DPT_NATIVE_SQLITE_SAFE_CONTAINMENT": value}
            with patch.dict(native.os.environ, env, clear=True), patch.object(native.subprocess, "check_output") as git:
                with self.assertRaisesRegex(native.Refusal, "disabled"):
                    native.containment_only()
                git.assert_not_called()

    def test_safe_mode_root_diagnostics_block_controls(self):
        env = self.env()
        env.update(DPT_NATIVE_SQLITE_SAFE_CONTAINMENT="1", DPT_WINDOWS_SYNTHETIC_ROOT_DIAGNOSTICS="1")
        with patch.dict(native.os.environ, env, clear=True), patch.object(native.platform, "system", return_value="Windows"), patch.object(native.platform, "machine", return_value="AMD64"), patch.object(native.subprocess, "check_output", return_value=SHA), patch.object(native, "windows_preflight") as controls:
            with self.assertRaisesRegex(native.Refusal, "root_diagnostics_refused"):
                native.containment_only()
            controls.assert_not_called()

    def test_safe_mode_cleanup_refusal_stops(self):
        env = self.env()
        env["DPT_NATIVE_SQLITE_SAFE_CONTAINMENT"] = "1"
        with patch.dict(native.os.environ, env, clear=True), patch.object(native.platform, "system", return_value="Windows"), patch.object(native.platform, "machine", return_value="AMD64"), patch.object(native.subprocess, "check_output", side_effect=[SHA, ""]), patch.object(native, "windows_preflight", side_effect=native.Refusal("unsupported_cleanup")), patch.object(native, "execute") as execute:
            with self.assertRaisesRegex(native.Refusal, "unsupported_cleanup"):
                native.containment_only()
            execute.assert_not_called()

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


class WindowsPrerequisiteOrder(unittest.TestCase):
    names = ("test_breakaway_is_refused",
             "test_child_grandchild_timeout_releases_exclusive_handle",
             "test_normal_exit_cleans_descendant")

    def run_controls(self, failure=None, position=None):
        import test_phase1_security_checks as guards
        observed = []

        def control(index):
            def test(case):
                observed.append(self.names[index])
                if index == position:
                    raise failure("synthetic refusal")
            return test

        # Real unittest dispatch/result behavior, fake control bodies only.
        controls = type("SyntheticControls", (unittest.TestCase,),
                        {name: control(index) for index, name in enumerate(self.names)})
        with patch.object(guards, "NativeContainmentTests", controls):
            if failure is None:
                native.windows_preflight()
            else:
                with self.assertRaisesRegex(native.Refusal, "unsupported_cleanup"):
                    native.windows_preflight()
        return observed

    def assert_stops(self, failure):
        for position in range(3):
            with self.subTest(position=position):
                self.assertEqual(self.run_controls(failure, position),
                                 list(self.names[:position + 1]))

    def test_cleanup_uncertainty_stops_later_controls(self):
        self.assert_stops(native.ContainmentError)

    def test_assertion_failure_stops_later_controls(self):
        self.assert_stops(AssertionError)

    def test_unexpected_exception_stops_later_controls(self):
        self.assert_stops(RuntimeError)

    def test_skip_stops_later_controls(self):
        self.assert_stops(unittest.SkipTest)

    def test_success_runs_exactly_three_controls(self):
        self.assertEqual(self.run_controls(), list(self.names))

    def test_stopped_or_incomplete_suite_never_passes(self):
        for count, stopped in ((0, False), (2, False), (3, True)):
            def run(suite, result):
                result.testsRun = count
                if stopped:
                    result.stop()
            with self.subTest(count=count, stopped=stopped), patch.object(unittest.TestSuite, "run", run):
                with self.assertRaisesRegex(native.Refusal, "unsupported_cleanup"):
                    native.windows_preflight()


class Supervisor(unittest.TestCase):
    def setUp(self):
        # Fake APIs on ALL hosts: never create a kqueue, process or real signal.
        self.addCleanup(patch.stopall)
        self.os = MagicMock(WNOHANG=1)
        self.os.getpgrp.return_value = 101
        self.os.getpgid.return_value = self.os.getsid.return_value = 100
        patch.object(native, "os", self.os).start()
        self.select = MagicMock(KQ_FILTER_PROC=-5, KQ_EV_ADD=1, KQ_EV_ENABLE=4,
                                KQ_EV_ERROR=0x4000, KQ_NOTE_EXIT=1, KQ_NOTE_FORK=2)
        patch.object(native, "select", self.select).start()
        patch.object(native, "signal", Mock(SIGKILL=9, SIGCHLD=20, SIG_DFL=0,
                                            getsignal=Mock(return_value=0))).start()
        self.child = Mock(pid=100, returncode=None)

    def event(self, flags=0, fflags=1, pid=100):
        return Mock(ident=pid, filter=-5, flags=flags, fflags=fflags)

    def observer(self, initial=()):
        self.select.kqueue.return_value.control.return_value = list(initial)
        return native.ProcessExit(100)

    def test_kqueue_registration_is_nonreaping(self):
        observer = self.observer()
        self.select.kevent.assert_called_once_with(100, filter=-5, flags=5, fflags=3)
        self.os.waitpid.assert_not_called()
        self.assertFalse(observer.exited)

    def test_initial_exit_before_cleanup_is_retained(self):
        observer = self.observer([self.event()])
        self.assertTrue(observer.exited)
        self.os.waitpid.assert_not_called()

    def test_only_exit_event_can_complete_observation(self):
        for event in (self.event(flags=0x4000), self.event(pid=101), self.event(fflags=0)):
            with self.assertRaisesRegex(native.Refusal, "unsupported_cleanup"):
                self.observer([event])

    def test_fork_is_unsupported_not_successful_tree_accounting(self):
        with self.assertRaisesRegex(native.Refusal, "unsupported_cleanup"):
            self.observer([self.event(fflags=3)])

    def test_interrupted_observation_fails_closed(self):
        observer = self.observer()
        observer.queue.control.side_effect = InterruptedError(errno.EINTR, "private")
        with self.assertRaisesRegex(native.Refusal, "unsupported_cleanup"):
            observer.observe(.05)
        self.os.waitpid.assert_not_called()

    def test_observation_timeout_does_not_reap(self):
        observer = self.observer()
        self.assertFalse(observer.observe(.01))
        observer.queue.control.assert_called_with(None, 1, .01)
        self.os.waitpid.assert_not_called()

    def test_registration_error_closes_queue(self):
        self.select.kqueue.return_value.control.side_effect = OSError(errno.ESRCH, "private")
        with self.assertRaises(native.Refusal):
            native.ProcessExit(100)
        self.select.kqueue.return_value.close.assert_called_once()

    def test_queue_close_failure_refuses_cleanup(self):
        observer = self.observer()
        observer.queue.close.side_effect = OSError(errno.EBADF, "private")
        with self.assertRaisesRegex(native.Refusal, "unsupported_cleanup"):
            observer.close()

    def test_esrch_is_empty_but_permission_failure_is_not(self):
        self.os.killpg.side_effect = OSError(errno.ESRCH, "private")
        self.assertFalse(native.group_exists(100))
        self.os.killpg.side_effect = OSError(errno.EPERM, "private")
        with self.assertRaises(native.Refusal): native.group_exists(100)

    def test_parent_group_and_reaped_leader_cannot_be_signaled(self):
        observer = Mock(exited=False, forked=False)
        for pgid, status in ((101, None), (100, 0)):
            self.child.returncode = status
            with self.assertRaises(native.Refusal):
                native.cleanup_group(self.child, pgid, observer)
        self.os.killpg.assert_not_called()

    def test_stall_kill_precedes_reap(self):
        observer = Mock(exited=False, forked=False)
        observer.observe.return_value = True
        order = []
        self.os.killpg.side_effect = lambda *args: order.append("signal")
        def reaped(*args):
            order.append("reap")
            return -9
        with patch.object(native, "group_exists", side_effect=[True, False]), patch.object(native, "reap_owned", side_effect=reaped) as reap:
            native.cleanup_group(self.child, 100, observer)
            self.os.killpg.assert_called_once_with(100, 9)
            reap.assert_called_once()
            self.assertEqual(order, ["signal", "reap"])

    def test_nondefault_sigchld_refuses_before_spawn(self):
        native.signal.getsignal.return_value = 1
        with patch.object(native.platform, "system", return_value="Darwin"), patch.object(native.subprocess, "Popen") as spawn:
            with self.assertRaises(native.Refusal): native.mac_group(["fixture"], {}, Mock())
            spawn.assert_not_called()

    def test_missing_kqueue_refuses_before_spawn(self):
        del self.select.kqueue
        with patch.object(native.platform, "system", return_value="Darwin"), patch.object(native.subprocess, "Popen") as spawn:
            with self.assertRaises(native.Refusal): native.mac_group(["fixture"], {}, Mock())
            spawn.assert_not_called()

    def test_normal_exit_never_signals_after_reap(self):
        observer = Mock(exited=True, forked=False)
        observer.observe.return_value = True
        with patch.object(native, "group_exists", return_value=False), patch.object(native, "reap_owned", return_value=0):
            self.assertEqual(native.cleanup_group(self.child, 100, observer), 0)
        self.os.killpg.assert_not_called()

    def test_unexpected_survivor_never_passes_or_is_signaled_after_reap(self):
        observer = Mock(exited=True, forked=False)
        observer.observe.return_value = True
        with patch.object(native, "group_exists", return_value=True), patch.object(native, "reap_owned", return_value=0):
            with self.assertRaises(native.Refusal):
                native.cleanup_group(self.child, 100, observer)
        self.os.killpg.assert_not_called()

    def test_signal_failure_is_not_success(self):
        observer = Mock(exited=False, forked=False)
        self.os.killpg.side_effect = OSError(errno.EPERM, "private")
        with patch.object(native, "group_exists", return_value=True), patch.object(native, "reap_owned") as reap:
            with self.assertRaises(native.Refusal): native.cleanup_group(self.child, 100, observer)
            reap.assert_not_called()

    def test_cleanup_deadline_never_hangs(self):
        observer = Mock(exited=False, forked=False)
        observer.observe.return_value = False
        with patch.object(native, "group_exists", return_value=False), patch.object(native.time, "monotonic", side_effect=[0, 0, 3]):
            with self.assertRaises(native.Refusal): native.cleanup_group(self.child, 100, observer)

    def test_reap_echild_or_interruption_is_not_assumed_success(self):
        for code in (errno.ECHILD, errno.EINTR):
            self.os.waitpid.side_effect = OSError(code, "private")
            with self.assertRaises(native.Refusal): native.reap_owned(self.child, native.time.monotonic()+1)
        self.assertIsNone(self.child.returncode)

    def test_real_status_interface_is_used_only_for_reaping(self):
        self.os.waitpid.return_value = (100, 0)
        self.os.WIFEXITED.return_value = True
        self.os.waitstatus_to_exitcode.return_value = 0
        self.assertEqual(native.reap_owned(self.child, native.time.monotonic()+1), 0)
        self.os.waitpid.assert_called_once_with(100, 1)
        self.assertEqual(self.child.returncode, 0)

    def test_mac_registration_and_isolation_precede_start_barrier(self):
        root = MagicMock(spec=Path)
        observer = Mock()
        observer.observe.return_value = True
        with patch.object(native.platform, "system", return_value="Darwin"), patch.object(native.subprocess, "Popen", return_value=self.child), patch.object(native, "ProcessExit", return_value=observer), patch.object(native, "cleanup_group", return_value=0) as cleanup:
            self.assertEqual(native.mac_group(["fixture"], {}, root), 0)
            root.__truediv__.assert_called_once_with("go")
            cleanup.assert_called_once_with(self.child, 100, observer)
            observer.close.assert_called_once()

    def test_wrong_group_never_releases_barrier(self):
        root = MagicMock(spec=Path)
        self.os.getpgid.return_value = 101
        with patch.object(native.platform, "system", return_value="Darwin"), patch.object(native.subprocess, "Popen", return_value=self.child), patch.object(native, "ProcessExit") as observer, patch.object(native, "reap_owned", return_value=-9), patch.object(native, "cleanup_group") as cleanup:
            with self.assertRaises(native.Refusal): native.mac_group(["fixture"], {}, root)
            root.__truediv__.assert_not_called()
            self.os.kill.assert_called_once_with(100, 9)
            cleanup.assert_not_called()
            observer.return_value.close.assert_called_once()

    def test_timeout_cannot_escape_before_cleanup_confirmation(self):
        observer = Mock()
        observer.observe.return_value = False
        with patch.object(native.platform, "system", return_value="Darwin"), patch.object(native.subprocess, "Popen", return_value=self.child), patch.object(native, "ProcessExit", return_value=observer), patch.object(native.time, "monotonic", side_effect=[0, 0, 4]), patch.object(native, "cleanup_group", side_effect=native.Refusal("unsupported_cleanup")):
            with self.assertRaises(native.Refusal): native.mac_group(["fixture"], {}, MagicMock(spec=Path))

    def test_terminal_status_requires_real_phase_markers(self):
        root = MagicMock(spec=Path)
        root.__truediv__.return_value.is_file.return_value = False
        with self.assertRaises(native.Refusal): native.assess(root, "terminal", 86)

    def test_wrong_terminal_status_is_refused(self):
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

    def test_windows_prerequisites_unchanged(self):
        for skip in (False, True):
            def run(suite, result):
                result.testsRun = 3
                (result.skipped if skip else result.failures).append((None, "refused"))
            with patch.object(unittest.TestSuite, "run", run):
                with self.assertRaises(native.Refusal): native.windows_preflight()


if __name__ == "__main__":
    unittest.main()
