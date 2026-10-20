# Disabled Windows/macOS synthetic terminal fixtures

This is an isolated integration-test target, not a production API or connection
migration. Only Windows/macOS compile its Rust body. Its sole libtest entry is
ignored by default. The existing Linux proof is unchanged. Owner/trustee policy, ACL masks, diagnostic gates and publication paths remain
unchanged. The test-only workflow preparation is described below. The fixture descriptors demonstrate ownership, not production-equivalent
namespace validation. Unix directory descriptors do not confine SQLite pathname
opens. Windows volume-root owner rejection is neither tested around nor relaxed.

## Authorization and default-off controls

No native execution is approved by this implementation. A separately approved
single run and assignment of published reviewed pins are required. The separate
`sqlite-terminal-native.yml` preparation is default-disabled and unconfigured.
The existing Phase 1 workflow continues to refuse terminal execution. Production activation and
acceptance of production privacy/data-loss risk remain separate decisions.

`python -B scripts/native_sqlite_terminal_checks.py` refuses by default. Future
execution additionally requires `--run` and ALL of:

- `DPT_SYNTHETIC_SQLITE_TERMINAL_TESTS=1`, exactly;
- `EXPECTED_SHA` equal to clean checkout HEAD, 40 lowercase hex characters;
- profile `github`: actual `GITHUB_ACTIONS=true`, `GITHUB_REF=refs/heads/main`,
  and windows-2022/AMD64, macos-15/arm64, or macos-15-intel/x86_64;
- alternatively profile `molly-local`: real Windows AMD64, 64-bit Python,
  actual host name Molly, explicit local host/risk gates and **no** `GITHUB_*`
  variables (details below);
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
EXPECTED_SHA, the complete tracked source inventory (hashes/modes),
target=native_sqlite_terminal_fixtures, and the executable path/SHA-256 extracted
from Cargo JSON. DPT_NATIVE_SQLITE_MANIFEST_SHA256 binds the manifest itself.
EXPECTED_CONTROLLER_SHA independently binds the clean controller checkout. Paths must resolve within the
checkout's target directory. The manifest is a trusted workflow record, not an
adversarial artifact attestation. The harness launches the executable directly;
reserved terminal status 86 must not be hidden by Cargo's test-failure status.


## Containment before execution

Windows runs all three existing native Job fixtures, requiring exact inventory,
three executions, no skips and no failure/error. Every new child then uses the
shared `run_windows_job`, with terminal-only NUL stdio isolation: atomic Job assignment, suspended creation, membership
verification before resume, non-inherited Job handle, no breakaway, bounded leader
wait and zero active processes before owned handle cleanup. This is independent
of root diagnostics. The suite verifies a separate exclusive fixture handle can
be reopened after every child, including timeout. Broker-created/hostile processes
are outside the trusted-launch scope.

Prerequisite dispatch now stops at the first failure, error or skip, including
unconfirmed cleanup. No later control starts after that result. A stopped or
incomplete suite cannot pass; only all three successful controls permit progress.
Test-owned teardown still runs as unittest requires; this change does not prove
native cleanup. The helper now optionally isolates stdio and supplies a final
timeout cleanup receipt; Phase 1's default stdio behaviour is preserved. Mock-only regressions exercise actual
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
WAL disables autocheckpoint, commits/checkpoints row 1, commits row 2 only to
WAL, then leaves row 3 uncommitted. A read-only main-file-only copy must contain
exactly [1] before the pending transaction. The recovery child requires exactly
[1,2]; rollback recovery requires [1]. Both require integrity_check=ok, successful
explicit close and fresh recovery evidence. Raw pre-recovery database/sidecar
copies and pre/post hashes remain private beside the retained originals. This is not power-loss
durability, arbitrary-VFS evidence, or the production recovery service.

Cleanup uncertainty prohibits recovery, later cases, automatic retry and teardown.
All terminal-suite fixtures retain their disposable files without printing paths,
including successes. A case-local receipt records source/binary hashes, phases,
statuses and pre/post recovery hashes. A failure never permits automatic deletion.
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

The active main workflow and its older source pin remain unchanged. No native command,
emergency fixture, workflow dispatch or OS setting change is authorized here.

## Prepared workflow and accepted compiler-timeout boundary

