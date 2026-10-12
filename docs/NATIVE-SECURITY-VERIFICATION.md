# Development 1.0.7 Phase 1 native verification

Prepared locally on `security/v1.0.7-phase1`, based on `46f49ba`, without a
commit, push, workflow dispatch, tag, package or release. Existing Phase 1 code,
GitHub README edits, published links and stashes are retained. This is a testing
branch, not a declaration that the open security findings have been fixed.

## Workflow and isolation

`.github/workflows/phase1-security-native.yml` has only `workflow_dispatch`.
It grants only `contents: read`, pins checkout/setup-python actions, disables
persisted checkout credentials, requires the exact testing branch and full
reviewed commit SHA, and uses Windows 2022 plus macOS 15 ARM64 and Intel runners.
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
| Windows owner SID | PowerShell queries actual new directory/file owners and compares with current token; no reliance on OW | Positive creation check only. Elevated runner default ownership can differ; investigate any failure rather than weakening assertion |
| Windows ancestor access | Broad synthetic ancestor above protected database parent must be refused before file creation | Expected to fail until ancestor validation is implemented |
| NTFS inheritance | Ordinary fixture inherits broad ACL; explicit private file excludes it | Fixture refuses non-NTFS; no silent skip |
| Windows reparse points | Synthetic junction cannot redirect creation; target bytes stay intact | Does not prove freedom from concurrent ancestor swaps |
| Windows read-only attributes | Kept/persisted staging must retain deliberately copied read-only policy and private DACL | Expected to fail because tempfile resets attributes |
| Windows failure preservation | Read-only destination refuses atomic replacement without changing original bytes/policy | Native result outstanding |
| macOS ACL entries | Inherited allow must be reported even with 0600 mode; existing inheritable ACL test also checks comma-suffixed entries | Expected failures in current text parser; tests do not repair it |
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

`cargo check --locked --offline` passed. Linux Phase 1 tests now report 15 passed
and 0 failed, including the reproduced cleanup regression and additional
directory/symlink/ancestor substitution and fail-closed coverage. The previous 886-test
full-suite result predates these verification fixtures; it is not a green result
for this working tree. Backup tests passed 8/8, database-recovery tests 29/29,
and recoverable-filing tests 31/31. Runner accounting tests passed 5/5.
Targeted formatting and `git diff --check` pass. Repository `cargo fmt --check`
still fails on 38 pre-existing formatting sections. Ruby parsed the workflow
YAML and checked manual-only triggers, read-only permissions, the three-runner
matrix and full-SHA action pins; actionlint is unavailable and GitHub has not
validated the workflow. Clippy
still fails on the unchanged `config.rs` non-octal test permission literal;
Linux emits 65 normal / 99 test warnings, without a new storage warning.
No native Windows/macOS build or workflow run has been executed locally.

## Review, commit and push only the testing branch

Run these commands only after reviewing the implementation and verification
diffs, including untracked files. The existing security implementation must be
included: committing only the workflow/tests onto clean 46f49ba would omit the
private-storage module and its dependency. This explicit allowlist reflects the
current inspected tree; do not replace it with `git add .` or force-push.

```bash
test "$(git branch --show-current)" = security/v1.0.7-phase1
git status --short
git diff --stat
git diff --check
git diff
git add -- Cargo.toml Cargo.lock README.md build.rs \
  docs/DEVELOPMENT.md docs/DOMAIN.md docs/PROJECT_STATE.md \
  docs/SECURITY-HARDENING.md docs/NATIVE-SECURITY-VERIFICATION.md \
  src/application_settings_screen.rs src/archive.rs src/backup_service.rs \
  src/config.rs src/database.rs src/database_recovery.rs src/environment.rs \
  src/main.rs src/payroll_archive_service.rs src/payroll_filing.rs \
  src/payroll_filing_tests.rs src/payroll_replacement.rs \
  src/payroll_snapshot_service.rs src/pdf_generator.rs src/signature.rs \
  src/timesheet_delivery.rs src/private_storage.rs \
  src/private_storage_linux.rs src/private_storage_macos.c \
  src/private_storage_macos.rs src/private_storage_windows.rs \
  src/private_storage_windows_tests.rs \
  .github/workflows/phase1-security-native.yml \
  scripts/phase1_security_checks.py scripts/test_phase1_security_checks.py
git diff --cached --check
git diff --cached --stat
git diff --cached
test "$(git branch --show-current)" = security/v1.0.7-phase1
git commit -m "Prepare v1.0.7 Phase 1 native security verification"
git push --set-upstream origin HEAD:refs/heads/security/v1.0.7-phase1
```

