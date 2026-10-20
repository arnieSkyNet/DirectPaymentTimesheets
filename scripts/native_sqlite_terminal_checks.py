"""Opt-in synthetic native suite. No production entry point.

Environment flags record a separately authorized run, not production approval.
Never inspect crash-report settings or publish fixture/report contents.
"""
import hashlib
import struct
import re
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

# Support isolated Python invocation without trusting PYTHONPATH or cwd.
sys.path.insert(0, str(Path(__file__).resolve().parent))

from phase1_security_checks import ContainmentError, run_windows_job
from native_sqlite_fixture_storage import PrivateRoot, no_links
import native_sqlite_terminal_receipt as sanitized

CASES = ("normal", "retry", "unwind", "terminal", "panic_terminal", "rollback", "wal", "stall")
TERMINAL = {"terminal", "panic_terminal", "rollback", "wal"}
STATUS = 86
FIXTURE = "tests/native_sqlite_terminal_fixtures.rs"
REVIEWED_FIXTURE_SHA256 = "f457d426fb87edfffa62afc6d79be854448380e9955ffddc728dd533486b9727"


class Refusal(Exception):
    """Only fixed classifications reach output."""


def prerequisites(env, system, machine, sha):
    if env.get("DPT_SYNTHETIC_SQLITE_TERMINAL_TESTS") != "1":
        raise Refusal("disabled")
    expected = env.get("EXPECTED_SHA", "")
    if len(expected) != 40 or any(c not in "0123456789abcdef" for c in expected) or expected != sha:
        raise Refusal("unapproved_source")
    profile = env.get("DPT_NATIVE_SQLITE_PROFILE", "github")
    if profile == "molly-local":
        if (system, machine) != ("Windows", "AMD64") or struct.calcsize("P") != 8:
            raise Refusal("unsupported_platform")
        if any(key.startswith("GITHUB_") for key in env):
            raise Refusal("local_github_environment_refused")
        if env.get("DPT_NATIVE_SQLITE_HOST") != "IMOLLY" or platform.node().casefold() != "imolly":
            raise Refusal("unapproved_host")
        if env.get("DPT_NATIVE_SQLITE_LOCAL_RISK") != "Molly-disposable-synthetic-terminal-only":
            raise Refusal("unsupported_privacy")
        if env.get("NATIVE_RUNNER") != "molly-local":
            raise Refusal("unsupported_platform")
    elif profile != "github":
        raise Refusal("unapproved_runner")
    runners = {("Windows", "AMD64"): "windows-2022", ("Darwin", "arm64"): "macos-15", ("Darwin", "x86_64"): "macos-15-intel"}
    if profile == "github" and (env.get("NATIVE_RUNNER") != runners.get((system, machine)) or (system, machine) not in runners):
        raise Refusal("unsupported_platform")
    if profile == "github" and (env.get("GITHUB_ACTIONS") != "true" or env.get("GITHUB_REF") != "refs/heads/main"):
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
    status, timed_out = None, None
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
                timed_out = subprocess.TimeoutExpired("synthetic_native_child", timeout)
                raise timed_out
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
        if timed_out is not None:
            timed_out.cleanup_receipt = dict(final_status=status, reaped=True,
                                            descendants_gone=True, handles_closed=True)
    return status


def mac_preflight():
    os.umask(0o077)
    # Disposable control scripts only; future authorized native run, never auto.
    script = ("import os,time,pathlib,signal; signal.alarm(5); root=pathlib.Path(os.environ['DPT_NATIVE_SQLITE_ROOT']); "
              "root.joinpath('created').write_bytes(b'created'); "
              "\nwhile not root.joinpath('go').exists(): time.sleep(.01)\n")
    for stall in (False, True):
        storage = PrivateRoot(prefix="dpt-native-control-")
        root = storage.path
        env = {"DPT_NATIVE_SQLITE_ROOT": str(root)}
        try:
            code = script + ("time.sleep(60)" if stall else "")
            try:
                status = mac_group([sys.executable, "-I", "-S", "-c", code], env, root, timeout=1)
            except subprocess.TimeoutExpired as error:
                expected = dict(final_status=-9, reaped=True, descendants_gone=True, handles_closed=True)
                if not stall or getattr(error, "cleanup_receipt", None) != expected or not (root / "created").is_file():
                    raise Refusal("unsupported_cleanup") from None
            else:
                if stall or status != 0 or not (root / "created").is_file():
                    raise Refusal("unsupported_cleanup")
        except Refusal:
            # Deliberately retain the fixture if process cleanup is uncertain.
            raise
        else:
            storage.verify()
            storage.close()
            shutil.rmtree(root)


