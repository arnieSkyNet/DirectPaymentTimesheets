# Development 1.0.7 Phase 1 native verification

The current Windows corrections are uncommitted on `security/v1.0.7-phase1`
at HEAD `5abca6816b1a74aa6f41304a51fd5e4bb2128b82`. Historical native run 38312225467
tested its earlier approved committed source; it did not test the later corrections. No tag,
package or release is prepared. Existing Phase 1 code, GitHub README edits,
published links and stashes are retained. This is a verification candidate,
not a declaration that the open security findings have been fixed.

## Workflow and isolation

`.github/workflows/phase1-security-native.yml` has only `workflow_dispatch`.
It grants only `contents: read`, pins checkout/setup-python actions, disables
persisted checkout credentials, and in its registered main definition requires
`refs/heads/main` as the dispatch entrypoint. The test source is exclusively
`security/v1.0.7-phase1` at an explicitly approved, hardcoded immutable SHA.
It uses Windows 2022 plus macOS 15 ARM64 and Intel runners.
It compiles the application and tests in the development profile, including the
Apple-SDK C helper on macOS. It never launches the application, calls the
packaging workflows, publishes artifacts, passes repository secrets, or uses
production data. All file/ACL changes are confined to synthetic temporary roots.

The runner script checks that each focused group actually discovers tests and
that the platform-specific Phase 1 marker is present, runs
groups sequentially even after failure, and returns failure if any group fails.
Compilation failure stops execution. Unit tests verify that zero matches cannot
silently pass, stale revisions are refused, and failed groups do not hide later
groups. Runner/test logs identify the source revision and platform. Downloaded
Rust/dependency/toolchain code is still a supply-chain dependency; `--locked`
prevents dependency resolution changes, not network access during fresh builds.

All four pre-existing workflows were inspected: package-rehearsal is manual;
Linux/macOS/Windows package jobs are manual or reusable. None has push, PR, tag,
release or workflow-run triggers. A push to this testing branch does not trigger
them. Recheck that inventory if the remote changes before pushing.

## Historical initial coverage and honest result interpretation

This table records the earlier preparation; current follow-up status and limits
are recorded below and in SECURITY-HARDENING.md.

| Area | Verification | Limits / expected failure |
| --- | --- | --- |
| Windows owner SID | PowerShell queries actual new directory/file owners and compares with current token; no reliance on OW | Positive creation and synthetic Administrators-owner/OW rejection checks. Ordinary foreign-user coverage remains missing; elevated default ownership can differ |
| Windows ancestor access | Broad synthetic ancestor above protected database parent must be refused before file creation | Expected to fail until ancestor validation is implemented |
| NTFS inheritance | Ordinary fixture inherits broad ACL; explicit private file excludes it | Fixture refuses non-NTFS; no silent skip. Immediate-parent assessment does not establish restrictive inheritable ACEs for future SQLite sidecars; this admission gap requires coverage and native evidence |
| Windows reparse points | Synthetic junction cannot redirect creation; target bytes stay intact | Does not prove freedom from concurrent ancestor swaps |
| Windows read-only attributes | Kept/persisted staging must retain deliberately copied read-only policy and private DACL | Expected to fail because tempfile resets attributes |
| Windows failure preservation | Read-only destination refuses atomic replacement without changing original bytes/policy | Native result outstanding |
| macOS ACL entries | Inherited allow must be reported even with 0600 mode; existing inheritable ACL test also checks comma-suffixed entries | Native entry inspection replaces text parsing in the corrections; SDK compilation and native acceptance remain unverified |
| macOS private creation | OS ACL listing must contain no entries after private directory/file creation under inheritable ACL | Native compiler/filesystem acceptance outstanding |
| Staging substitution | Directory and symlink replacements are retained; ancestor substitution cleans only the original anchored parent; unprotected parents retain staging | Linux regression now passes. Windows/macOS refuse automatic cleanup until native implementation is verified |
| Static symlinks, private modes, SQLite sidecars | Existing Phase 1 tests, including isolated Unix umask 000 subprocess | Concurrent namespace races and remote filesystem semantics remain open |
| Restore/recovery | Restore-plan reference lifetime, staged database/config privacy, backup rejection, rollback and filing interruption tests | Existing recovery groups are retained; not a production recovery rehearsal |

The Windows fixtures do not create accounts, impersonate another user, change
the runner token, or grant privileges. A negative test involving an actual
foreign ordinary owner/OW and access under a second non-administrator token is
not yet safely implemented. GitHub Windows runners execute as administrators;
this does not establish ordinary-user acceptance. Controlled standard-user and
foreign-owner verification remains necessary. There is no deterministic test
hook for a concurrent filesystem swap during an open/publication operation;
static links and the cleanup substitution test do not establish race safety.

Never ignore the known-failing assertions or add continue-on-error to obtain
green CI. A failure may be a real gap, fixture setup problem, compilation issue,
or runner assumption; inspect the assertion and OS error. Even a completely
green run would not close the coverage gaps above or approve a release.

## Previously captured baseline checks

