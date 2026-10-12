# Development 1.0.7 Phase 1 native verification

The current corrections are uncommitted on `security/v1.0.7-phase1` at HEAD
`6329b6d543ff4d6428de83242d9a8202648b4017`. The workflow was registered on main
and run 38264276658 tested that approved source. These corrections have not been
committed, pushed or dispatched; no tag, package or release is prepared.
Existing Phase 1 code,
GitHub README edits, published links and stashes are retained. This is a testing
branch, not a declaration that the open security findings have been fixed.

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

## Coverage and honest result interpretation

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

## Local checks on this revision

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

The explicit allowlist below is exactly the 20 modified files relative to HEAD
`6329b6d543ff4d6428de83242d9a8202648b4017`, including all fixture consumers of
PrivateTempDir. Do not stage a partial fixture migration, use `git add .`, or
include unchanged README, workflow, dependency or release files. Reconcile this
list again if the working tree changes. These are proposed commands only; no
staging, commit or push is authorized by this documentation update.

```bash
test "$(git branch --show-current)" = security/v1.0.7-phase1
test "$(git rev-parse HEAD)" = 6329b6d543ff4d6428de83242d9a8202648b4017
git status --short
git diff --stat
git diff --check
git diff
git add -- docs/NATIVE-SECURITY-VERIFICATION.md \
  docs/SECURITY-HARDENING.md \
  src/backup_service.rs \
  src/database_recovery.rs \
  src/payroll_archive_tests.rs \
  src/payroll_document_import_tests.rs \
  src/payroll_evidence/stage4_tests.rs \
  src/payroll_evidence/stage7_tests.rs \
  src/payroll_evidence/tests.rs \
  src/payroll_filing_tests.rs \
  src/payroll_production_tests.rs \
  src/payroll_replacement.rs \
  src/payroll_sickness_editor.rs \
  src/payroll_timesheet_screen.rs \
  src/personal_assistant_status_tests.rs \
  src/private_storage.rs \
  src/private_storage_macos.c \
  src/private_storage_macos.rs \
  src/private_storage_windows.rs \
  src/private_storage_windows_tests.rs
git diff --cached --name-status
git diff --cached --check
git diff --cached
```

Require the staged inventory to contain exactly those 20 paths and review the
complete staged diff before separately approving a commit. No application
changes may enter main as part of workflow registration. Preserve both safety
stashes and all unrelated changes. Any future commit/push must target only the
security testing branch and requires separate authorization. This is an isolated
verification candidate, not production security sign-off.

## Registered workflow and future source-pin approval

The workflow is already registered on main. Do not repeat initial registration
or copy the older workflow from the security branch over the registered main
workflow: its dispatch guards differ. The currently approved source SHA is
`6329b6d543ff4d6428de83242d9a8202648b4017`; it excludes the uncommitted corrections.

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
   on main at or after registration merge 33c366439265b31fcb32c95739efb3e0580c3763,
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

Read the uncommitted remediation record in [SECURITY-HARDENING.md](SECURITY-HARDENING.md).
The old approved SHA remains pinned; it does not include the following changes.
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