def windows_preflight():
    import unittest
    import test_phase1_security_checks as guards
    names = {"test_breakaway_is_refused", "test_child_grandchild_timeout_releases_exclusive_handle", "test_normal_exit_cleans_descendant"}
    discovered = {name for name in unittest.defaultTestLoader.getTestCaseNames(guards.NativeContainmentTests)}
    if discovered != names:
        raise Refusal("unsupported_cleanup")
    class PrerequisiteResult(unittest.TestResult):
        def addSkip(self, test, reason):
            super().addSkip(test, reason)
            self.stop()

    result = PrerequisiteResult()
    result.failfast = True  # Failure/error (including subtests) stops the suite.
    unittest.defaultTestLoader.loadTestsFromTestCase(guards.NativeContainmentTests).run(result)
    if result.shouldStop or result.testsRun != 3 or not result.wasSuccessful() or result.skipped:
        raise Refusal("unsupported_cleanup")


def terminal_preflight(system):
    if system != "Windows":
        return mac_preflight()
    # Existing controls remain shared with Phase 1. Only this terminal profile
    # adds private storage and sanitizes their inherited environments/stdio.
    from unittest.mock import patch
    import test_phase1_security_checks as guards
    storage = PrivateRoot(prefix="dpt-native-control-")
    original = guards.checks.run_windows_job
    def isolated(args, env, timeout=600):
        clean = {key: value for key, value in env.items()
                 if key in ("SystemRoot", "WINDIR") or key.startswith("DPT_TEST_")}
        clean.update(TEMP=str(storage.path), TMP=str(storage.path))
        return original(args, clean, timeout=timeout, isolate_stdio=True)
    clean = {key: value for key, value in os.environ.items() if key in ("SystemRoot", "WINDIR")}
    with patch.dict(os.environ, clean, clear=True), patch.object(tempfile, "tempdir", str(storage.path)), patch.object(guards.checks, "run_windows_job", isolated):
        windows_preflight()
    storage.verify()
    storage.close()