The captured Linux results for the complete correction tree are: cargo check
including tests passed; Phase 1 16/16, backup 7/7, database recovery 29/29, filing
recovery 31/31 and Python runner tests 5/5 passed. The full suite passed 893/893
outside the sandbox after its synthetic loopback SMTP fixtures reproduced EPERM
inside the sandbox. No real email recipients or production data were used.
Targeted formatting and `git diff --check` passed; pre-existing formatting
sections were left unchanged. Clippy remains blocked by the unchanged
`src/config.rs:993` non-octal test permission literal. These are previously
captured results, not new test executions during this documentation correction.
The corrected Windows code and macOS SDK helper have not been compiled or run
natively. See [SECURITY-HARDENING.md](SECURITY-HARDENING.md) for exact results.

## Review and commit only the complete correction set (separate approval)

The current classification-only candidate modifies exactly these three paths relative
to HEAD `b4e15654f18cb8ca9b20ef1c6017205303edca0e`:

```text
docs/NATIVE-SECURITY-VERIFICATION.md
docs/SECURITY-HARDENING.md
src/private_storage_windows.rs
```

Reconcile this inventory with the complete diff before any separately authorized
staging or commit. No staging, commit or push is authorized by this preparation.
Do not stage historical correction inventories, use `git add .`, or include
README, workflows, dependencies, release files or production data. Preserve
both safety stashes. This candidate does not change admission/publication policy
and is not production security sign-off.

## Registered workflow and future source-pin approval

The workflow is already registered on main. Do not repeat initial registration
or copy the older workflow from the security branch over the registered main
workflow: its dispatch guards differ. The currently approved source SHA is
`b4e15654f18cb8ca9b20ef1c6017205303edca0e`; it excludes this root-owner probe.
Recheck the live registered workflow before any future pin update or dispatch.

After separately reviewing, committing and pushing the complete correction set:

1. Capture the resulting full 40-character security-source commit SHA and obtain
   explicit approval for that exact immutable revision. A branch name, local HEAD
   or a newly pushed revision alone is not approval.
2. In a separate registration worktree and separately approved one-file PR,
   update only the registered main workflow's hardcoded approved source SHA and
   associated guard expectations/defaults to that approved revision. Preserve
   main-only dispatch, exact source-branch checks, remote-tip checks before and
   after checkout, immutable checkout, checkout-HEAD verification, pinned actions,
   read-only permissions, synthetic tests and absence of packaging/release steps.
3. Review and separately approve that PR before merge. Do not merge the security
   implementation into main. No pin update or default-branch action occurs here.
4. Before any separately authorized dispatch, verify that the workflow is present
   on main at or after source-pin merge f8acbab0a120dfcf70e8f4bd3c047415cb89f347,
   and that the remote security branch tip equals the approved hardcoded SHA.
   Missing/moved branches, mismatched inputs, incorrect workflow refs and a
   mismatched checkout HEAD must fail closed.

`github.sha` describes the main dispatch event, not the security test source.
Checkout must use only the hardcoded approved security-source SHA. Never weaken
the guards to accept matching event/input values for an arbitrary revision.

## Manual dispatch and inspecting results (separate approval)

