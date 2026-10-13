# Disabled Windows/macOS synthetic terminal fixtures

This is an isolated integration-test target, not a production API or connection
migration. Only Windows/macOS compile its Rust body. Its sole libtest entry is
ignored by default. The existing Linux proof is unchanged. No existing workflow,
source pin, owner/trustee policy, ACL mask, diagnostic gate or publication path is
changed. The fixture descriptors demonstrate ownership, not production-equivalent
namespace validation. Unix directory descriptors do not confine SQLite pathname
opens. Windows volume-root owner rejection is neither tested around nor relaxed.

## Authorization and default-off controls

No native execution is approved by this implementation. A future independently
reviewed workflow change and a separately approved single run are required. The
test harness has no connection to the current workflow. Production activation and
acceptance of production privacy/data-loss risk remain separate decisions.

`python -B scripts/native_sqlite_terminal_checks.py` refuses by default. Future
execution additionally requires `--run` and ALL of:

- `DPT_SYNTHETIC_SQLITE_TERMINAL_TESTS=1`, exactly;
- `EXPECTED_SHA` equal to clean checkout HEAD, 40 lowercase hex characters;
- `GITHUB_ACTIONS=true` and `GITHUB_REF=refs/heads/main`;
- exact runner/architecture: windows-2022/AMD64, macos-15/arm64, or
  macos-15-intel/x86_64;
- `DPT_SYNTHETIC_REPORT_RISK=disposable-synthetic-only`, recording a separately
  approved disposable synthetic run where OS-controlled reports remain possible;
- both Windows root-diagnostic variables absent, including values of zero.

These environment records are runtime gates for trusted test code, not a security
boundary against a user deliberately forging them or proof of human approval.
The future main workflow must independently enforce the immutable reviewed SHA,
pre/post source-tip and checkout guards, and default-false manual opt-in. The
future workflow must compile only this target with --no-run and supervise build
processes independently; this driver never launches Cargo or rustc. It requires
DPT_NATIVE_SQLITE_MANIFEST naming a target-local JSON manifest with sha matching
EXPECTED_SHA, target=native_sqlite_terminal_fixtures, and executable naming the
native test executable extracted from Cargo JSON. Paths must resolve within the
checkout's target directory. The manifest is a trusted workflow record, not an
adversarial artifact attestation. The harness launches the executable directly;
reserved terminal status 86 must not be hidden by Cargo's test-failure status.


## Containment before execution

Windows runs all three existing native Job fixtures, requiring exact inventory,
three executions, no skips and no failure/error. Every new child then uses the
unchanged `run_windows_job`: atomic Job assignment, suspended creation, membership
verification before resume, non-inherited Job handle, no breakaway, bounded leader
wait and zero active processes before owned handle cleanup. This is independent
of root diagnostics. The suite verifies a separate exclusive fixture handle can
be reopened after every child, including timeout. Broker-created/hostile processes
are outside the trusted-launch scope.

macOS requires Python waitid/WNOWAIT and associated constants before spawning.
If unavailable, it reports unsupported_cleanup; there is no weaker fallback.
Each child starts in a new session and waits at a file barrier before database
operations. The supervisor verifies PID=PGID=SID and exclusion of its own group
before releasing the barrier. Exit observation uses WNOWAIT to reserve the leader
PID until group termination, preventing signaling a reused ID after reaping. The
supervisor sends SIGKILL only to the verified group, then confirms leader reaping
and group absence within two seconds. It never signals that ID again after reap.
Before-isolation failures kill only the owned, unreaped child. These trusted
fixtures do not fork, change groups or use brokers.

Two future macOS prerequisite controls verify normal creation/exit and deliberate
stall cleanup. Native Rust children have an independent ten-second alarm; control
scripts have a five-second alarm. The supervisor's per-child budget is three
seconds (one second for controls), followed by bounded cleanup. Unknown wait,
group, signal or reap outcomes stop the entire suite. Kernel I/O can prevent
termination despite userspace bounds; this is unsupported, never success.

## Privacy before database work

Windows children set and read back WER NOHEAP. This reduces collection, DOES NOT
disable LocalDumps and cannot exclude administrator/debugger/monitor collection.
macOS children set/read zero soft/hard RLIMIT_CORE; this does not suppress every
diagnostic report. The profile gate acknowledges only a future synthetic run's
residual risk, never production acceptance. Without it: unsupported_privacy.
There is no registry, system preference or crash-report-directory inspection or
modification. No reports, databases, fixtures or artifacts are uploaded.

The child environment is restricted to loader necessities and fixture controls;
root diagnostics and arbitrary inherited credentials are excluded. Children
discard output and suppress synthetic panic hooks. Output uses fixed case/class
labels only, never paths, identities, ACLs, rows or report contents.

## Future cases and evidence

Eight cases: normal checked close; real FFI SQLITE_BUSY retained-owner retry;
successful panic unwind; busy destructor terminal; busy destructor during unwind;
rollback recovery; separately labelled WAL recovery; supervisor stall.

Actual Connection::close returns the complete owner on failure. Owned synthetic
handles and staging stay present; the unfinalized FFI statement survives until
successful retry or process death. Terminal cases must observe real SQLITE_BUSY
before entering the destructor. Only that expected failure permits self
TerminateProcess (Windows) or _exit (macOS) with reserved status 86. Unexpected
close failures retain the owner and wait for supervisor cleanup; they cannot
unwind and release guards or fall back to abort. TerminateProcess returning is
also a failed/stalled fixture, never successful terminal evidence.

Success requires child privacy readiness, exact status and busy/terminal markers,
plus absent guard/staging-release/return markers. Unwind-terminal additionally
requires its panic phase. SIGABRT, watchdog kill, libtest failure, missing phases,
or unexpected close errors cannot pass. Staging-release markers mean ownership
ended; fixtures do not delete staging themselves. Parent teardown is allowed
only after confirmed whole-child cleanup and successful evidence checks.

Rollback uses DELETE/FULL, a committed baseline, uncommitted blob insert and real
db_cacheflush. Before reopening, the harness checks retained database, valid
recorded hot-journal header and database bytes changed from the baseline image.
WAL uses disabled autocheckpoint and a committed baseline followed by an
uncommitted insert; retained WAL must be present before a separate recovery child.
Both recovery children require integrity_check=ok, exactly the committed row,
successful explicit close and fresh recovery evidence. This is not power-loss
durability, arbitrary-VFS evidence, or the production recovery service.

Cleanup uncertainty prohibits recovery, later cases, automatic retry and teardown.
Failed fixtures deliberately retain their disposable files without printing paths.
No production cleanup or anchored Linux cleanup helper is changed.

## Validation boundary

Python tests are mocked only: no process creation, signals or SQLite termination.
They distinguish disabled/source/runner/privacy gates, missing phases, wrong
statuses, inaccessible groups, retained PID observation and cleanup refusal.
Linux offline Cargo checks exclude the Windows/macOS body and do not prove its
native compilation, FFI behavior, runner compatibility or privacy observations.
Native execution remains required and unauthorized here. For safe local Linux
proof validation, exclude busy_destructor, pending_write_abort and child_watchdog;
those four existing tests perform termination and must not run in this task.