def fixture_env(root, case, sha):
    # Absolute executable, no PATH, user profile, credential, loader or Rust env.
    env = {key: os.environ[key] for key in ("SystemRoot", "WINDIR") if key in os.environ}
    env.update(DPT_SYNTHETIC_SQLITE_TERMINAL_TESTS="1",
               DPT_SYNTHETIC_REPORT_RISK="disposable-synthetic-only",
               DPT_NATIVE_SQLITE_ROOT=str(root), DPT_NATIVE_SQLITE_CASE=case,
               EXPECTED_SHA=sha, NATIVE_RUNNER=os.environ["NATIVE_RUNNER"],
               TEMP=str(root), TMP=str(root), TMPDIR=str(root),
               DPT_NATIVE_SQLITE_PREFLIGHT="verified")
    profile = os.environ.get("DPT_NATIVE_SQLITE_PROFILE", "github")
    env["DPT_NATIVE_SQLITE_PROFILE"] = profile
    if profile == "molly-local":
        for key in ("DPT_NATIVE_SQLITE_HOST", "DPT_NATIVE_SQLITE_LOCAL_RISK"):
            env[key] = os.environ[key]
    else:
        # Copy independently checked real runner values. Never synthesize them.
        for key in ("GITHUB_ACTIONS", "GITHUB_REF"):
            env[key] = os.environ[key]
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
            return run_windows_job(args, env, timeout=3, isolate_stdio=True).returncode
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
    if case == "wal" and not (root / "wal_committed_only").is_file():
        raise Refusal("wal_witness_missing")
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
    sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True, timeout=5).strip()
    system = platform.system()
    prerequisites(os.environ, system, platform.machine(), sha)
    if subprocess.check_output(["git", "status", "--porcelain=v1"], text=True, timeout=5).strip():
        raise Refusal("unapproved_source")
    executable = approved_executable(sha)
    os.umask(0o077)
    receipt = new_sanitized_receipt(sha, executable)
    current, closing = None, False
    try:
        terminal_preflight(system)
        for case in CASES:
            current = receipt["cases"][case]
            current["state"] = "UNVERIFIED"
            if approved_executable(sha) != executable:
                raise Refusal("fixture_inventory_mismatch")
            storage = PrivateRoot()
            root = storage.path
            try:
                status = execute(executable, root, case, system, sha)
            except subprocess.TimeoutExpired as error:
                current["timed_out"] = True
                # Only a successful supervisor cleanup receipt permits filesystem
                # inspection. Other exceptions leave every unknown field null.
                cleanup = getattr(error, "cleanup_receipt", {})
                if cleanup == dict(final_status=(1 if system == "Windows" else -9),
                                   reaped=True, descendants_gone=True, handles_closed=True):
                    current["cleanup"].update(leader_reaped=True, namespace_empty=True, supervisor_handles_closed=True)
                    current.update(exit_code=cleanup["final_status"], timed_out=True)
                    current["phases"] = capture_phases(root)
                assess_timeout(root, case, error, system)
            else:
                current["cleanup"].update(leader_reaped=True, namespace_empty=True, supervisor_handles_closed=True)
                current["exit_code"] = status
                current["phases"] = capture_phases(root)  # Preserve BEFORE recovery clears markers.
                if case == "stall":
                    raise Refusal("stall_missing")
                assess(root, case, status)
            if system == "Windows":
                windows_handle_release(root)
                current["cleanup"]["exclusive_released"] = True
            if case in ("rollback", "wal"):
                recovery_material(root, case)
                storage.verify()
                recovery = current["recovery"]
                recovery["pre"] = recovery_hashes(root)
                snapshot = root / "pre-recovery"
                snapshot.mkdir(mode=0o700)
                for item in (root / "stage").iterdir():
                    if item.is_file():
                        shutil.copyfile(item, snapshot / item.name)
                storage.verify()
                if approved_executable(sha) != executable:
                    raise Refusal("fixture_inventory_mismatch")
                # A recovery failure must not inherit the primary child's cleanup.
                recovery["attempted"] = True
                try:
                    recovery["exit_code"] = execute(executable, root, "recover_" + case, system, sha)
                except subprocess.TimeoutExpired as error:
                    recovery["timed_out"] = True
                    cleanup = getattr(error, "cleanup_receipt", {})
                    if cleanup == dict(final_status=(1 if system == "Windows" else -9), reaped=True, descendants_gone=True, handles_closed=True):
                        recovery.update(exit_code=cleanup["final_status"], timed_out=True)
                        recovery["cleanup"].update(leader_reaped=True, namespace_empty=True, supervisor_handles_closed=True)
                    raise Refusal("recovery_unconfirmed") from None
                recovery["cleanup"].update(leader_reaped=True, namespace_empty=True, supervisor_handles_closed=True)
                recovery["privacy_ready"] = (root / "privacy_ready").is_file()
                recovery["confirmed"] = (root / "recovery_confirmed").is_file()
                assess(root, "recover_" + case, recovery["exit_code"])
                # Rust marks confirmation only after integrity/row assertions and close.
                recovery.update(integrity_ok=True, rows="BASELINE" if case == "rollback" else "WAL_COMMITTED")
                if system == "Windows":
                    windows_handle_release(root)
                    recovery["cleanup"]["exclusive_released"] = True
                recovery["post"] = recovery_hashes(root)
            storage.verify()
            if approved_executable(sha) != executable:
                raise Refusal("fixture_inventory_mismatch")
            current["source_verified"] = True
            closing = True
            storage.close()  # Failed handle closure is cleanup uncertainty.
            closing = False
            current["storage_closed"] = True
            current["state"] = "PASS"
            sanitized.validate(receipt)
            print(f"native.sqlite: case={case}; class=verified")
        receipt["final"].update(state="PASS", driver_exit=0, source_verified=True,
                                storage_closed=True, complete=True)
    except Exception:
        if current is not None:
            recovery = current["recovery"]
            known = not closing and all(current["cleanup"][key] is True for key in sanitized.CLEANUP[:3]) and (
                not recovery["attempted"] or all(recovery["cleanup"][key] is True for key in sanitized.CLEANUP[:3]))
            if system == "Windows":
                known = known and current["cleanup"]["exclusive_released"] is True and (
                    not recovery["attempted"] or recovery["cleanup"]["exclusive_released"] is True)
            current["state"] = "FAIL" if known else "UNVERIFIED"
        receipt["final"].update(state="UNVERIFIED" if current is None or current["state"] == "UNVERIFIED" else "FAIL",
                                driver_exit=1)
        # Metadata only. Never revisit an uncertain fixture, recover, delete,
        # close its guards, start another child, or serialize an exception.
        sanitized.publish(receipt)
        raise
    sanitized.publish(receipt)
    return 0


