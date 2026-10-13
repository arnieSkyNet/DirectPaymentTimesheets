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

Prerequisite dispatch now stops at the first failure, error or skip, including
unconfirmed cleanup. No later control starts after that result. A stopped or
incomplete suite cannot pass; only all three successful controls permit progress.
Test-owned teardown still runs as unittest requires; this change does not prove
native cleanup or change the Job helper. Mock-only regressions exercise actual
unittest dispatch with synthetic control bodies failing in each position.

macOS requires Python's supported select.kqueue/kevent process filter, NOTE_EXIT
and NOTE_FORK constants. Python 3.12 os.waitid is not available on macOS and is
no longer used. Missing APIs report unsupported_cleanup before spawning. Each
child starts in a new session and waits at a barrier before database operations.
The supervisor registers exit/fork observation and verifies PID=PGID=SID and
exclusion of its own group before releasing the barrier. Initial events are
validated too, including an exit before cleanup. NOTE_EXIT observes departure;
it does not reap or supply a status this code interprets as wait status.

This is a SINGLE direct-child supervisor, not general descendant containment.
The reviewed fixture/control children do not fork or use brokers. Other processes
cannot ordinarily join a different session's group. Unexpected NOTE_FORK, event
errors, foreign PID/filter or unrecognized events refuse accounting, even if a
subsequent group signal succeeds. No descendant termination/reaping is claimed.
Do not use it for Cargo or arbitrary descendant-spawning programs.

The parent must exclusively own waiting for this child: no competing waiter,
SIGCHLD handler, SIGCHLD ignore or external native auto-reaper. A nondefault
Python SIGCHLD disposition is rejected before spawn; this is a trusted plain
Python harness, not protection against hidden C code altering signal flags.
An unreaped direct child retains its PID. The supervisor signals only its
verified private group while that identity is reserved, observes exit with bounded
kqueue control calls, then uses bounded waitpid(WNOHANG) to reap that sole member.
It never uses Popen.poll's ECHILD-to-success behavior. ECHILD, surfaced EINTR,
unknown statuses, missing exit observation, signal errors and deadline exhaustion
refuse cleanup. Python may restart an interrupted kevent internally using its
recomputed finite timeout; every supplied timeout is finite and the outer deadline
is monotonic.

After reap, no further signal is sent to the old PID/group ID. Group presence at
that point is conservatively unsupported_cleanup (including possible ID reuse),
never evidence authorizing another kill. Registration/isolation failures before
the barrier signal only the owned unreaped PID, then attempt bounded reaping;
they never signal an unverified group and never permit fixture success. Queue
handles are closed on normal and failure paths; close failure also refuses cleanup.

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
statuses, inaccessible groups, non-reaping kqueue observation, retained PID identity and cleanup refusal.
Linux offline Cargo checks exclude the Windows/macOS body and do not prove its
native compilation, FFI behavior, runner compatibility or privacy observations.
Native execution remains required and unauthorized here. For safe local Linux
proof validation, exclude busy_destructor, pending_write_abort and child_watchdog;
those four existing tests perform termination and must not run in this task.

The mocked supervisor cases cover registration, initial exit, unexpected fork,
wrong/error events, interrupted observation, observation timeout, normal leader
exit, stall termination-before-reap, signal failures, ECHILD/interrupted reap,
deadline exhaustion, post-reap survivors and barrier isolation. These establish
control-flow expectations only; no kqueue, process, signal or native fixture runs
in the mocked suite. Native macOS Intel/Apple Silicon compilation and the two
separately authorized containment controls remain required. No unsupported
accounting condition is converted to a successful observation or cleanup result.

