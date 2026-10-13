"""Future opt-in synthetic native suite. No production or workflow entry point.

Environment flags record a separately authorized run, not production approval.
Never inspect crash-report settings or publish fixture/report contents.
"""
import errno
import json
import os
from pathlib import Path
import platform
import shutil
import select
import signal
import subprocess
import sys
import tempfile
import time

from phase1_security_checks import ContainmentError, run_windows_job

CASES = ("normal", "retry", "unwind", "terminal", "panic_terminal", "rollback", "wal", "stall")
TERMINAL = {"terminal", "panic_terminal", "rollback", "wal"}
STATUS = 86


class Refusal(Exception):
    """Only fixed classifications reach output."""


def prerequisites(env, system, machine, sha):
    if env.get("DPT_SYNTHETIC_SQLITE_TERMINAL_TESTS") != "1":
        raise Refusal("disabled")
    expected = env.get("EXPECTED_SHA", "")
    if len(expected) != 40 or any(c not in "0123456789abcdef" for c in expected) or expected != sha:
        raise Refusal("unapproved_source")
    runners = {("Windows", "AMD64"): "windows-2022", ("Darwin", "arm64"): "macos-15", ("Darwin", "x86_64"): "macos-15-intel"}
    if env.get("NATIVE_RUNNER") != runners.get((system, machine)) or (system, machine) not in runners:
        raise Refusal("unsupported_platform")
    if env.get("GITHUB_ACTIONS") != "true" or env.get("GITHUB_REF") != "refs/heads/main":
        raise Refusal("unapproved_runner")
    if env.get("DPT_SYNTHETIC_REPORT_RISK") != "disposable-synthetic-only":
        raise Refusal("unsupported_privacy")
    if any(name in env for name in ("DPT_WINDOWS_ROOT_DIAGNOSTICS", "DPT_WINDOWS_SYNTHETIC_ROOT_DIAGNOSTICS")):
        raise Refusal("root_diagnostics_refused")


def group_exists(pgid):
    try:
        os.killpg(pgid, 0)
        return True
    except OSError as error:
        if error.errno == errno.ESRCH:
            return False
        raise Refusal("unsupported_cleanup") from None


class ProcessExit:
    """kqueue observation only; never wait/poll/reap the child here."""
    def __init__(self, pid):
        self.pid, self.exited, self.forked = pid, False, False
        try:
            self.queue = select.kqueue()
        except OSError:
            raise Refusal("unsupported_cleanup") from None
        try:
            event = select.kevent(pid, filter=select.KQ_FILTER_PROC,
                                  flags=select.KQ_EV_ADD | select.KQ_EV_ENABLE,
                                  fflags=select.KQ_NOTE_EXIT | select.KQ_NOTE_FORK)
            self._consume(self.queue.control([event], 1, 0))
        except Exception:
            try:
                self.queue.close()
            finally:
                raise Refusal("unsupported_cleanup") from None

    def _consume(self, events):
        if len(events) > 1:
            raise Refusal("unsupported_cleanup")
        for event in events:
            if event.ident != self.pid or event.filter != select.KQ_FILTER_PROC or event.flags & select.KQ_EV_ERROR:
                raise Refusal("unsupported_cleanup")
            if event.fflags & select.KQ_NOTE_FORK:
                self.forked = True
                raise Refusal("unsupported_cleanup")
            if not event.fflags & select.KQ_NOTE_EXIT or event.fflags & ~(select.KQ_NOTE_EXIT | select.KQ_NOTE_FORK):
                raise Refusal("unsupported_cleanup")
            self.exited = True

    def observe(self, seconds):
        if not self.exited:
            try:
                self._consume(self.queue.control(None, 1, max(0, seconds)))
            except OSError:
                # EINTR is a refusal, not an unbounded restart or successful exit.
                raise Refusal("unsupported_cleanup") from None
        return self.exited

    def close(self):
        try:
            self.queue.close()
        except OSError:
            raise Refusal("unsupported_cleanup") from None


def reap_owned(child, deadline):
    while time.monotonic() < deadline:
        try:
            pid, raw = os.waitpid(child.pid, os.WNOHANG)
        except OSError:
            # Unlike Popen.poll, ECHILD must not be converted to success.
            raise Refusal("unsupported_cleanup") from None
        if pid == child.pid:
            if not (os.WIFEXITED(raw) or os.WIFSIGNALED(raw)):
                raise Refusal("unsupported_cleanup")
            status = os.waitstatus_to_exitcode(raw)
            child.returncode = status  # No later Popen destructor/poll reaping.
            return status
        if pid != 0:
            raise Refusal("unsupported_cleanup")
        time.sleep(0.01)
    raise Refusal("unsupported_cleanup")