def capture_phases(root):
    return {name: (root / name).is_file() for name in sanitized.PHASES}


def recovery_hashes(root):
    result = {}
    for role, suffix in (("database", ""), ("journal", "-journal"), ("wal", "-wal"), ("shm", "-shm")):
        path = root / ("stage/synthetic.sqlite" + suffix)
        no_links(path.parent)
        if path.exists():
            no_links(path)
            if not path.is_file() or path.stat().st_nlink != 1:
                raise Refusal("recovery_material_missing")
            result[role] = dict(sha256=digest(path), size=path.stat().st_size)
    return result


def inventory_digest(inventory):
    return hashlib.sha256(json.dumps(inventory, sort_keys=True, separators=(",", ":")).encode("utf-8")).hexdigest()


def new_sanitized_receipt(sha, executable):
    data = json.loads(Path(os.environ["DPT_NATIVE_SQLITE_MANIFEST"]).read_text())
    profile = os.environ.get("DPT_NATIVE_SQLITE_PROFILE", "github")
    version = subprocess.check_output(["rustc", "--version"], text=True, timeout=5)
    match = re.fullmatch(r"rustc ([0-9]+\.[0-9]+\.[0-9]+) \([^\r\n]+\)\n?", version)
    if not match or match[1] != "1.98.1":
        raise Refusal("unsupported_toolchain")
    architecture = {("Windows", "AMD64"): "windows-x86_64", ("Darwin", "arm64"): "macos-arm64", ("Darwin", "x86_64"): "macos-x86_64"}[(platform.system(), platform.machine())]
    run = dict(profile=profile, run_id=os.environ["GITHUB_RUN_ID"] if profile == "github" else str(time.time_ns()),
               attempt=int(os.environ.get("GITHUB_RUN_ATTEMPT", "1")), S=sha,
               W=os.environ["EXPECTED_CONTROLLER_SHA"],
               L=os.environ.get("DPT_NATIVE_SQLITE_LAUNCHER_SHA") if profile == "github" else None,
               architecture=architecture, python=platform.python_version(), rust=match[1], retention_days=90)
    hashes = dict(fixture_source=data["source"][FIXTURE]["sha256"],
                  source_inventory=inventory_digest(data["source"]),
                  controller_inventory=inventory_digest(source_inventory(Path(__file__).resolve().parents[1])),
                  manifest=digest(Path(os.environ["DPT_NATIVE_SQLITE_MANIFEST"])), executable=digest(executable))
    return sanitized.new(run, hashes)


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


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def source_inventory(root=None):
    command = ["git"] if root is None else ["git", "-C", str(root)]
    records = subprocess.check_output(command + ["ls-files", "-s", "-z"], timeout=5).split(b"\0")
    result = {}
    for record in filter(None, records):
        metadata, name = record.split(b"\t", 1)
        mode, blob, stage = metadata.split()
        if mode not in (b"100644", b"100755") or stage != b"0":
            raise Refusal("unapproved_source")
        name = Path(os.fsdecode(name))
        path = name if root is None else root / name
        no_links(path)
        if not path.is_file() or path.stat().st_nlink != 1:
            raise Refusal("unapproved_source")
        content = path.read_bytes()
        git_blob = hashlib.sha1(b"blob " + str(len(content)).encode("ascii") + b"\0" + content).hexdigest()
        if git_blob != blob.decode("ascii"):
            raise Refusal("unapproved_source")
        result[name.as_posix()] = {"mode": mode.decode(), "sha256": hashlib.sha256(content).hexdigest()}
    if not result:
        raise Refusal("unapproved_source")
    return result