API references: [Apple kqueue(2)](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/kqueue.2.html)
distinguishes NOTE_EXIT from NOTE_REAP and documents aggregation and registration
checks; [Python 3.12 select](https://docs.python.org/3.12/library/select.html#kqueue-objects)
provides the supported wrapper. This correction does not authorize native testing
or change the default-off terminal/privacy gates.

## Proposed compilation and safe containment stage (not wired to a workflow)

The declarative [safe build manifest](native_sqlite_safe_build_manifest.json)
records exact commands and inventory. It is a recipe, not a claim of bit-identical
compiler output or an executable/artifact attestation. Dependencies are pinned by
Cargo.lock; the future runners use Python 3.12 and Rust 1.98.1, matching the current
native workflow. Native compilation remains unverified on all three runners.

The new `--containment-only` command is independent of `--run`. It cannot load an
executable manifest, call a SQLite child, construct a fixture environment or run
recovery. Terminal flags and case selections in its environment do not change
that control flow. It requires exactly `DPT_NATIVE_SQLITE_SAFE_CONTAINMENT=1`,
the existing clean SHA/main/runner/profile checks and absent root-diagnostic
variables. The profile records disposable control data only; it does not approve
terminal execution or production privacy risk. Default invocation remains disabled.
Windows requires the unchanged three Job tests with no skips; macOS runs only the
two existing static Python no-descendant controls. No SQLite database is opened.
Success prints `class=containment_verified; terminal_executed=false`. Any refusal
stops the stage, including unsupported_cleanup or unsupported_privacy. No retry.

Exact proposed main-workflow changes, for a separately approved PR:

1. Replace its seven `d058c5c847215f81235acb6d23728937df8437b9` source references
   with the final independently reviewed security commit containing this stage.
   Do not use the recipe's starting commit or a moving branch as its checkout.
   Preserve main-only dispatch, required expected_sha, pre/post remote-tip and
   checkout-HEAD guards, existing immutable actions and read-only permissions.
2. Add `controlled_sqlite_terminal_tests`, boolean, default false, and a separate
   `compile_sqlite_containment_only`, boolean, default false. For this stage reject
   terminal=true and root-diagnostics=true before checkout/build. There must be no
   workflow step selecting `--run`, even if another input or environment enables it.
   Add `if: ${{ !inputs.compile_sqlite_containment_only }}` to the existing guard
   step and include that condition in the existing Phase 1 harness step. The new
   safe steps use `if: ${{ inputs.compile_sqlite_containment_only && success() }}`.
   This prevents duplicate native controls and excludes unrelated test execution
   from the compile-only stage. The driver alone enforces its three Windows
   prerequisites; no skip/failure can produce its success record.
3. After source verification, compile only the manifest's target with the exact
   `cargo test --locked --test native_sqlite_terminal_fixtures --no-run
   --message-format=json` arguments, with a 30-minute step timeout. Capture JSON
   locally in target; accept exactly one compiler-artifact with target name above,
   kind=[test], profile.test=true and a non-null executable resolving under target.
   A missing/duplicate/foreign artifact fails. No package/release build or upload.
4. Invoke that executable with only `--list --ignored`, with a ten-second deadline,
   locally capture output and require its nonempty stripped lines to equal the
   manifest inventory exactly. This libtest listing does not invoke test bodies;
   do not append arbitrary flags or run `--ignored` without `--list`. Any timeout
   or uncertain child cleanup stops the job, with no subsequent controls or retry.
5. Run the manifest containment command with a two-minute outer step timeout,
   EXPECTED_SHA and exact NATIVE_RUNNER, GITHUB_ACTIONS=true, main ref, synthetic
   profile and safe opt-in. Keep both root variables and terminal fixture/profile
   variables unset. The driver's independent mode remains safe even if terminal
   variables are injected. Require a successful step before recording verification.

The macOS supervisor is deliberately unsuitable for Cargo's descendant tree. The
future workflow must treat a compiler or inventory deadline as a failed job and
delegate runner teardown to Actions; an Actions timeout is not proof of compiler
descendant reaping. No later fixture may run after it. The single-child kqueue
controls have their own deadlines and confirmed owned-child reap; all accounting
uncertainty refuses success. A process exiting before kqueue registration is not
assumed to generate an initial event: a missed event fails closed. The start
barrier keeps the normal/stall controls alive until registration and isolation.
Kernel-blocked tasks, competing reapers, unexpected forks or missing APIs remain
unsupported. Native normal/stall evidence on both macOS runners is still required;
mock tests prove only the selected control flow. These trusted controls can test
kqueue, leader identity, signaling and reaping without loading SQLite terminal
code. They do not establish arbitrary descendant containment or dump suppression.

The actual workflow and its older source pin remain unchanged. No native command,
emergency fixture, workflow dispatch or OS setting change is authorized here.