The security-branch workflow now contains the proposed main-only preparation.
Its immutable source pin stays d4057855c8302aaec8a8d1b6cbdcea0256553e30.
Committing this preparation necessarily advances the remote security tip: the
remote-tip guard will refuse dispatch until a separately reviewed pin update.
Main is not changed and no dispatch is authorized by this preparation.

Ordinary Actions job/step timeouts are explicitly accepted for Cargo compilation
without proof of compiler-descendant reaping. Build failure/timeout blocks every
later verification step. Inventory uses only --list --ignored with a ten-second
child deadline; it never invokes the ignored SQLite test. Strict cleanup remains
mandatory for the three Windows Job controls and the two macOS single-child
controls. Their unsupported/error/skip outcomes cannot pass or permit later tests.
The outer containment step deadline is a final failure boundary, not evidence of
successful cleanup. Compiler cancellation and unsupported security cleanup must
never be reported as verification.

The compile-only input defaults false and rejects terminal/root diagnostic inputs.
Terminal execution is refused by the workflow in every mode. Safe controls receive
an allowlisted environment and hardcoded containment-only arguments. The normal
Phase 1 mode remains separate, with an immediate-stop prerequisite runner and only
the three exact documented macOS Windows-native skips accepted.


## Stage 3 test-only corrections (2026-10-20)

This section governs the corrected driver and new separate workflow; the older
Phase 1 compile-only preparation above remains historical and unchanged.
No production SQLite path, owning wrapper or emergency policy is enabled.

### Storage and evidence

