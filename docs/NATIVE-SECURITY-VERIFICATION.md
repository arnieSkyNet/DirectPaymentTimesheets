# Development 1.0.7 Phase 1 native verification

The current Windows corrections are uncommitted on `security/v1.0.7-phase1`
at HEAD `b4e15654f18cb8ca9b20ef1c6017205303edca0e`. Native run 38312225467
tested that approved committed source; it did not test this root-owner probe. No tag,
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
