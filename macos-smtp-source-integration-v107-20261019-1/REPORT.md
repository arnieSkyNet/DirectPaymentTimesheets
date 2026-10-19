# PASS — local reviewed source integration

19 October 2026, Florence/flo64. Integration and focused local checks passed;
**macOS native compilation and Keychain execution have not occurred**. No commit,
push, merge, release or workflow trigger was performed. No VM, production data,
credential store or host service was accessed or changed.

## Why the source differed

GitHub's `security/v1.0.7-phase1` advertises
`5abca6816b1a74aa6f41304a51fd5e4bb2128b82`. Local HEAD is
`e5811307706d62f8a1613e7ae84e15d0b7b3ed75`, its setup-guide-only child.
The reviewed encrypted source, `77d3268a5a17b4585371a249135a0cceecfbe00b`,
is another child of that same parent, with tree
`4748f5d329a8d6a029ee2b802240a930d256b21e`. The branch never included that
separate commit; there is no evidence that its credential module was removed.
Why the branch ref was never advanced is not inferable from these histories.

The object was absent from the primary repository object database, but present
in the preserved source bundle and the temporary inspection clone. An exact-SHA
HTTPS fetch from GitHub into that clone succeeded, proving GitHub can serve it
despite its absence from the advertised branch tip. The SSH query failed on the
host's existing SSH configuration permissions; nothing there was changed.
Sandbox HTTPS DNS was restricted; the authorised read-only HTTPS queries passed.

The original bundle is
`/mnt/ct1tb-root/VirtualBox-Testing/Runtime-Media-v107-20261017-1/allowlist/inputs/source.bundle`.
Its SHA-256 is
`8627108e497d900a074144cdb869966b121aedf8ab5f4b71b7a1c47343af258c`,
matching the preserved MEDIA-INVENTORY. MEDIA-PROVENANCE pins the same commit;
EXTRACTION-VERIFICATION records the same tree, 241 ancestry commits, strict fsck
and export comparisons. The inspection clone passed `git fsck --strict` again.
This establishes preserved-content and Git identity, not a new signature claim.
The reviewed commit records its Windows validation; the independent Debian
report separately identifies this exact source as actually executed.

## Exact integration and workflow changes

Applied the complete 31-file parent-to-reviewed-commit delta, after a successful
conflict check. The current branch's 72 setup-guide paths are disjoint and remain
unchanged. No cherry-pick, merge commit, branch move or object import was needed.
All 31 integrated paths match the reviewed commit byte for byte; see
INTEGRATED-FILES.json and the independent review.

The delta adds one shared `smtp_credentials.rs`, platform-specific dependencies
and the tested config/save/send/backup/GUI integration. It also retains the
reviewed Windows guarded publication, development isolation and related tests.
These coupled publication safeguards were kept together rather than creating a
partial second implementation. Cargo.lock and every Windows implementation file
match the reviewed source. The imported documentation is historical as of the
reviewed commit; this report and preserved Debian report give subsequent results.

The prepared workflow still checks out the exact reviewed commit, which is now
known to be available on GitHub. Its harness now first compares the integrated
dispatch checkout against that immutable source: all src files (including the
Apple C helper), assets, Cargo manifests/lock, build.rs and Windows resource input.
Extra/missing/changed files and symlinks are refused. The source checkout must be
clean before applying the hash-checked, test-only macOS fixture. This tests the
intended implementation while refusing to claim equivalence after further drift.
The earlier macOS preparation document was updated accordingly. Existing native
Windows/security and packaging workflows were not changed.

## Actual local results

| Check | Actual result |
| --- | --- |
| Credential unit regressions | 8 passed, 0 failed, 1 native test ignored |
| config::tests | 15 passed, 0 failed |
| backup_service::tests | 8 passed, 0 failed |
| email_settings::tests | 9 passed, 0 failed |
| Python harness regressions | 8 passed, 0 failed |
| Reviewed source equivalence | All 31 delta paths and complete workflow build inputs match |
| Workflow/static checks | YAML parse, manual trigger, read permissions, architecture matrix, action pins, bash syntax, Rust fixture formatting and git diff whitespace passed |

The first sandboxed credential run passed seven tests but failed the synthetic
loopback listener with EPERM at smtp_credentials.rs:498. The authorised rerun
outside that sandbox passed all eight. It still used the memory store, never a
host credential provider. The original failed-run log is retained, not erased.
The narrower config::tests filter intentionally differs from the prior broader
Debian config:: filter. Total new Rust unit passes: **40**, with one ignored
native smoke test. Successful compile/test results do not mean Keychain passed.
RESULTS.json records exact commands/results and the distinction from prior data.
No actionlint executable was available; Ruby/static checks do not establish full
GitHub workflow schema acceptance.

Historical evidence remains unchanged: Windows x64 records 961 unit passes,
zero failures, five ignored plus separate native migration/restart success;
Debian x86-64 records 78 unit passes and 24 real Secret Service probes. Neither
native procedure was repeated. Windows evidence is recorded in the reviewed
commit/docs; Debian evidence remains in native-linux-smtp-v107-20261019-1.

## Independent review and remaining gates

INDEPENDENT-REVIEW.md independently confirms the exact integration, all 72 guide
paths preserved, Windows code retention and fail-closed workflow source selection.
There is no identified integration blocker.

The workflow remains a manual ARM64/macOS-15 and x86-64/macOS-15-intel candidate.
Both require actual Apple SDK/C helper compilation and real hosted-runner
Keychain execution. Denied-provider scenarios are mocked; native missing-entry
checks and fresh-process persistence are real only when run on macOS. Headless
Keychain availability, real lock/ACL denial, signing/upgrades, GUI prompts and
login/reboot persistence remain unverified. Windows ARM64, Linux ARM64/ARMhf and
other unexecuted platform requirements remain as documented; no release is
approved by this integration.

Next required approval: commit/publish this reviewed local integration and
workflow. Triggering the macOS workflow requires explicit separate approval.