GitHub requires a workflow-dispatch workflow on the default branch. The registered
workflow must be dispatched **exclusively from main (`refs/heads/main`)**. Never
select the security branch as the workflow ref. Its branch name identifies only
the test source. See [GitHub's manual-run documentation](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow).

Before dispatch, inspect the registered main workflow's actual workflow_dispatch
input names and allowed values, using the Actions UI or this read-only command:

```bash
gh workflow view phase1-security-native.yml --ref main --yaml
```

Supply its actual declared inputs with the exact explicitly approved immutable
security-source SHA and source branch where required. Do not guess input names
or derive approval with `git rev-parse HEAD`. Any future CLI dispatch must use
`gh workflow run phase1-security-native.yml --ref main` with those verified inputs;
that incomplete command is not a runnable dispatch instruction. Dispatch itself
requires separate authorization after all preflight checks pass.

In the Actions UI select **Phase 1 native security verification**, select **main**
as the workflow branch, and enter only the approved source values. To inspect an
already authorized run, without dispatching or rerunning it:

```bash
gh run list --workflow phase1-security-native.yml --branch main --limit 5
gh run view RUN_ID --log-failed
gh run view RUN_ID --log
```

Inspect all three native jobs, compilation output, discovery counts and every
group's result. Match the logged test-source SHA to the approved immutable source,
not the main event SHA. Distinguish fixture/infrastructure failures from compiler
and security-assertion failures. Do not automatically rerun failed jobs, ignore
known-failing assertions or dispatch a packaging workflow as a workaround. Even
a green native run does not close ancestor/path-substitution, inheritable-ACE,
SQLite sidecar, read-only publication or ordinary-user coverage gaps.

## Follow-up to failed run 38264276658

This historical remediation was committed as 026b53f and tested by run
38290913085. See [SECURITY-HARDENING.md](SECURITY-HARDENING.md). The account below
describes the earlier failed run and is not current native acceptance.
All three jobs compiled successfully but failed fixture creation/assessment.
Linux validation of the follow-up is not native acceptance. Owner validation
and ACL entry inspection have been changed; ancestor and read-only regressions
remain active and may still fail. The new synthetic Windows foreign-owner fixture
may require owner-assignment permission on the runner and must fail visibly if
that capability is unavailable. macOS now also tests absent ACLs and invalid
handles. Temporary fixtures remain private and non-Linux cleanup remains disabled.
Review and commit the source changes separately, then obtain approval for a new
immutable workflow source pin; do not rerun the old SHA to claim remediation.

For the registered main workflow, the dispatch entrypoint remains main; only the
test source branch/SHA is selected. This follow-up changes neither the registered
workflow nor its approved SHA. The native helper/API changes require a new
reviewed commit before updating that approval; Linux checks cannot substitute for
Windows/NTFS or Apple-SDK compilation and execution.

## Historical Windows follow-up at 026b53f (committed as c11221b)

Run 38290913085 passed both macOS architectures and native compilation on all
platforms, but Windows failed two replacement operations (error 5), unsafe
ancestor admission and read-only keep/persist assertions. The new local code
adds redacted operation diagnostics, a conservative database ancestor/inheritance
guard and private keep/publication wrappers. See SECURITY-HARDENING.md for API
assumptions, retained failure guarantees and remaining path/publication races.
The previous 893-test result is for the committed baseline, not this follow-up.
The passing macOS run likewise predates these common Rust wrapper changes;
no native acceptance of the current follow-up is established. Windows rename
semantics, default profile ACLs, standard-user access and new guard policy must
be reviewed natively before claiming resolution or approving production use.
The historical ten-file correction is now committed. Do not change the
registered main workflow/source pin as part of this correction set.

The independent review retains production sign-off blockers: ancestor handles
expire before SQLite opens and do not cover later journal/WAL/SHM operations;
publication and failure cleanup remain pathname-based in shared writable parents.
The conservative owner/ACE/inheritance policy can reject legitimate storage.
Replacement of open/read-only destinations and restrictive copied DACLs still
require native evidence. Negative tests that accept any error can mask an earlier
refusal, and nested diagnostic wrappers lose structured OS error codes. These
limits must not be presented as resolved merely by committing or obtaining a
green runner result. Separate approval of the resulting immutable source SHA,
a separate main workflow pin update and separately authorized main-only dispatch
are required before testing this candidate natively.


## Diagnostic candidate after run 38302554351

Windows compiled but reported 66 owner_mismatch/unsafe_ancestor runtime failures
and one MoveFileExW error 5 at write_atomic publication. Both macOS architectures
passed Phase 1 19, backup 7, database recovery 29, filing recovery 31 and Python 5.
Windows Phase 1 was 17 passed/5 failed, backup 0/6, database recovery 3/26,
filing 0/30; Python 5 passed. No exact ancestor or rename denial cause was logged.

The new Windows-only diagnostics report component depth, volume_root /
existing_ancestor / immediate_parent role, SID validity and current / system /
administrators / other / invalid category. No service-owner identity is inferred
or newly trusted. No paths, actual SIDs, account names or ACLs are rendered.
The owner allowlist, destructive-access mask, inheritance checks, held identity
handles, rename flags and fail-closed behaviour remain unchanged.

Negative ancestry fixtures now require positive admission first and the precise
refusal reason; an earlier system-owner failure can no longer count as a pass.
A separate positive control reserves a private database. The eight-case native
publication matrix covers absent normal/read-only stages and existing normal /
read-only destinations with closed, read-sharing-7 and blocking-sharing-3 handles.
Its normal open-sharing-7 success expectation is a regression contract to test,
not a claim that MoveFileExW supports that case. API outcome/refusal-class
mismatches are counted for the final assertion; setup, invariant and cleanup
failures terminate immediately and may prevent remaining cases from running. Raw errors come from the same MoveFileExW call used in production. Tests
compare bytes, available handle identities, attributes and opaque security-policy
snapshots; snapshots are never printed. Closed-destination cases deliberately
avoid retaining an original handle so the control is actually closed. Only
synthetic cleanup resets read-only, after assertions.

For separately authorized local native diagnostics, run the matrix with
`cargo test --locked phase1_windows_atomic_publication_handle_attribute_matrix -- --nocapture`.
Successful-test output is otherwise captured by Rust. This is not a workflow
change or dispatch authorization. Native Windows compilation/execution is still
required; Linux cannot exercise this module. Standard-user Windows 10 controls
would materially help isolate open-target semantics and profile ancestry but do
not establish Server 2022 acceptance. Existing SQLite lifetime, shared-parent
publication/cleanup races, recovery and production sign-off blockers remain.

Ancestor API errors carry their own operation, code, classification, depth and
role even when logs interleave. RootDir labelled volume_root is the namespace
root; UNC paths denote a share root rather than necessarily a physical volume.
Matrix expected refusals require a structured OS code in the documented access /
sharing / locking / missing-privilege categories (5, 32, 33, 1314). Missing paths,
invalid parameters, unrelated codes and unstructured errors cannot satisfy a
negative case. A native-only classifier test covers these distinctions; the
accepted category alone still does not prove a refusal's underlying cause.


## Classification-only root-owner probe after run 38312225467

The committed baseline reported 69 depth-zero volume_root refusals with a valid
owner SID classified other. This does not identify the owner or justify trusting
it. The new probe executes only in that valid-owner rejection branch, retaining
the original owner_mismatch result. It selects exactly one fixed hypothesis:
the locally qualified TrustedInstaller service SID. There is no configurable,
enumerated or arbitrary service-principal admission mechanism.

The probe validates the resolved SID, compares it with EqualSid, verifies forward
and reverse lookup identity/domain against fixed qualified service literals, opens
only the exact service key through the local SCM, and queries SERVICE_SID_INFO.
Only documented enabled configurations (unrestricted/restricted) count as
configuration verification. Disabled/unknown configurations, missing candidates,
lookup failures and SCM access failures are indeterminate. This identifies a
configured per-service SID, not a service process logon account or running token;
it establishes no trust, executable integrity, root safety or authorization.

Output contains only fixed classification/operation labels, depth/role, resolution
and verification booleans, an optional match boolean and raw error codes. No SID,
lookup name/domain, service key, path or ACL is printed. The fixed candidate name
is used internally but not included in diagnostic output. Bounded aligned SID
storage and bounded UTF-16 lookup buffers refuse oversized results; SCM handles
use CloseServiceHandle and request CONNECT / QUERY_CONFIG only. No service is
created, started, reconfigured, impersonated or assigned privileges.

Three new Windows-only synthetic tests cover native known SID-category comparisons,
the exact gate, unresolved/unverified fact combinations, false matches, invalid
lookup text and diagnostic redaction. They do not depend on the runner root owner
or service availability. They do not prove native lookup/SCM API acceptance; a
separately approved native run is still necessary. Tests inject verification facts
for unavailable-candidate paths rather than changing real service configuration.
Native evidence must confirm candidate resolution, independent SCM verification,
standard-user access/refusal, raw errors and unchanged owner rejection. A positive
classification is evidence for future review, never an automatic allowlist change.
The existing ancestor, SQLite lifetime, publication/cleanup and recovery limitations
remain open, including MoveFileExW open-target refusal. No native run is dispatched.


### Root-owner probe availability and evidence limits

LookupAccountNameW and LookupAccountSidW begin resolution locally with the current
NULL system parameter, but even qualified lookup may consult trusted domains.
They are not guaranteed local-only and have no guaranteed bounded latency.
Repeated rejected-root inspections may synchronously repeat those lookups and
read-only SCM operations while ancestor directory handles remain held. This
probe is diagnostic-only; bounded or opt-in execution and its availability risks
must be assessed before production integration. Qualification and bounded memory
buffers do not bound lookup execution time.

The synthetic tests do not establish actual API failure handling, raw-error
capture or SCM handle cleanup through the real probe. Native compilation and
runtime evidence remain necessary for those paths. Forward/reverse identity
mapping and service-SID configuration are separate, non-atomic observations.
They establish neither executable trust nor a running token's identity. A
configured service SID remains distinct from the process account and running
token. verified_candidate_service_sid is a diagnostic classification, never
authorization to admit the owner; owner_mismatch remains the admission result.


## Opt-in namespace-root and DACL diagnostics (candidate, not native acceptance)

The next candidate keeps all owner refusals and trustee restrictions intact.
`DPT_WINDOWS_ROOT_DIAGNOSTICS=1` explicitly enables diagnostics only for a valid,
rejected `other` owner at depth zero with the syntactic `volume_root` role. Without
that exact opt-in, neither new root queries nor the existing account-resolution /
SCM probe runs. No workflow or pin has been changed to enable it.

The read-only root query requires an exact handle-derived volume GUID root,
DRIVE_FIXED, NTFS with FILE_PERSISTENT_ACLS, no observed reparse attribute, and
matching FILE_ID_INFO (64-bit volume serial plus 128-bit file ID) from an
independently opened GUID root and a repeat query of the original handle.
GUID strings and identities stay in memory and are never logged. Unsupported,
remote, malformed, descendant and ambiguous namespaces cannot produce a verified
classification. After a GUID-query failure, an opened handle-derived DOS UNC name reports
network_namespace with the original numeric query error. If that observation is
unavailable, the failure remains indeterminate; no particular network type is inferred. The query is conservative and is not a general filesystem
compatibility or adversarial-driver attestation. Classification never authorizes
admission, including when it reports verified_local_ntfs_root.

All root/DACL records carry fixed depth=0 and role=volume_root context. Root
errors retain fixed metadata, final-path, volume-information, independent-open
and file-ID operation labels. Fallback errors preserve both primary and fallback
codes; a successful fallback records whether UNC was observed. Insufficient
final-path buffers use derived ERROR_INSUFFICIENT_BUFFER (122), rather than a
stale GetLastError; an oversized GUID-root result remains ambiguous and does not
open an independent root. GetDriveType supplies a drive classification, not a documented
GetLastError failure; non-fixed results remain unsupported.

Root DACL diagnostics use the same extracted policy evaluator as admission,
independently of the unchanged owner refusal. policy_pass means only that the
existing conservative ACE scan passed; it is not effective AccessCheck, does not
attest owner rights, and does not authorize an otherwise foreign owner. OW and
inherit-only handling, unsupported-ACE refusal and destructive-access masks are
unchanged. No TrustedInstaller owner or trustee exception is added.

Seven new Windows-only tests cover injected positive/negative root facts, strict
GUID/UNC/device/descendant parsing, failed-query classification and private
rendering, in-memory foreign-owner safe/unsafe/OW DACLs, and unchanged private
fixture identity/policy after queries. Injectable query stages additionally exercise
successful independent-root comparison, identity mismatch, reparse substitution,
each fallible stage, and RAII drop counts after open and subsequent failures.
Primary GUID errors and UNC fallback failure/non-UNC/success outcomes are asserted
separately. Structured DACL error tests retain GetAclInformation/GetAce operation
and original numeric code without changing admission error text or kind.
The foreign-owner fixture is a synthetic
stand-in, not a TrustedInstaller authorization test. Reparse and identity mismatch
facts are injected; no machine volume root is mutated. Invalid-handle failure is
queried read-only. Real local-root success, UNC behaviour, standard-user access,
API ABI/error paths, actual OS handle closure and concurrent substitution still
require native execution. Synthetic drop counts verify Rust orchestration, not
CloseHandle success or driver behaviour. Existing junction/substitution regressions remain.

Normal production refusals no longer synchronously repeat unbounded account
resolution. Explicit opt-in still permits repeated lookup/SCM work while directory
handles are held; memory bounds do not bound latency. Volume/file APIs may also
block. Independent root File handles use RAII; native cleanup and failure paths
are unverified. Identity, filesystem and service observations are non-atomic.
SQLite guard lifetime, pathname publication/cleanup and MoveFileExW open-target
error 5 remain unresolved; production sign-off is not implied.

Available Linux validation: cargo check --locked --offline --bins --tests passed;
Phase 1 tests 18/18 and private_storage tests 15/15 passed. Windows-only code/tests
are excluded from these Linux results; only the Linux Rust target is installed.
Targeted rustfmt and git diff --check passed. Native verification must explicitly
opt in through a separately reviewed change; this candidate does not dispatch it.

API references: [final path by handle](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getfinalpathnamebyhandlew),
[volume information by handle](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getvolumeinformationbyhandlew),
[file information by handle](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getfileinformationbyhandleex).

## Controlled synthetic root observation (verification candidate)

This follow-up supersedes the opt-in execution boundary above. Production builds
cannot activate root diagnostics, even with `DPT_WINDOWS_ROOT_DIAGNOSTICS=1`.
Only the thread-local scope in
`phase1_windows_positive_database_ancestor_admission` permits one observation,
consumed on the first qualifying rejected root. Its ancestry walk starts from a
fresh synthetic private-parent fixture. The only additional real objects queried
are that fixture's runner-local volume root and the independently opened
handle-derived GUID root; no real root ACL is changed. Other tests, threads and
subsequent fixture operations cannot consume this scope. Scope exit, including
panic unwinding, clears it. No account-resolution or SCM probe is invoked.

For a separately reviewed controlled Windows run, set
`DPT_WINDOWS_SYNTHETIC_ROOT_DIAGNOSTICS=1` in the environment of
`python -B scripts/phase1_security_checks.py`, with the existing exact
`EXPECTED_SHA` gate satisfied. The harness supplies
`DPT_WINDOWS_ROOT_DIAGNOSTICS=1` and `--nocapture` only to the Phase 1 test
execution. No workflow defaults or source pins are changed. Use this contained
harness for diagnostic execution; a direct opt-in `cargo test` invocation does
not establish process-tree containment. The harness never runs the application
or accesses payroll data.

Expect at most one `windows.root_namespace` and one `windows.root_dacl` record
when the root has a valid rejected other owner. Namespace classification covers
fixed local NTFS, persistent ACLs, reparse observations and matching independent
and repeated file identities. DACL classification uses the unchanged admission
scan, independently of the owner refusal. Records contain fixed classifications,
booleans, depth/role and numeric error codes; identities and ACLs stay in memory.
An admitted root produces no additional records. A rejected root still fails the
positive test with owner_mismatch; verified namespace or policy_pass never grants
admission, including to TrustedInstaller.

Observation count and buffers are bounded, but synchronous volume/file APIs have
no intrinsic latency guarantee. Only the opt-in Windows Phase 1 execution
receives a 600-second budget, through
an unnamed Job Object. Cargo is assigned atomically using
PROC_THREAD_ATTRIBUTE_JOB_LIST at CreateProcessW time, created suspended,
checked with IsProcessInJob, then resumed. Windows 10 / Server 2016 or newer is
required; unsupported APIs or nested-job restrictions refuse execution without
an uncontained fallback. JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE is set and neither
breakaway flag is permitted. Only duplicated stdin/stdout/stderr handles are
inherited through an explicit handle list; the job handle is never inherited.
The harness is single-threaded during handle duplication and process creation.

On success, failure, timeout or exception, the harness terminates the entire job,
queries its active process count until zero (a separate ten-second cleanup
budget), verifies the leader is signaled, and closes owned handles. Unconfirmed
cleanup or an API/handle-close failure stops the verification run; later groups
may run after timeout only when cleanup was confirmed. Other groups retain their
original execution without a new timeout. The supervisor remains outside this
new job; closing its last job handle is also the fallback for supervisor death.
This contains trusted CreateProcess descendants, including Cargo's test binaries
and their subprocesses. It is not a hostile-code sandbox: broker/WMI-created
processes or work delegated to existing services are not covered. The controlled
fixture commands do not launch processes through such brokers.
Independent File handles retain RAII cleanup; injected drop tests do not prove
native CloseHandle success. Native execution is still required for ABI, actual
root observations and cleanup evidence. No native run was dispatched for this
follow-up; the result of run 38319219921 was not fetched or independently verified.

Follow-up Linux validation: targeted rustfmt and
`cargo check --locked --offline --bins --tests` passed; Phase 1 tests passed 18/18,
private-storage tests passed 15/15 (overlapping groups), and harness guard tests
passed 8/8, with three native-only tests skipped on Linux. `git diff --check`
passed. The Windows-only gate test now exercises missing / non-exact / exact
environment
settings in isolated test subprocesses, cross-thread isolation, scope consumption,
nested-rearm refusal and unwind cleanup through the actual diagnostic gate. It
was not executed on Linux. Production builds retain the separate cfg(not(test))
false-returning branch; native production compilation is still unverified.
The complete diff was
reviewed separately for privacy and unchanged admission/publication policy.

### Containment follow-up validation boundary

Three native Python fixtures in the existing harness test file exercise a synthetic
leader/child/grandchild timeout with a grandchild holding a non-shared file handle,
cleanup of a descendant after normal leader exit, and explicit breakaway refusal.
The timeout fixture requires
that the grandchild reached its held-handle state, whole-job ActiveProcesses became
zero, the leader was reaped, and deleting the held file succeeds after cleanup.
They run on Windows and are explicitly skipped on Linux; mocked failure-accounting
tests are not native containment proof. Run
`python -B -m unittest discover -s scripts -p test_phase1_security_checks.py -v`
on the approved Windows runner before opting into diagnostics. No native fixture
or diagnostic run has been dispatched here. Windows ctypes ABI, runner nested-job
compatibility, actual timeout termination and handle-release evidence remain
unresolved until that run succeeds.

The old subprocess.run(timeout=600) implementation was rejected because killing
Cargo alone leaves descendants uncontained. It is removed. The Job Object candidate
is ready for source review, not accepted as proven native containment.

References: [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects),
[process creation attributes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute).

The breakaway fixture first creates and reaps an ordinary equivalent process.
It then changes only CREATE_BREAKAWAY_FROM_JOB and accepts only
ERROR_ACCESS_DENIED (5), the expected refusal when the containing job does not
permit breakaway. Missing controls, unexpected denial codes, successful breakaway
and runner/nested-job incompatibility fail the fixture; they are never silently
accepted or skipped on Windows. A successful breakaway is immediately killed
and waited on, and still fails verification. This is a controlled negative test,
not proof against brokers or adversarial process creation.

## Bounded leader wait after native timeout failure

Run 38374457447 passed breakaway refusal and normal-exit cleanup, but the tree
fixture failed with reap_unconfirmed after its ten-second timeout. The previous
helper observed zero active job processes, then checked the leader with a
zero-time wait. Its log did not preserve the wait result or OS error, so a
completion-order race, wait failure or surviving leader cannot be distinguished.
No handle-lifetime defect is established by that evidence; the owned process
handle remained open until the cleanup finally block. The exclusive-handle
assertion was not reached, and the workflow correctly skipped the Windows
harness and root diagnostics. Both macOS jobs passed 19 Phase 1, 7 backup,
29 database-recovery and 31 filing-recovery tests.

The correction waits explicitly for the leader to signal within the same
shared ten-second cleanup deadline, then independently confirms zero active job
processes. WAIT_TIMEOUT, WAIT_FAILED (with numeric GetLastError), unexpected wait
values, accounting failures, deadline exhaustion and handle-close failures remain
refusals. GetExitCodeProcess remains an exit-status query after a successful wait,
not a substitute for termination signaling; STILL_ACTIVE alone is not proof.
The native fixture still requires a reached held-handle state and successful
exclusive-file deletion after cleanup. A deletion refusal has fixed redacted
classification and numeric error. Assignment, breakaway restrictions, diagnostic
gates, admission and publication are unchanged.

Linux Python validation: 14 tests passed, including six focused cleanup tests;
three native fixtures were skipped. These mocks cover delayed leader signaling,
successful reap, timeout, failed/unexpected wait, surviving descendant, inaccessible
accounting and handle-release refusal; they are not Windows termination proof.
Native success, exclusive-handle release and the underlying cause of the previous
failure remain pending a separately authorized native run. No workflow pin was
updated and no run was dispatched for this correction.

## Molly: authorized production volume-root policy — 15 October 2026

The user separately authorized the exact TrustedInstaller owner SID at a proven
fixed local NTFS volume root, including the residual implicit WRITE_DAC risk.
SECURITY-HARDENING.md records the revised scope. The normal Windows tests verify
Molly's real root owner and GUID/volume/identity proof without modifying its ACL,
and reject the same owner when assessed in private-object/intermediate roles.
Injected SID/root-fact cases cover foreign or invalid owners, wrong depth/role,
UNC/network/removable/unsupported volumes, query errors, reparse and identity
mismatch. Production admission performs no account, SCM or signature lookup.

Native regression groups passed: approved root policy (2), existing root queries
(9), DACL diagnostics (1), diagnostic scoping (1), Email Settings (7), email
service (10), SMTP transport (14), standalone actions (11), database refusal
controls (3), junction refusal (1), positive database ancestry (1), inherited
ACL/private creation (1). Groups overlap and must not be summed as unique tests.
The layout regression checks both field rows and aligned action headings/buttons
at 1000/1360 pixels, clicks standalone Preview Email with no SMTP credentials or
payroll/PA context, and verifies no database changes or send worker. Narrow windows
use horizontal scrolling to retain each row. No real email was sent.

Tests use a new private synthetic TMP/TEMP beneath Molly's AppData\Local because
the ordinary Temp directory has an unrelated modify ACE. No production files or
settings are copied. Three existing negative tests initially failed at PowerShell
Set-Acl's audit-security privilege requirement; their fixture setters now write
only a synthetic DACL via .NET, preserving owner/SACL and all assertions. No
privilege was enabled. Normal and development-feature Cargo checks passed.

Molly's separate ACL-only repair was subsequently authorized and completed on
15 October 2026. The managed application root and 17 recorded objects were
verified against the approved owner/group/DACL manifest; all eight file hashes,
schema 41 and logical row counts remained unchanged. The exact trusted grants
were retained and CodexSandboxUsers access removed. This was a separate external
repair, not an application permission-repair feature or a private-parent bypass.

The normal Windows x64 application subsequently launched with Molly's real
profile, existing employer/PA/settings and no development banner. Local evidence
is under target/windows-v107-acl-repair and later windows-v107-* reports. Those
ignored artifacts contain private information and are not release documentation
or files to commit. Later read-only inspection found config.toml using inherited
trusted-only grants rather than its recorded protected DACL; that metadata drift
is a separate observation, not proof of additional access or a diagnosed app bug.
See the current correction-pass report for the comparison and attribution limits.

The About/native database regressions now require positive read-only schema
access through admitted ancestry before testing controlled unsafe-ancestor or
junction refusal. Exact/foreign SID, wrong-role/depth and root-fact negative
coverage remains in private_storage_windows.rs. A machine's actual root is never
changed to manufacture a negative owner fixture.

Correction-pass validation on Molly, 15 October 2026: focused executions passed
15/15, including both replacement About cases, approved-root policy, DACL
rejection and unauthenticated remote relay readiness/STARTTLS/implicit TLS.
Normal and development-feature Windows x64 cargo check --bins --tests passed.
Cargo metadata/package listing includes required development source modules and
excludes retained temporary preview/generated artifacts. See
V1.0.7-COMMIT-CHECKLIST.md for the required new-file inventory.

Historical pre-publication-correction Windows x64 cargo test run (superseded by
the validation below) was NOT green: 899 passed,
41 failed, zero unit tests ignored (1439.97 seconds). Integration targets ran
zero tests and retained one pre-existing explicitly unauthorized ignored native
child fixture. Of the 41 failures, 37 report MoveFileExW access denied during
atomic replacement/PDF generation. The native publication matrix independently
fails for an existing writable destination held open with delete sharing. The
ancestor guard test observes a rename succeed while its handle is held. The
remaining failures are missing symlink privilege (OS 1314) and an owner-rights
fixture PowerShell setup failure. No assertions were relaxed or privileges
enabled. These are outstanding validation/security blockers, not evidence that
the completed ACL repair or approved root policy should be bypassed.

Both development compile/launch gates, all nine standalone Email Settings UI
tests and all fifteen SMTP transport tests passed in that full run. No real GUI
or production SMTP endpoint was used. The ignored local correction-pass report
and full log retain exact failures; resolve atomic publication, held-ancestor
rename guarantees and fixture prerequisites before marking native validation
complete or the branch ready to commit on that historical evidence alone.

## Approved Windows publication correction — 15 October 2026

The operator approved excluding malicious/uncooperative full-authority same-user
processes. Other-user protection and all unrelated owner, ACL, reparse,
held-identity and SQLite containment requirements remain. See
SECURITY-HARDENING.md for the exact boundary, native API contract and residual
validation-to-rename race; there is no expected-target-ID compare-and-swap claim.

Publication now uses source-handle FileRenameInfoEx POSIX replacement with the
destination identity handle retained. FILE_LIST_DIRECTORY ancestor guards and
verified fixed local NTFS remain required. Per-parent-identity, per-user Global
mutexes serialize cooperating writers; failures never trigger copying, TxF or an
ACL/attribute bypass. Target owners/ACLs are checked before staging and before
copying an access policy onto new bytes. Unexpected source/target identities
refuse publication; an absent target is always published without overwrite.
The trusted renderer's completed private stage is validated under the same guard
before hashing/ledger approval, rather than weakening the publication ID check.

The owner-rights fixture's error 1307 was invalid owner assignment to
Administrators under Molly's filtered token (see local FOLLOWUP.md), not a
production owner-policy failure. Its replacement checks an actual native
in-memory foreign-owner descriptor and the production owner predicate, plus a
real current-owner positive control whose security descriptor is unchanged.
It does not claim a physical foreign-owner fixture succeeded.

Native symlink coverage remains explicitly unavailable on Molly without
SeCreateSymbolicLinkPrivilege/Developer Mode. Its ignored test requires an
independently capable host and is never counted as passed. Junction refusal and
held-directory rename/substitution checks remain active. The ignored cooperating
publication child is actually executed by its parent test; the separately ignored
SQLite terminal child still requires its future harness authorization.

Final focused validation passed 32 test executions (one repeated ancestor test):
existing/absent destinations, open anchors, read-only/sharing refusal, unchanged
contents, unsafe ancestry, unsafe destination policy before staging, stage
substitution, destination ID changes, no-clobber insertion, cross-process writer
serialization and the explicitly excluded after-check same-user race. Config
creation and real renderer/snapshot integration regressions also passed.
Normal and windows-dev-isolation Windows x64 cargo check --bins --tests passed.
No production ACL repair, GUI launch, settings save or real SMTP send occurred.

Final default-feature Windows x64 cargo test --locked --offline --no-fail-fast
with --test-threads=1 exited 0: 949 unit tests passed, zero failed, two ignored
(1448.53 seconds). One ignored entry is the unavailable symlink test; the other
is the cooperating child explicitly exercised by its passing parent. Integration
targets ran zero ordinary tests and reported the pre-existing unauthorized
native_sqlite_terminal_fixtures child as one ignored test. Neither unavailable
native coverage nor the physical foreign-owner fixture is claimed as passed.
Interrupted diagnostic attempts are retained separately, not completed runs.
Evidence: target/windows-v107-diagnostics/PUBLICATION-CORRECTION.md,
publication-full-cargo-test.log, final-focused-*.log, publication-check.log and
publication-development-check.log. These local artifacts are ignored; required
production code includes src/private_storage_windows_publication.rs. Existing
preview/development files remain retained, with the commit inventory in
V1.0.7-COMMIT-CHECKLIST.md. SMTP plaintext credential storage is explicitly
deferred by that publication-only pass. The subsequent 16 October SMTP storage
implementation is documented in SMTP-CREDENTIAL-STORAGE.md; other-platform
native validation is still required, not implied by the earlier full test run.
Proceed to the next security review under the approved boundary, not release
approval. That publication-only run did not cover restore/config-save coordination.
The subsequent SMTP storage pass integrates BackupService restore with the same
mutex, pre-commit fingerprint recheck and guarded atomic writer; a focused Windows
regression verifies competing-save exclusion and stale-approval rejection. Other
platforms/architectures and a symlink-capable native host were not validated in
this pass. git diff --check passed with existing LF/CRLF notices; HEAD and branch
are unchanged and all earlier untracked preview/development files are retained.

### SMTP credential storage Windows completion, 16 October 2026

The subsequent credential-storage completion run passed the complete Windows x64
suite: 961 unit tests passed, zero failed, five ignored (1453.78 seconds). The
guarded-connection integration target has no tests; the native SQLite terminal
child integration fixture remains explicitly ignored pending separate harness
authorization, not counted as passed. Native symlink privilege coverage remains
unexecuted on Molly. The publication child harness runs through its parent test.

Separately invoked synthetic native migration/recovery/restart test: one parent
passed, including its fresh-process child's success and removal/absence checks
for only its own test-namespace Credential Manager entries. Ordinary unit tests
use Memory and cannot silently access real credentials. Native provider denial
is simulated through Store; the blocked-publication/recovery path uses a real
disposable read-only file. This is not a native vault ACL/privilege test, nor
logout/reboot durability evidence. Normal and windows-dev-isolation Windows x64
cargo check --bins --tests both passed; git diff --check passed with existing
line-ending notices. Evidence: ignored target/windows-v107-diagnostics/
credentials-completion-*.log and SMTP-CREDENTIALS.md. Cross-platform native gates
and recovery limitations are in SMTP-CREDENTIAL-STORAGE.md. No real-profile
migration, GUI launch, real SMTP send or production data/ACL change was performed.
