"""Run only synthetic security/recovery unit tests; never run the application.

Continue after a failed group so native platform gaps remain visible. A group
matching no tests is an error, not a successful security result. No fixture files
are uploaded and only cargo's normal unit-test output is printed.
"""
import ctypes
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import time

GROUPS = ("phase1", "backup_service::tests::", "database_recovery::",
          "payroll_archive_service::recovery::tests::")


class ContainmentError(Exception):
    """Fixed failure output only; never serialize commands, env or OS messages."""


def confirm_job_cleanup(leader, wait, active_processes):
    """Separate process signaling and job accounting within one cleanup budget."""
    deadline = time.monotonic() + 10
    if leader:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise ContainmentError("windows.job: class=cleanup_unconfirmed")
        remaining_ms = min(10000, max(1, int(remaining * 1000)))
        outcome = wait(leader, remaining_ms)
        if outcome == 0xffffffff:  # WAIT_FAILED; capture only its documented error.
            code = ctypes.get_last_error()
            raise ContainmentError(f"windows.job: class=leader_wait_failed; code={code}")
        if outcome == 258:  # WAIT_TIMEOUT never confirms termination.
            raise ContainmentError("windows.job: class=leader_wait_timeout; wait=258")
        if outcome != 0:  # Only WAIT_OBJECT_0 is valid for a process handle.
            raise ContainmentError(f"windows.job: class=leader_wait_unexpected; wait={outcome}")
    while True:
        active = active_processes()  # Query errors propagate; inaccessible is not empty.
        if time.monotonic() >= deadline:
            raise ContainmentError("windows.job: class=cleanup_unconfirmed")
        if active == 0:
            return
        time.sleep(0.02)