def verify_controller(root=None):
    root = Path(__file__).resolve().parents[1] if root is None else root
    expected = os.environ.get("EXPECTED_CONTROLLER_SHA", "")
    if len(expected) != 40 or any(c not in "0123456789abcdef" for c in expected) or expected == "0" * 40:
        raise Refusal("unapproved_controller")
    head = subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], timeout=5, text=True).strip()
    dirty = subprocess.check_output(["git", "-C", str(root), "status", "--porcelain=v1", "--untracked-files=all"], timeout=5)
    if head != expected or dirty.strip():
        raise Refusal("unapproved_controller")
    source_inventory(root)  # Also rejects assume-unchanged/filter-hidden edits.


def verify_source_fixture(root, sha, inventory=None):
    # S is explicitly supplied by the launcher/caller; __file__ belongs to W.
    head = subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True, timeout=5).strip()
    if head != sha:
        raise Refusal("unapproved_source")
    actual = source_inventory(root)
    if subprocess.check_output(["git", "-C", str(root), "status", "--porcelain=v1", "--untracked-files=all"], timeout=5).strip():
        raise Refusal("unapproved_source")
    if inventory is not None and inventory != actual:
        raise Refusal("unapproved_source")
    inventory = actual
    if inventory.get(FIXTURE, {}).get("sha256") != REVIEWED_FIXTURE_SHA256:
        raise Refusal("unreviewed_fixture")
    # Match the actual S file too, even when a manifest inventory is supplied.
    fixture = no_links(root / FIXTURE)
    if digest(fixture) != REVIEWED_FIXTURE_SHA256:
        raise Refusal("unreviewed_fixture")
    return fixture


def approved_executable(sha):
    manifest = os.environ.get("DPT_NATIVE_SQLITE_MANIFEST")
    if not manifest:
        raise Refusal("fixture_inventory_mismatch")
    target = no_links(Path.cwd() / "target").resolve(strict=True)
    manifest = no_links(Path(manifest)).resolve(strict=True)
    if not manifest.is_relative_to(target) or not manifest.is_file() or manifest.stat().st_nlink != 1:
        raise Refusal("fixture_inventory_mismatch")
    if digest(manifest) != os.environ.get("DPT_NATIVE_SQLITE_MANIFEST_SHA256"):
        raise Refusal("fixture_inventory_mismatch")
    verify_controller()
    data = json.loads(manifest.read_text())
    if data.get("sha") != sha or data.get("source") != source_inventory():
        raise Refusal("unapproved_source")
    if subprocess.check_output(["git", "status", "--porcelain=v1", "--untracked-files=all"], timeout=5).strip():
        raise Refusal("unapproved_source")
    verify_source_fixture(Path.cwd(), sha, data["source"])
    executable = no_links(Path(data.get("executable", ""))).resolve(strict=True)
    if (not executable.is_relative_to(target) or not executable.is_file()
            or executable.stat().st_nlink != 1
            or data.get("target") != "native_sqlite_terminal_fixtures"
            or data.get("executable_sha256") != digest(executable)):
        raise Refusal("fixture_inventory_mismatch")
    return executable


def evidence_hashes(root):
    result = {}
    for path in sorted(root.rglob("*")):
        no_links(path)
        if path.is_file() and path.name != "receipt.json":
            result[path.relative_to(root).as_posix()] = dict(size=path.stat().st_size, sha256=digest(path))
    return result


def write_receipt(root, receipt):
    # Only known synthetic filenames/digests/phase classifications, never bytes.
    path = root / "receipt.json"
    with path.open("w", encoding="utf-8") as stream:
        json.dump(receipt, stream, sort_keys=True, indent=2)
        stream.flush()
        os.fsync(stream.fileno())


def assess_timeout(root, case, error, system):
    receipt = getattr(error, "cleanup_receipt", {})
    expected = 1 if system == "Windows" else -9
    if (case != "stall" or receipt != dict(final_status=expected, reaped=True,
            descendants_gone=True, handles_closed=True)
            or not all((root / name).is_file() for name in ("privacy_ready", "handles_ready", "stall_ready"))
            or any((root / name).exists() for name in ("unexpected_close_failure", "guards_dropped", "staging_released", "completed", "terminal_ready"))):
        raise Refusal("fixture_timeout")


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Refusal as error:
        print("native.sqlite: class=" + str(error))
        sys.exit(1)
    except Exception:
        print("native.sqlite: class=unsupported_cleanup_or_fixture_error")
        sys.exit(1)