def cleanup_group(child, pgid, observer, budget=2):
    # Caller exclusively owns the child: no other waiter or SIGCHLD handler.
    if pgid != child.pid or pgid == os.getpgrp() or child.returncode is not None:
        raise Refusal("unsupported_cleanup")
    # Reserve PID until ALL signals are done. NOTE_EXIT is not a wait operation.
    if not observer.exited and group_exists(pgid):
        try:
            os.killpg(pgid, signal.SIGKILL)
        except OSError as error:
            if error.errno != errno.ESRCH:
                raise Refusal("unsupported_cleanup") from None
    deadline = time.monotonic() + budget
    while not observer.observe(min(0.05, max(0, deadline - time.monotonic()))):
        if time.monotonic() >= deadline:
            raise Refusal("unsupported_cleanup")
    status = reap_owned(child, deadline)
    # Never signal this ID after reaping. Reuse can cause conservative refusal,
    # never a signal to another process. Forks are outside the single-child scope.
    if observer.forked or group_exists(pgid):
        raise Refusal("unsupported_cleanup")
    return status


def mac_group(args, env, root, timeout=3):
    required = ("kqueue", "kevent", "KQ_FILTER_PROC", "KQ_EV_ADD", "KQ_EV_ENABLE",
                "KQ_EV_ERROR", "KQ_NOTE_EXIT", "KQ_NOTE_FORK")
    if platform.system() != "Darwin" or not all(hasattr(select, name) for name in required):
        raise Refusal("unsupported_cleanup")
    if signal.getsignal(signal.SIGCHLD) != signal.SIG_DFL:
        raise Refusal("unsupported_cleanup")
    try:
        child = subprocess.Popen(args, env=env, stdin=subprocess.DEVNULL,
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                 start_new_session=True)
    except OSError:
        raise Refusal("unsupported_cleanup") from None
    observer, isolated = None, False
    status = None
    try:
        # Both registration and isolation verification precede the start barrier.
        observer = ProcessExit(child.pid)
        try:
            verified = os.getpgid(child.pid) == child.pid and os.getsid(child.pid) == child.pid and child.pid != os.getpgrp()
        except OSError:
            raise Refusal("unsupported_cleanup") from None
        if not verified:
            raise Refusal("unsupported_cleanup")
        isolated = True
        (root / "go").write_bytes(b"contained")
        deadline = time.monotonic() + timeout
        while not observer.observe(min(0.05, max(0, deadline - time.monotonic()))):
            if time.monotonic() >= deadline:
                raise subprocess.TimeoutExpired("synthetic_native_child", timeout)
    finally:
        try:
            if isolated:
                status = cleanup_group(child, child.pid, observer)
            else:
                # Before go, trusted children do not fork. Signal the unreaped
                # owned PID only, then bounded reap; never signal an unknown group.
                try:
                    os.kill(child.pid, signal.SIGKILL)
                except OSError as error:
                    if error.errno != errno.ESRCH:
                        raise Refusal("unsupported_cleanup") from None
                reap_owned(child, time.monotonic() + 2)
        finally:
            if observer is not None:
                observer.close()
    return status


def mac_preflight():
    # Disposable control scripts only; future authorized native run, never auto.
    script = ("import os,time,pathlib,signal; signal.alarm(5); root=pathlib.Path(os.environ['DPT_NATIVE_SQLITE_ROOT']); "
              "root.joinpath('created').write_bytes(b'created'); "
              "\nwhile not root.joinpath('go').exists(): time.sleep(.01)\n")
    for stall in (False, True):
        root = Path(tempfile.mkdtemp(prefix="dpt-native-control-"))
        env = {"DPT_NATIVE_SQLITE_ROOT": str(root)}
        try:
            code = script + ("time.sleep(60)" if stall else "")
            try:
                status = mac_group([sys.executable, "-I", "-S", "-c", code], env, root, timeout=1)
            except subprocess.TimeoutExpired:
                if not stall or not (root / "created").is_file():
                    raise Refusal("unsupported_cleanup") from None
            else:
                if stall or status != 0 or not (root / "created").is_file():
                    raise Refusal("unsupported_cleanup")
        except Refusal:
            # Deliberately retain the fixture if process cleanup is uncertain.
            raise
        else:
            shutil.rmtree(root)