Verify the intended `origin` repository through your Git configuration before
pushing. These commands do not merge into main, push tags or dispatch workflows.
Stashes are not needed and must not be applied or dropped. Review known-failing
tests as verification evidence, not a production-ready change.

## Minimal default-branch registration plan (approval required)

No step in this plan has been executed. Keep this working tree on the security
branch. First separately approve, review, commit and push its complete testing
revision using the allowlist above. Capture its full SHA. Registration must not
cherry-pick that commit or merge the security branch into main.

1. Fetch the latest remote default branch and inspect its workflow triggers
   again. Confirm the default branch is still `main`; otherwise revise this plan.
2. Create a separate clean worktree and a `ci/phase1-native-registration` branch
   based on `origin/main`. Do not switch this security workspace or use stashes.
3. Copy **only** `.github/workflows/phase1-security-native.yml` from the reviewed,
   committed security SHA into that clean worktree. The default branch does not
   need any application/version/security/test/script/documentation changes.
4. Stage that one file. Require the entire staged filename list to equal that
   exact path; inspect its complete diff and run the workflow guard/static checks.
5. Only with separate approval, commit and push the registration branch, review
   a one-file PR to main, and merge that one-file registration. Do not merge the
   unfinished implementation or invoke any packaging workflow.
6. Confirm the workflow is registered on main, but dispatch it with the security
   branch selected and its full reviewed SHA supplied. A dispatch selecting main
   will fail the first step before checkout/toolchain/test execution.

Possible commands for the separately approved registration work (not executed):

```bash
git fetch origin
reviewed_security_sha=$(git rev-parse security/v1.0.7-phase1)
registration_dir=$(mktemp -d "${TMPDIR:-/tmp}/dpt-native-registration.XXXXXXXX")
git worktree add -b ci/phase1-native-registration "$registration_dir" origin/main
mkdir -p "$registration_dir/.github/workflows"
git show "$reviewed_security_sha:.github/workflows/phase1-security-native.yml" \
  > "$registration_dir/.github/workflows/phase1-security-native.yml"
git -C "$registration_dir" add -- .github/workflows/phase1-security-native.yml
test "$(git -C "$registration_dir" diff --cached --name-only)" = \
  .github/workflows/phase1-security-native.yml
git -C "$registration_dir" diff --cached --check
git -C "$registration_dir" diff --cached
git -C "$registration_dir" status --short
# Review and separately approve this exact one-file diff before these writes:
git -C "$registration_dir" commit -m "Register manual Phase 1 native security verification"
git -C "$registration_dir" push --set-upstream origin \
  HEAD:refs/heads/ci/phase1-native-registration
```

The first security commit must actually contain the workflow; current HEAD
46f49ba does not. After the one-file PR is approved and merged, dispatch commands
below target the already reviewed security commit. The workflow definition at
that selected ref must also be reviewed, not merely the registration copy.
`github.sha` is the selected-ref event SHA: the guard matches it to the supplied
40-character lowercase SHA and checkout uses that immutable SHA, never main or
a moving branch name. The runner then checks the checked-out HEAD again. A
branch movement before dispatch invalidates the supplied SHA; movement after
the event does not change the checked-out revision.

## Manual-dispatch prerequisite and inspecting results

GitHub requires a workflow-dispatch workflow to exist on the default branch.
Pushing this new workflow only to the testing branch will not satisfy that
requirement. See [GitHub's manual-run documentation](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow).
No default-branch change is authorized or made here. The one-file plan above
needs separate approval. Do not dispatch a packaging workflow
as a workaround. Runner privilege limits are documented by
[GitHub](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).

Only after that separately approved registration and the testing-branch push:

```bash
phase1_sha=$(git rev-parse HEAD)
gh workflow run phase1-security-native.yml \
  --ref security/v1.0.7-phase1 -f expected_sha="$phase1_sha"
gh run list --workflow phase1-security-native.yml \
  --branch security/v1.0.7-phase1 --limit 5
gh run view RUN_ID --log-failed
gh run view RUN_ID --log
```

Alternatively select **Phase 1 native security verification** in Actions,
select the exact testing branch, and enter the full reviewed SHA. Inspect all
three runner jobs, compilation output, test discovery counts and every group's
summary, including groups after the first failure. Match the logged SHA to the
reviewed commit. The workflow deliberately uploads no fixtures or binaries.