def run_windows_job(args, env, timeout=600, *, isolate_stdio=False):
    """Windows 10+ atomic job assignment; no uncontained launch fallback.

    Intended for trusted CreateProcess-based synthetic tests, not hostile code or
    broker-created processes. No job handle is inherited by Cargo/descendants.
    """
    if os.name != "nt":
        raise ContainmentError("windows.job: class=unsupported_platform")
    from ctypes import wintypes as w
    size = ctypes.c_size_t
    ptr = ctypes.c_void_p
    class BasicLimits(ctypes.Structure):
        _fields_ = [("process_time", ctypes.c_int64), ("job_time", ctypes.c_int64),
                    ("flags", w.DWORD), ("min_ws", size), ("max_ws", size),
                    ("active", w.DWORD), ("affinity", size),
                    ("priority", w.DWORD), ("scheduling", w.DWORD)]
    class IO(ctypes.Structure):
        _fields_ = [(name, ctypes.c_uint64) for name in
                    ("read_ops", "write_ops", "other_ops", "read_bytes", "write_bytes", "other_bytes")]
    class Limits(ctypes.Structure):
        _fields_ = [("basic", BasicLimits), ("io", IO), ("process_memory", size),
                    ("job_memory", size), ("peak_process", size), ("peak_job", size)]
    class Accounting(ctypes.Structure):
        _fields_ = [(name, ctypes.c_int64) for name in
                    ("user", "kernel", "period_user", "period_kernel")] + [
                    (name, w.DWORD) for name in ("faults", "total", "active", "terminated")]
    class Startup(ctypes.Structure):
        _fields_ = [("cb", w.DWORD), ("reserved", w.LPWSTR), ("desktop", w.LPWSTR),
                    ("title", w.LPWSTR)] + [(name, w.DWORD) for name in
                    ("x", "y", "xs", "ys", "xc", "yc", "fill", "flags")] + [
                    ("show", w.WORD), ("reserved_size", w.WORD), ("reserved_ptr", ptr),
                    ("stdin", w.HANDLE), ("stdout", w.HANDLE), ("stderr", w.HANDLE)]
    class StartupEx(ctypes.Structure):
        _fields_ = [("startup", Startup), ("attributes", ptr)]
    class Process(ctypes.Structure):
        _fields_ = [("process", w.HANDLE), ("thread", w.HANDLE),
                    ("pid", w.DWORD), ("tid", w.DWORD)]
    expected = (144, 112, 24) if ctypes.sizeof(ptr) == 8 else (112, 72, 16)
    if (ctypes.sizeof(Limits), ctypes.sizeof(StartupEx), ctypes.sizeof(Process)) != expected or ctypes.sizeof(Accounting) != 48:
        raise ContainmentError("windows.job: class=abi_layout_mismatch")
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    def api(name, result, types):
        fn = getattr(kernel, name)
        fn.restype, fn.argtypes = result, types
        return fn
    create_job = api("CreateJobObjectW", w.HANDLE, [ptr, w.LPCWSTR])
    set_info = api("SetInformationJobObject", w.BOOL, [w.HANDLE, ctypes.c_int, ptr, w.DWORD])
    query = api("QueryInformationJobObject", w.BOOL, [w.HANDLE, ctypes.c_int, ptr, w.DWORD, ptr])
    terminate = api("TerminateJobObject", w.BOOL, [w.HANDLE, w.UINT])
    close = api("CloseHandle", w.BOOL, [w.HANDLE])
    initialize = api("InitializeProcThreadAttributeList", w.BOOL, [ptr, w.DWORD, w.DWORD, ctypes.POINTER(size)])
    update = api("UpdateProcThreadAttribute", w.BOOL, [ptr, w.DWORD, size, ptr, size, ptr, ptr])
    delete = api("DeleteProcThreadAttributeList", None, [ptr])
    current = api("GetCurrentProcess", w.HANDLE, [])
    std = api("GetStdHandle", w.HANDLE, [w.DWORD])
    duplicate = api("DuplicateHandle", w.BOOL, [w.HANDLE, w.HANDLE, w.HANDLE, ctypes.POINTER(w.HANDLE), w.DWORD, w.BOOL, w.DWORD])
    create = api("CreateProcessW", w.BOOL, [w.LPCWSTR, w.LPWSTR, ptr, ptr, w.BOOL, w.DWORD, ptr, w.LPCWSTR, ptr, ptr])
    member = api("IsProcessInJob", w.BOOL, [w.HANDLE, w.HANDLE, ctypes.POINTER(w.BOOL)])
    resume = api("ResumeThread", w.DWORD, [w.HANDLE])
    wait = api("WaitForSingleObject", w.DWORD, [w.HANDLE, w.DWORD])
    exit_code = api("GetExitCodeProcess", w.BOOL, [w.HANDLE, ctypes.POINTER(w.DWORD)])
    def require(ok):
        if not ok:
            raise ContainmentError(f"windows.job: class=api_failure; code={ctypes.get_last_error()}")
    open_null = api("CreateFileW", w.HANDLE, [w.LPCWSTR, w.DWORD, w.DWORD, ptr, w.DWORD, w.DWORD, w.HANDLE])
    timed_out, final_status = None, None
    job = create_job(None, None)
    require(job)
    handles, attributes, initialized = [], None, False
    process = Process()
    try:
        limits = Limits()
        limits.basic.flags = 0x2000  # KILL_ON_JOB_CLOSE; neither breakaway flag.
        require(set_info(job, 9, ctypes.byref(limits), ctypes.sizeof(limits)))
        length = size()
        initialize(None, 2, 0, ctypes.byref(length))
        if ctypes.get_last_error() != 122 or not length.value:
            raise ContainmentError("windows.job: class=attribute_size_failure")
        attributes = ctypes.create_string_buffer(length.value)
        require(initialize(attributes, 2, 0, ctypes.byref(length)))
        initialized = True
        jobs = (w.HANDLE * 1)(job)
        require(update(attributes, 0, 0x2000D, jobs, ctypes.sizeof(jobs), None, None))
        for selector in [-10, -11, -12]:
            handle = w.HANDLE()
            source = std(selector & 0xffffffff) if not isolate_stdio else open_null(
                "NUL", 0x80000000 if selector == -10 else 0x40000000, 3, None, 3, 0x80, None)
            require(source and source != ptr(-1).value)
            try:
                require(duplicate(current(), source, current(), ctypes.byref(handle), 0, True, 2))
                # Register before closing source, so failure still closes duplicate.
                handles.append(handle.value)
            finally:
                if isolate_stdio:
                    require(close(source))
        inherited = (w.HANDLE * 3)(*handles)
        require(update(attributes, 0, 0x20002, inherited, ctypes.sizeof(inherited), None, None))
        startup = StartupEx()
        startup.startup.cb = ctypes.sizeof(startup)
        startup.startup.flags = 0x100  # STARTF_USESTDHANDLES
        startup.startup.stdin, startup.startup.stdout, startup.startup.stderr = handles
        startup.attributes = ctypes.cast(attributes, ptr)
        executable = shutil.which(args[0])
        if executable is None:
            raise ContainmentError("windows.job: class=executable_unavailable")
        command = ctypes.create_unicode_buffer(subprocess.list2cmdline([executable] + args[1:]))
        environment = ctypes.create_unicode_buffer("\0".join(
            f"{key}={value}" for key, value in sorted(env.items(), key=lambda item: item[0].upper())) + "\0\0")
        # JOB_LIST assignment is atomic at creation; suspended for membership verification.
        require(create(executable, command, None, None, True, 0x80000 | 0x400 | 4,
                       environment, None, ctypes.byref(startup), ctypes.byref(process)))
        inside = w.BOOL()
        require(member(process.process, job, ctypes.byref(inside)))
        if not inside.value:
            raise ContainmentError("windows.job: class=membership_failure")
        if resume(process.thread) != 1:
            raise ContainmentError("windows.job: class=resume_failure")
        outcome = wait(process.process, int(timeout * 1000))
        if outcome == 258:
            # Exception escapes only after finally confirms whole-job cleanup.
            timed_out = subprocess.TimeoutExpired("controlled_synthetic_group", timeout)
            raise timed_out
        if outcome != 0:
            raise ContainmentError("windows.job: class=wait_failure")
        code = w.DWORD()
        require(exit_code(process.process, ctypes.byref(code)))
        return subprocess.CompletedProcess(args, code.value)
    finally:
        # Also remove descendants after ordinary Cargo exit or any setup/API failure.
        # Any cleanup failure escapes and stops the harness, never starts another group.
        try:
            require(terminate(job, 1))
            def active_processes():
                accounting = Accounting()
                require(query(job, 1, ctypes.byref(accounting), ctypes.sizeof(accounting), None))
                return accounting.active
            confirm_job_cleanup(process.process, wait, active_processes)
            if process.process:
                code = w.DWORD()
                require(exit_code(process.process, ctypes.byref(code)))
                final_status = code.value
        finally:
            if initialized:
                delete(attributes)
            # Close every owned handle even if another CloseHandle fails.
            failed = False
            for handle in [process.thread, process.process, *handles, job]:
                if handle and not close(handle):
                    failed = True
            if failed:
                raise ContainmentError("windows.job: class=handle_close_failure")
        if timed_out is not None:
            timed_out.cleanup_receipt = dict(final_status=final_status, reaped=True,
                                            descendants_gone=True, handles_closed=True)