def windows_preflight():
    import unittest
    import test_phase1_security_checks as guards
    names = {"test_breakaway_is_refused", "test_child_grandchild_timeout_releases_exclusive_handle", "test_normal_exit_cleans_descendant"}
    discovered = {name for name in unittest.defaultTestLoader.getTestCaseNames(guards.NativeContainmentTests)}
    if discovered != names:
        raise Refusal("unsupported_cleanup")
    result = unittest.TestResult()
    unittest.defaultTestLoader.loadTestsFromTestCase(guards.NativeContainmentTests).run(result)
    if result.testsRun != 3 or not result.wasSuccessful() or result.skipped:
        raise Refusal("unsupported_cleanup")


def fixture_env(root, case, sha):
    # Keep OS executable-loading necessities, not arbitrary inherited credentials.
    env = {key: os.environ[key] for key in ("SystemRoot", "WINDIR", "PATH", "TEMP", "TMP") if key in os.environ}
    env.update(DPT_SYNTHETIC_SQLITE_TERMINAL_TESTS="1",
               DPT_SYNTHETIC_REPORT_RISK="disposable-synthetic-only",
               DPT_NATIVE_SQLITE_ROOT=str(root), DPT_NATIVE_SQLITE_CASE=case,
               EXPECTED_SHA=sha, NATIVE_RUNNER=os.environ["NATIVE_RUNNER"],
               GITHUB_ACTIONS="true", GITHUB_REF="refs/heads/main",
               DPT_NATIVE_SQLITE_PREFLIGHT="verified")
    return env


def execute(executable, root, case, system, sha):
    for name in ("go", "privacy_ready", "completed", "recovery_confirmed"):
        (root / name).unlink(missing_ok=True)
    args = [str(executable), "--exact", "native_child", "--ignored", "--test-threads=1"]
    env = fixture_env(root, case, sha)
    if system == "Windows":
        # Assignment and membership verification precede resume inside this helper.
        (root / "go").write_bytes(b"contained")
        try:
            return run_windows_job(args, env, timeout=3).returncode
        except ContainmentError:
            raise Refusal("unsupported_cleanup") from None
    return mac_group(args, env, root)


def assess(root, case, status):
    if (root / "unsupported_privacy").is_file():
        raise Refusal("unsupported_privacy")
    if not (root / "privacy_ready").is_file() or (root / "unexpected_close_failure").exists():
        raise Refusal("fixture_preflight_failed")
    if case in TERMINAL:
        if status != STATUS or not all((root / name).is_file() for name in ("busy_confirmed", "terminal_ready")):
            raise Refusal("terminal_unconfirmed")
        if any((root / name).exists() for name in ("guards_dropped", "staging_released", "completed")):
            raise Refusal("premature_cleanup")
        if case == "panic_terminal" and not (root / "panic_ready").is_file():
            raise Refusal("terminal_unconfirmed")
    elif case.startswith("recover_"):
        if status != 0 or not (root / "recovery_confirmed").is_file() or any(
                (root / name).exists() for name in ("guards_dropped", "staging_released")):
            raise Refusal("recovery_unconfirmed")
    elif status != 0 or not all((root / name).is_file() for name in ("completed", "guards_dropped", "staging_released")):
        raise Refusal("normal_close_unconfirmed")
    if case == "retry" and not (root / "busy_confirmed").is_file():
        raise Refusal("busy_unconfirmed")


def recovery_material(root, case):
    if not (root / "pending_ready").is_file() or not (root / "stage/synthetic.sqlite").is_file():
        raise Refusal("pending_unconfirmed")
    suffix = "-journal" if case == "rollback" else "-wal"
    evidence = root / ("stage/synthetic.sqlite" + suffix)
    if not evidence.is_file() or evidence.stat().st_size <= (512 if case == "rollback" else 32):
        raise Refusal("recovery_material_missing")
    if case == "rollback":
        journal = evidence.read_bytes()
        if not journal.startswith(bytes.fromhex("d9d505f920a163d7")) or journal[8:12] == bytes(4):
            raise Refusal("hot_journal_unconfirmed")
        if (root / "stage/synthetic.sqlite").read_bytes() == (root / "baseline-image").read_bytes():
            raise Refusal("disk_write_unconfirmed")


