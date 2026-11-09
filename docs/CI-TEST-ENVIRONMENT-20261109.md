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


## Standalone Windows owner investigation — run 42627247019

Base: `bcd6ff32c4a084c0af777934965be7128ce2b57e`; existing failed job
[129194167358](https://github.com/arnieSkyNet/DirectPaymentTimesheets/actions/runs/42627247019/job/129194167358).
Only the existing job log was inspected; no Actions run was triggered or monitored.
The recorded outcome is 1149 passed, 1 failed, 7 ignored. The sole failure is the
owner fixture at stage get_acl, category exception, HRESULT -2146233087.

### What is established, and what is not

The failure occurs before GetOwner/current-token comparison, on Get-Acl itself.
The same-file assertion completed and the Rust fixture supplied the ordinary,
identity-checked drive spelling. Removing the verbatim prefix did not fix this
run, so a prefix-only explanation is insufficient. The package wrapper's private
NTFS/default-owner preflight passed. The Rust helper invokes Windows PowerShell
powershell.exe with NoProfile/NonInteractive; the package wrapper runs under
PowerShell 7. These are different .NET/provider execution contexts.

The recorded HRESULT does not determine the exception type or root cause.
SystemException and wrappers can carry generic HRESULTs and inner exceptions;
Microsoft's [SystemException implementation](https://github.com/dotnet/runtime/blob/main/src/libraries/System.Private.CoreLib/src/System/SystemException.cs)
illustrates why the number alone is insufficient. The
[Get-Acl implementation](https://github.com/PowerShell/PowerShell/blob/master/src/Microsoft.PowerShell.Security/security/AclCommands.cs)
uses provider existence/security-descriptor operations. Current PowerShell source
requests owner/group/access by default, with audit added only when selected;
that is useful context, not proof about the installed Windows PowerShell 5.1
implementation or evidence that this exception is a missing privilege.
No raw exception type, inner exception or message was retained in the old marker.
No permission repair, privilege enablement, shell substitution, Get-Acl fallback,
owner exception or production-policy change is justified by that evidence.

### Prepared changes

- Extract the same owner query into scripts/windows-owner-query.ps1, included
  directly by the Rust test and used verbatim by the standalone verifier. The
  successful query and required True owner result are unchanged.
- Add bounded redacted diagnostics: actual exception kinds (fixed allowlist),
  up to four HRESULT levels, Win32 native codes where the exception really is
  Win32Exception, and a fixed error-ID category. Preserve the legacy marker too.
  No type is inferred from an HRESULT. Raw messages, ErrorRecords, paths, SIDs,
  account names and stderr are never formatted into the public result.
- Add scripts/verify-windows-owner-fixture.ps1 and the small supporting
  scripts/windows-owner-fixture-native.cs. This is a synthetic diagnostic, not
  a substitute for application/native platform sign-off. It requires x64 Windows,
  PowerShell 7 and NTFS, and creates only new private random-named fixture objects.
  It does not build the application or invoke Cargo or the package wrapper.
- Reuse only the existing package wrapper's C# default-owner helper to match
  the package job's child context. The verifier restores the prior default owner
  in finally and changes no privileges, group membership or existing ACLs.
- Independent Win32 reads check actual current-user ownership and a protected
  current-user/System-only full-control DACL. Native file identity and privacy
  are checked before and after each query. These checks never become optional.
- Compare Windows PowerShell 5.1 and PowerShell 7, directory and file, ordinary
  and verbatim spellings: eight query cases. Four ordinary-path cases reproduce
  the current fixture requirement and must pass. Verbatim queries are explicitly
  labelled provider comparisons; their outcomes cannot satisfy a failed required
  query. A successful direct Win32 check cannot substitute for a failed Get-Acl.
- Record only sanitized shell/runtime version numbers, process bitness, filesystem
  provider/cmdlet/module booleans, current/default-owner equality and enabled admin
  role status. Both stdout and stderr are captured; only stderr presence is public.
  Bounded child shells are terminated on timeout. Synthetic objects are retained;
  no cleanup/delete operation is provided.

The mandatory same-file identity, exact current-token owner and private-ACL
assertions in the existing Rust fixture remain unchanged. All production Windows
owner/ACL/SQLite implementations, accepted GUI source and workflows are unchanged.

### Proposed short native command (not run here)

From a checkout containing these reviewed files on a windows-2022 x64 runner:

```powershell
pwsh -NoProfile -File scripts/verify-windows-owner-fixture.ps1
```

Only the small C# helper is compiled by Add-Type. No Rust application build,
package build, full suite or native vault operation is involved. Output is a
sanitized JSON report; nonzero exit means a required query or native invariant
failed. A subsequent reviewed manual-only entry point is documented below;
it has no automatic trigger. Execution needs separate authorization.

Interpretation: differing shell results isolate the provider/runtime context;
differing spelling results isolate path handling; successful Win32 owner/privacy
with failing Get-Acl isolates the query layer without proving which internal
provider operation failed. Inspect actual exception kinds, inner/native codes and
fixed error-ID category before proposing a fix. If the bounded chain still ends
in other/none or generic errors, the root cause remains unresolved. Never convert
that uncertainty into a security bypass or a claimed pass.

### Focused local validation

Evidence: /tmp/dpts-owner-native-42627247019/ (scratch, not durable storage).
The exact new diagnostic parser passed six tests, including legacy compatibility,
inner codes/types, identical HRESULTs with different actual types, malformed fields,
overflow, invalid UTF-8, private canaries and excessive output. The standalone C#
helper compiled on Mono. Its pure descriptor validator passed two positive and
nine unsafe controls (foreign owner, broad/reduced/denied/extra grants, unprotected,
missing-user/System, empty/null DACL). No Win32 calls were executed on Linux.
Initial scratch controls exposed Mono SDDL-parser limitations for empty/null DACL;
those controls were constructed directly as RawSecurityDescriptor objects instead.
The verifier's policy was not weakened. Rust syntax and wiring/privacy source
checks passed. PowerShell parsing/execution and native file-identity/owner/provider
behavior remain unverified because no PowerShell/Windows executor is available
locally. No full application compilation or suite was performed.

The original Get-Acl root cause remains unknown pending the native diagnostic.
No production data, real credential store, existing directory ACL or unrelated
worktree was accessed or modified. Historical recovery metadata is unchanged.
No commit, push, merge, Actions, package build, tag or release was performed.


## Manual-only owner verification entry point

Prepared `.github/workflows/windows-owner-fixture.yml` during the subsequent
review. Its only trigger is workflow_dispatch; the sole job uses windows-2022
(the verifier additionally refuses non-x64 contexts), a five-minute timeout,
contents:read, and checksum-pinned checkout with credentials not persisted.
The only run command is:

```powershell
pwsh -NoProfile -File scripts/verify-windows-owner-fixture.ps1
```

There is no Rust compilation/suite, installer/package build, artifact upload,
cache, publisher, release or automatic push/pull-request trigger. The standalone
verifier's tiny C# Add-Type compilation remains necessary for the independent
Win32 controls. Existing release workflows are unchanged. No workflow was
triggered or monitored during review; native execution remains unverified and
requires separate authorization. Historical recovery metadata is excluded.