`native_sqlite_fixture_storage.py` creates fresh unpredictable fixture roots.
Windows verifies native AMD64 without WOW64/emulation using IsWow64Process2,
refuses an elevated token, and supplies an explicit protected, inheritable DACL at CreateDirectoryW:
current token + SYSTEM only. Owner, non-null DACL, protected root and every ACE
are inspected; unfamiliar ACE types/trustees refuse. Reparse components,
symlinks, nonregular objects and multiply-linked files refuse. Handles retain
root and all ancestors without DELETE sharing, through all child/recovery work.
Existing permissions are never repaired. macOS checks current UID, exact 0700
for directories / 0600 for files, device/inode identity and empty extended ACLs;
unknown ACL results refuse. ABI constants and empty-ACL iteration were checked
against Apple's [sys/acl.h](https://github.com/apple-oss-distributions/Libc/blob/main/include/sys/acl.h)
and [acl_entry.c](https://github.com/apple-oss-distributions/Libc/blob/main/posix1e/acl_entry.c). Ancestors require root/current ownership, no unsafe
write modes (trusted sticky roots allowed) and empty ACLs. Creating an empty root
with inherited ACLs may be refused before any child/data exists; no ACL stripping
or machine policy change is attempted. Root descriptors stay held until review
checks complete. These are trusted synthetic fixture admission checks, **not**
production VFS confinement or resistance to another process of the same user.
Unix ancestor replacement and SQLite's internal pathname opens remain limitations.

Before terminal execution the parent uses umask 077. Child environments keep only
SystemRoot/WINDIR where present plus explicit profile/case/source/barrier fields
and fixture-private temp paths; no PATH, HOME, SMTP values, tokens, loader
injection or Rust overrides. GitHub identity values are copied from checked
runner values, never manufactured. Molly gets no GitHub identity at all.
Windows terminal-profile supervisor controls also receive private temp storage,
allowlisted environments and NUL handles; existing Phase 1 callers retain defaults.

Every case is checked against the complete source/controller and executable
inventory before and after running, and again before a recovery child. The
manifest maker accepts only the one Cargo test artifact from a completed
`--no-run` JSON build. The manifest must be produced and reviewed after the build
in a clean checkout; it is a trusted build receipt, not proof against a malicious
compiler/same-user writer. Clean Git status plus comparison with each committed Git blob, full source hashes,
and manifest/executable hashes detect subsequent changes, including hidden
assume-unchanged edits. Checkout must preserve committed bytes (no CRLF rewriting). Build cancellation still uses
ordinary Actions timeouts; it is never evidence of strict compiler reaping.

Stall requires privacy_ready, handles_ready and stall_ready, absence of terminal,
close-failure, completion and guard-release markers, plus a final cleanup receipt:
Windows exit 1 / macOS -9, leader reaped, no remaining owned group/job processes,
and supervisor handles closed. No receipt is attached until cleanup succeeds;
any API, wait, handle-close or timeout-cleanup failure stops all later work.
Windows exclusive handles must reopen after each child, recovery and stall.
macOS remains a single trusted nonforking-child supervisor: the receipt's
`descendants_gone` means verified absence of the owned process group and no fork
observed, not general descendant containment.

Roots `dpt-native-sqlite-*` and their receipts stay private for local review.
`pre-recovery/` preserves synthetic database/sidecar bytes before recovery changes
originals; receipts retain SHA-256 and sizes before and after. No raw output,
artifact upload or raw evidence publication is configured. An Actions runner's
private disk is ephemeral: this does **not** promise durable raw preservation
beyond the job. Agree a sanitized durable receipt policy before a hosted run;
raw retention/export would require separate authorization.

### Local Molly profile (not executed)

Use an isolated clean checkout containing the separately published reviewed
source/controller revisions, not Florence's dirty working tree. On Molly,
independently confirm the real computer name, x64 Python and the reviewed hashes.
Required records, in addition to the common SHA/manifest/synthetic-risk gates:

- DPT_NATIVE_SQLITE_PROFILE=molly-local
- NATIVE_RUNNER=molly-local
- DPT_NATIVE_SQLITE_HOST=Molly
- DPT_NATIVE_SQLITE_LOCAL_RISK=Molly-disposable-synthetic-terminal-only
- EXPECTED_CONTROLLER_SHA=<reviewed published controller SHA>
- DPT_NATIVE_SQLITE_MANIFEST_SHA256=<independently retained manifest digest>

No GITHUB_* variable may exist. The Python supervisor checks platform.node and
pointer width; the Rust child independently calls GetComputerNameW and requires
Windows x86_64 and native AMD64 using IsWow64Process2. Do not set synthetic GitHub variables or relabel Florence/Molly as
windows-2022. Source acquisition is by exact approved commit, with clean Git
status and hashes, never an unpinned branch tip. Build with the existing pinned
Rust toolchain using only --no-run, inspect the one ignored-test inventory,
produce the manifest using native_sqlite_terminal_manifest.py, retain its digest
separately, then use the driver only after separate execution approval. No
privilege elevation, service/VM change, report-setting change or retry is allowed.

### Separate macOS workflow and decisions

`sqlite-terminal-native.yml` is workflow_dispatch only, main-only, default false,
contents:read, macos-15 ARM64 / macos-15-intel x86_64. Checkout and setup-python
use immutable action SHAs. Controller and source use separate immutable checkout
pins, credential persistence disabled; both pins are zero sentinels and refused
before checkout. They cannot be assigned honestly while this work is uncommitted.
Future approval must name published source S and controller W; update both env
constants and corresponding checkout refs together in a separately reviewed
change. A controller commit may precede the workflow commit, avoiding a
self-referential commit pin. The controller is verified again per case.

Execution order: immutable checkouts; build --no-run (30 min); inventory only
(10 s child); supervisor controls (2 min outer); eight cases (5 min outer).
Each fixture child has 3 s, barrier 2 s; Windows cleanup 10 s, macOS cleanup 2 s;
macOS child alarm 10 s. Normal/retry/unwind exit 0; terminal/panic/rollback/WAL
exit 86; recovery exit 0; stall must have the exact supervisor-kill status above.
There are eight acceptance cases and two additional recovery invocations.

Before native execution the required decisions are:

1. Which published immutable source S and controller W are independently approved?
   Evidence: current changes have no commit pin; zero workflow sentinels refuse.
2. Is one disposable synthetic run on actual Molly and each macOS architecture
   authorized with the documented residual reporting risk? WerSetFlags(NOHEAP)
   does not suppress LocalDumps/collectors; RLIMIT_CORE=0 does not suppress every
   macOS report. No OS reporting settings will be inspected or changed.
3. What sanitized durable receipt policy is approved for ephemeral hosted runners?
   Private raw evidence stays local and no raw upload is authorized.

These decisions authorize neither production emergency termination nor acceptance
of real payroll/credential privacy risk. Native passes would prove only this
terminal substrate and fixture, not the full Stage 2 owning-connection prototype:
registration/open cleanup, backup destination ownership, private staging lifetimes,
Unix namespace races, panic inside close/destructor, and production shutdown
integration remain conditional/unresolved. Microsoft Outlook.com, Hotmail.com
and Live.com OAuth2 remains mandatory for v1.0.7 and outside this SQLite stage.


## Sanitized receipts and split publication preparation (2026-10-20)

The controller now emits `dpt-native-sqlite-receipt/v1` through
`scripts/native_sqlite_terminal_receipt.py`. This is a positive schema, not a
redaction filter: exact keys, enumerated states/cases/roles, booleans/nulls,
integer exit codes, numeric tool versions/IDs and fixed-length hexadecimal hashes.
Unknown fields, duplicate JSON keys, paths, usernames, free-form error strings,
raw logs and database contents are not accepted fields. The exporter never reads
raw fixture receipts, recursively exports a directory or serializes exceptions.
Only database/journal/WAL/SHM hashes and sizes are projected from known paths.
A cryptographic-looking string is not independently authenticated evidence;
provenance comes from the pinned controller and external run/receipt verification.

Each of the eight primary cases retains its original phases BEFORE recovery
clears markers. Recovery has its own actual exit, timeout flag, cleanup, fresh
privacy/confirmation, integrity/row-set outcome and pre/post hashes. The original
synthetic pre-recovery files still remain privately retained. An uncertain primary
or recovery cleanup is UNVERIFIED, not a successful timeout; no fixture inspection,
recovery, guard release, deletion or later case follows uncertainty. Export then
uses only already recorded in-memory metadata in a separate private metadata root.

PASS is assigned only after final source/controller/executable admission and
successful fixture-storage handle closure. An exporter metadata-root close failure
also prevents output. The strict validator refuses missing cases, incomplete final
checks, wrong exits, absent witnesses, or unconfirmed cleanup being called PASS.
The local copy is canonical JSON, mode 0600 on Unix, beneath a validated private
root (Windows uses the private inherited DACL). A separate SHA-256 sidecar is
created, data/checksum are flushed, and neither is automatically deleted. The
receipt declares a minimum retention policy of 90 days. This metadata and file
format are suitable for long-term local retention; temporary directories and
hosted disks cannot guarantee that period. Preserve an approved private copy of
only these sanitized files outside ephemeral storage for at least 90 days.

The driver prepares one local receipt/checksum and emits canonical JSON to the job
log plus a GitHub job summary. This repository is public: future execution needs
explicit approval for publication of these sanitized fields. There is no raw
artifact upload, repository write permission, secret input or production action.
The workflow records actual `github.workflow_sha` as L, separately from source S
and controller W. Summary delivery/readback and actual process/job exit remain
independent acceptance checks; a PASS payload alone is insufficient, including
when export/output fails or a runner is cancelled. Missing/truncated receipts are
UNVERIFIED and do not authorize a retry. Summary upload success is not proved by
writing the runner's summary control file.

Offline verification after an authorized run uses the retained JSON plus sidecar:
`python -I -B scripts/native_sqlite_terminal_receipt.py --verify <receipt.json>`.
This verifies schema, canonical bytes and checksum only, not authenticity.
`--retain <input.json> <new-output.json>` verifies the input/sidecar, then writes
only canonical metadata and a checksum into an existing approved private directory
with no overwrite or permission repair. Use this to retain the sanitized files in
persistent private storage for >=90 days; it never copies raw fixture directories.
Compare with the independently read-back canonical JSON/digest and approved S/W/L, actual
run ID/attempt, architecture and job/driver exits. Never copy raw fixture roots,
crash reports, build logs or environment dumps alongside the retained receipt.

The S checkout MUST contain fixture SHA-256
`a2fe71e95c11e712f5c58459974e91869b44e1d9b832815a6d60f3a7d18168bc`.
The manifest producer and driver verify the clean committed S inventory and this
fixture independently of controller-local files. Old HEAD's fixture is refused.
W is verified from the controller script's own checkout; no Rust fixture needs to
be copied into main (main lacks its Unix libc dependency). The configured launcher
L is a later commit that checks out literal S/W; W cannot pin its own commit.
The workflow remains default-disabled with zero S/W sentinels until separately
reviewed publication. Workflow tests now accept either both zero pins or both
configured pins and require matching source/controller checkout refs. Split-layout
regressions use synthetic files with mocked immutable Git metadata; they prove
admission/layout control flow, not remote acquisition or native execution.

This preparation changes only the controller/receipt/test contract. Stage 1,
Stage 2, the native Rust fixture, application inputs, Phase 1 workflow and unrelated
user edits remain preserved. Before publication, approve exact S/W/L trees and the
public sanitized receipt policy/90-day retention process. Native execution and
residual reporting risk acceptance remain separate approvals.