def windows_handle_release(root):
    import ctypes
    from ctypes import wintypes as w
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.CreateFileW.argtypes = [w.LPCWSTR, w.DWORD, w.DWORD, ctypes.c_void_p, w.DWORD, w.DWORD, w.HANDLE]
    kernel.CreateFileW.restype = w.HANDLE
    kernel.CloseHandle.argtypes, kernel.CloseHandle.restype = [w.HANDLE], w.BOOL
    handle = kernel.CreateFileW(str(root / "exclusive"), 0x80000000, 0, None, 3, 0x80, None)
    if handle == ctypes.c_void_p(-1).value:
        raise Refusal("exclusive_handle_unreleased")
    if not kernel.CloseHandle(handle):
        raise Refusal("unsupported_cleanup")


def main():
    # A structurally separate mode: no manifest/executable loading, fixture
    # environment, execute(), recovery or terminal assessment is reachable.
    if sys.argv[1:] == ["--containment-only"]:
        return containment_only()
    # Even --run requires exact gates and a separately approved synthetic profile.
    if sys.argv[1:] != ["--run"]:
        print("native.sqlite: class=disabled")
        return 2
    sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    system = platform.system()
    prerequisites(os.environ, system, platform.machine(), sha)
    if subprocess.check_output(["git", "status", "--porcelain=v1"], text=True).strip():
        raise Refusal("unapproved_source")
    executable = approved_executable(sha)
    (windows_preflight if system == "Windows" else mac_preflight)()
    for case in CASES:
        root = Path(tempfile.mkdtemp(prefix="dpt-native-sqlite-"))
        # No automatic teardown on uncertain cleanup, no automatic rerun.
        try:
            status = execute(executable, root, case, system, sha)
        except subprocess.TimeoutExpired:
            if case != "stall" or not all((root / name).is_file() for name in ("handles_ready", "privacy_ready")):
                raise Refusal("fixture_timeout") from None
        else:
            if case == "stall":
                raise Refusal("stall_missing")
            assess(root, case, status)
            if system == "Windows":
                windows_handle_release(root)
            if case in ("rollback", "wal"):
                recovery_material(root, case)
                result = execute(executable, root, "recover_" + case, system, sha)
                assess(root, "recover_" + case, result)
        if case == "stall" and system == "Windows":
            windows_handle_release(root)
        shutil.rmtree(root)
        print(f"native.sqlite: case={case}; class=verified")
    return 0


def containment_only():
    if os.environ.get("DPT_NATIVE_SQLITE_SAFE_CONTAINMENT") != "1":
        raise Refusal("disabled")
    sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True, timeout=5).strip()
    # Reuse source/runner/privacy/root exclusions, without enabling the terminal
    # gate in the process environment or handing it to a child.
    records = dict(os.environ)
    records["DPT_SYNTHETIC_SQLITE_TERMINAL_TESTS"] = "1"
    system = platform.system()
    prerequisites(records, system, platform.machine(), sha)
    if subprocess.check_output(["git", "status", "--porcelain=v1"], text=True, timeout=5).strip():
        raise Refusal("unapproved_source")
    (windows_preflight if system == "Windows" else mac_preflight)()
    print("native.sqlite: class=containment_verified; terminal_executed=false")
    return 0


def approved_executable(sha):
    # Future workflow creates this manifest from --no-run Cargo JSON after an
    # independently supervised build. This driver never launches Cargo/rustc.
    manifest = os.environ.get("DPT_NATIVE_SQLITE_MANIFEST")
    if not manifest:
        raise Refusal("fixture_inventory_mismatch")
    target = (Path.cwd() / "target").resolve()
    manifest = Path(manifest).resolve()
    if not manifest.is_relative_to(target):
        raise Refusal("fixture_inventory_mismatch")
    data = json.loads(manifest.read_text())
    if data.get("sha") != sha:
        raise Refusal("unapproved_source")
    executable = Path(data.get("executable", "")).resolve()
    if not executable.is_relative_to(target) or not executable.is_file():
        raise Refusal("fixture_inventory_mismatch")
    inventory = data.get("target")
    if inventory != "native_sqlite_terminal_fixtures":
        raise Refusal("fixture_inventory_mismatch")
    return executable


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Refusal as error:
        print("native.sqlite: class=" + str(error))
        sys.exit(1)
    except Exception:
        print("native.sqlite: class=unsupported_cleanup_or_fixture_error")
        sys.exit(1)
