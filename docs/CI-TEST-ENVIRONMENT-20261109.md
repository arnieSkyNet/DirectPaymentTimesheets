# Rehearsal test-environment corrections — 9 November 2026

Base: `877abd2fffbbdba007f2c6da38e83a3791826cee`. Historical failing rehearsal:
[42424203279](https://github.com/arnieSkyNet/DirectPaymentTimesheets/actions/runs/42424203279)
at `a63f2556198c30b2c2bad0d79b4633f8c5c57a63`. Only historical job logs were read;
no Actions run was triggered, rerun or monitored for this correction.

## Corrections

- Linux x86_64/aarch64: the separate Debian packaging/test container did not have
  rustc. ARMv7's compiler probe also failed in its target userspace. Install a
  native Debian rustc only in the test-runtime container, explicitly select
  `/usr/bin/rustc`, and fail before tests if it is absent. The cfg-matrix test now
  honors `RUSTC` and reports process status. No test is skipped. This standalone
  compile-error probe uses Debian's compiler; application/test executable builds
  still use pinned Rust 1.98.1 in the original build container. Test-only compiler
  installation does not change application runtime dependencies or package pins.
- macOS: normalize the OS temporary root into physical TMPDIR before Cargo starts
  the fixtures. Raw tempfile paths then avoid the /var system alias before SQLite
  sees them. No production database path, no-follow flag, ownership latch or
  symlink admission policy changes.
- Windows x64: invoke the existing Cargo tests through
  `scripts/windows-package-tests.ps1`. Hosted elevated tokens can default ordinary
  object ownership to Administrators, defeating current-user fixture assumptions.
  The wrapper changes only the current PowerShell process token's default owner
  to its existing user SID, verifies readback and an ordinary created file, and
  restores the old owner and TMP/TEMP in finally. It creates a unique private
  current-user/System-only NTFS temporary root; it does not repair any existing
  directory, change user/group identities, enable privileges or relax admission.
  The ordinary suite runs without skips or diagnostic opt-ins. Evidence is retained.
- Windows NTFS fixture: query DriveInfo instead of the failing Storage-module
  Get-Volume -FilePath directory query; strip only the synthetic volume's verbatim
  spelling before the query. NTFS remains mandatory.
- Traversal fixture: preserve literal ParentDir components in an ordinary Windows
  drive spelling. Joining onto a verbatim tempfile path had normalized away `..`
  before the security code received it. Assert the traversal exists, then require
  both source readers to reject it; retain byte/permission/noncreation assertions.
- Unsafe-ancestor backup fixture: failed population deliberately retains a private
  `.incomplete` recovery directory. Require exactly one new private incomplete
  directory, preserve every original backup directory, and verify validation and
  discovery refuse the new incomplete backup. Source bytes, ACLs and all existing
  refusal assertions remain checked.

The token-owner API changes default ownership of newly created objects, not token
privileges or group membership. See Microsoft's
[TOKEN_OWNER](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-token_owner)
and [SetTokenInformation](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-settokeninformation).

## Focused local verification

- Selected actual Rust tests: 3 passed, 0 failed, 0 ignored: both development-gate
  tests and backup source parent-traversal refusal. Test execution 0.34 seconds;
  incremental test compilation 24.84 seconds. No full suite or package build.
- Compiler regression: the gate passes with PATH unavailable and an explicit real
  RUSTC; removing RUSTC under that same PATH fails with the intended diagnostic.
  This negative control is expected refusal, not an unresolved test failure.
- Actual SQLite NOFOLLOW regression on a disposable symlinked temporary root:
  alias path refuses with SQLITE_CANTOPEN (14); the macOS script's real TMPDIR
  normalization block resolves the root and the physical path opens successfully.
- Four existing focused packaging checks passed: artifact-only/read permissions
  and action pins, Linux invalid-target refusal, macOS minimum/export ordering,
  and Windows installer architecture/data-preservation rules.
- Modified shell scripts passed bash syntax checks. The Windows wrapper's C#
  interop type compiles as a standalone library on Mono; it was not executed.
  Changed Rust fixture modules parse successfully without formatting source.
- Initial local Rust selection stopped at Cargo argument parsing before any
  compilation or tests; the corrected harness invocation produced the pass above.

Evidence retained under `/tmp/dpts-rehearsal-env-20261109/`; this directory is
runtime scratch and may disappear on reboot. Historical reports remain unchanged.

## Remaining platform verification

The Windows PowerShell/Win32 wrapper, actual default-owner inheritance, NTFS
sidecars, destination permissions, verbatim-path traversal and incomplete-backup
fixture still require native Windows execution. Windows ARM64 remains compile/
package-only under the existing workflow. Native macOS Intel/ARM64 and Linux
ARM64/ARMv7-QEMU executions of the corrected environments were not performed.
Linux behavioral checks and C# compilation do not establish those native passes.
Reviewable corrections are prepared; a successful seven-target/ten-package
rehearsal is not claimed. No existing completed security campaign is reopened.

All production ownership/security/SQLite implementations and accepted GUI code
remain unchanged. Rust source edits are confined to cfg(test) modules and the
Windows-only test fixture file. Historical REBOOT-CHECKPOINT.md and
RESTORATION-STATUS.json remain untouched. No commits, pushes, merges, Actions,
package builds, releases, production payroll or real credential operations.


## Follow-up: remaining failures in run 42615222339

Reviewed the existing completed run's jobs, selected existing job logs and
artifact inventory read-only. No workflow was dispatched, rerun or monitored.
Source: `d8b9a8efbbd00c1e211f1f23b0c74b8cfca3fa57`.

### Confirmed Linux option defect

Linux x86_64 and ARMv7 job logs report Rust 1.63.0 and this exact compiler error:
`incorrect value false for codegen option debug-assertions`; accepted values
include `yes` and `no`. All three Linux jobs have only the compiler gate failing,
with 1121 application tests passing (the separately run 23 SQLite integration
cases also passed where inspected). The test now selects `debug-assertions=yes`
or `debug-assertions=no`, retaining all four feature/debug/profile cases, the
positive control, and the requirement that negative cases report the actual
security compile_error rather than accepting any compiler failure. No production
build gate or compiler/package workflow was changed in this follow-up.

### Windows evidence and its limit

Windows x64 job 129153384471 records 1149 passed, 1 failed, 7 ignored. The sole
failure is `phase1_windows_actual_new_object_owners_match_current_token`, at the
generic PowerShell helper assertion: exit code 1, command_failure. The wrapper's
private-root/default-owner preflight passed. Other native fixture tests passed.

The helper captured stdout and stderr but discarded their contents on failure.
Consequently the original PowerShell exception, exact HRESULT and failing script
stage cannot be recovered from the published log. There is no Windows x64 debug
artifact: the run retained only both macOS package artifacts and Windows ARM64.
Do not present a guessed PathNotFound or AccessDenied exception as observed fact.

The uniquely used Get-Acl owner probe receives the canonical Windows fixture
path from private_storage::tempdir, including the verbatim drive prefix. This is
a provider compatibility risk (see PowerShell's
[documented extended-prefix issue](https://github.com/PowerShell/PowerShell/issues/10805)).
That is an evidence-based explanation, not a native reproduction of the lost
exception. The narrow fixture correction now uses the ordinary drive spelling
only for this Get-Acl query, removing the prefix only for VerbatimDisk paths and
requiring same-file identity between the original and query paths before use.
UTF-16 is retained without lossy conversion. Production paths are unchanged.
The existing exact owner-vs-current-token assertion and private ACL assertion
both remain mandatory for the real directory and file.

The owner script now identifies get_acl/get_owner/current_token/compare stages
and catches exceptions into fixed path_not_found/access_denied/exception
categories plus a numeric HRESULT. The Rust helper whitelists the marker, stage
and category and parses an i32; malformed/private output becomes unclassified.
Raw stderr, paths, account names, SIDs and exception messages remain undisclosed.
All failures still fail the test. No assertion or owner/ACL/SQLite policy is relaxed.

### Focused checks and outstanding native execution

Evidence: `/tmp/dpts-ci-followup-42615222339/` (scratch, not durable storage).

- Two actual development gate tests copied verbatim with their helper into a
  small Rust test harness: PASS, 2 passed. No full application build or full suite.
- Compiler gate repeated with a strict option wrapper rejecting true/false:
  PASS. The wrapper delegates real compilation to local Rust 1.98.1; this is not
  a claim that Debian Rust 1.63 was executed locally.
- Four tests of the exact new diagnostic parser: PASS. Includes all accepted
  stages/categories, malformed and overflowed codes, invalid UTF-8, private
  canaries and extra-field injection. No raw canary is emitted.
- Both changed Rust test files parse successfully. Diff whitespace and unchanged
  production-source/metadata checks pass.

Native Windows verification still required: run only the owner test against
actual newly created directory/file objects, verify the identity-checked path,
PowerShell owner result and private ACL checks, and inspect the fixed-category
stage/HRESULT if any exception remains. No Windows executor or PowerShell is
available in this local Linux environment; no native success is claimed. An
actual native run is also needed to establish the precise original exception.
The existing earlier Windows wrapper and other native fixture passes are retained;
this does not reopen them. Likewise both Mac package jobs in this run succeeded;
minimum-OS acceptance remains separate from hosted-runner package success.

Only src/main.rs's cfg(test) gate, the Windows-only test fixture file and this
report changed. All production security checks, Windows owner/ACL policy, SQLite
safeguards, accepted GUI code, historical recovery metadata and existing worktrees
remain unchanged. No commit, push, merge, Actions dispatch/rerun/monitoring, full
build, package build, release, live payroll or native-vault access was performed.