def main():
    root = Path(__file__).resolve().parents[1]
    os.chdir(root)
    native = platform.system()
    if native not in ("Windows", "Darwin"):
        print("This runner requires native Windows or macOS; Linux checks are separate.")
        return 2
    sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    if sha != os.environ.get("EXPECTED_SHA"):
        print("Refusing an unreviewed revision.")
        return 2
    print(json.dumps({"sha": sha, "platform": native,
                      "machine": platform.machine()}, sort_keys=True), flush=True)
    for tool in ("rustc", "cargo"):
        subprocess.run([tool, "--version"], check=True)
    if native == "Darwin":
        subprocess.run(["xcrun", "--show-sdk-path"], check=True)
        subprocess.run(["/usr/bin/clang", "--version"], check=True)
    # The Windows-specific fixtures themselves require NTFS and fail otherwise.
    cargo = ["cargo", "+1.98.1"]
    check = subprocess.run(cargo + ["check", "--locked", "--bins", "--tests"])
    if check.returncode:
        return check.returncode
    failures = []
    native_marker = ("phase1_windows_actual_new_object_owners_match_current_token"
                     if native == "Windows" else
                     "phase1_macos_assessment_detects_comma_separated_inherited_allow")
    for group in GROUPS:
        listing = subprocess.run(cargo + ["test", "--locked", group, "--", "--list"],
                                 capture_output=True, text=True)
        count = sum(line.endswith(": test") for line in listing.stdout.splitlines())
        print(f"Security group {group}: {count} discovered tests", flush=True)
        if (listing.returncode or count == 0 or
                (group == "phase1" and native_marker not in listing.stdout)):
            print(listing.stdout)
            print(listing.stderr, file=sys.stderr)
            failures.append(group + " (discovery/native marker/compilation)")
            continue
        # Never propagate a raw diagnostic setting to arbitrary test groups.
        test_env = os.environ.copy()
        test_env.pop("DPT_WINDOWS_ROOT_DIAGNOSTICS", None)
        diagnostic = (native == "Windows" and group == "phase1" and
                      os.environ.get("DPT_WINDOWS_SYNTHETIC_ROOT_DIAGNOSTICS") == "1")
        if diagnostic:
            test_env["DPT_WINDOWS_ROOT_DIAGNOSTICS"] = "1"
        args = cargo + ["test", "--locked", group, "--", "--test-threads=1"]
        if diagnostic:
            args.append("--nocapture")
        try:
            result = (run_windows_job(args, test_env) if diagnostic else
                      subprocess.run(args, env=test_env))
        except ContainmentError as error:
            print(str(error), flush=True)
            return 1
        except subprocess.TimeoutExpired:
            print("Contained security group timed out; whole-job cleanup confirmed.", flush=True)
            failures.append(group + " (timeout)")
            continue
        if result.returncode:
            failures.append(group)
    print("Failed groups: " + (", ".join(failures) if failures else "none"), flush=True)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
