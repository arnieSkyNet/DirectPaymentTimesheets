# Prepared macOS SMTP verification — 19 October 2026

Status: **no macOS native test result yet**. Published integration `d3006b7` and
main registration `ef4ffcd` enabled run 39332360502, but both jobs failed at
historical-source checkout before testing. See
`../macos-smtp-actions-v107-20261019-2/REPORT.md` for actual results.

The source-acquisition correction described below is locally prepared and
unpublished. It makes no new commit, push or workflow request. See
`../macos-smtp-source-acquisition-fix-v107-20261019-1/REPORT.md` for fresh-fetch
evidence, checks and independent review. Existing Windows/Debian native evidence
is preserved; no VM, host service or host credential store is accessed.

## Source finding and execution boundary

Before integration, the local `security/v1.0.7-phase1` checkout was `e581130`, with no
`src/smtp_credentials.rs` or keyring dependency. It must not be presented as the
encrypted implementation tested on Debian. The preserved Debian report
`native-linux-smtp-v107-20261019-1/REPORT.md` identifies reviewed bundle commit
`77d3268a5a17b4585371a249135a0cceecfbe00b`: 78 ordinary unit tests passed,
one native test ignored, and 24 separate real Secret Service probes passed.
Those results establish Debian amd64 evidence, not macOS coverage. Historical
Windows verification remains as documented in `NATIVE-SECURITY-VERIFICATION.md`;
this preparation makes no new Windows acceptance claim.

The new, separate `macos-smtp-keychain.yml` workflow preserves existing workflows.
It is manual only, contents-read only, with pinned actions and no secret inputs,
caches, artifact uploads, packaging or release. Its harness comes from the
dispatch commit; corrected application acquisition is pinned to the published
integration commit `d3006b7c526042a7ea5a2ab5f108a2f0d393d5dd`, never a branch tip.
The runner verifies that exact commit and SHA-256
`5ff10aa9d5920f88dff2a2988901e17dcfebc480d9cb51bf583c0d994e77e0bc`
of the credential module before injecting test-only code inside its existing
test module. Production credential code is unchanged. The module is restored
after execution; interrupted jobs rely on disposal of the hosted runner.

An earlier successful fetch into an already populated inspection clone was
insufficient proof of fresh remote availability. Both Actions runners, and a
fresh local fetch, reject the historical bundle SHA `77d3268` with `not our ref`.
It is outside the published development commit's ancestry. GitHub's commit API
reports no commit for that SHA; no server-retention explanation is assumed.

The correction retains `77d3268` and its original Git tree as provenance, backed
by a hash-pinned `scripts/macos_smtp_reviewed_source.json` generated from its
Git objects. All 111 original build-input paths, contents and executable modes
must match in both the fetched source and dispatch harness before their existing
mutual equivalence check and the test overlay. The manifest covers complete
`src`, assets, Cargo manifests/lock, build script and Windows resource input.
Extra/missing/changed files, symlinks, changed manifest bytes or provenance,
wrong acquisition SHA and dirty source checkouts are refused. Matching drift in
both checkouts cannot replace the independent original hashes. A fresh remote
checkout of `d3006b7` and all these local checks passed.

## What the GitHub jobs test

The matrix selects `macos-15` ARM64 and `macos-15-intel` x86-64. The runner
positively verifies native architecture and GitHub-hosted macOS before accessing
Keychain. Both compile the pinned application test binary using Rust 1.98.1 and
Cargo.lock. Compilation failures are failures, not skipped native tests.

The ignored, explicitly selected Rust fixture calls the concrete macOS keyring
backend through the application's `native_entry` helper, using UUID accounts in
`DirectPaymentTimesheets.Tests.macos.smtp`, never the production service name.
It tests:

- Real native migration of a synthetic legacy config to an opaque reference,
  correct password retrieval and unchanged-save reference reuse.
- Real retrieval in a fresh child process; unrelated configuration metadata
  survives migration.
- Native missing/deleted-entry errors and refusal to use cached passwords.
- **Mocked** denied-provider/migration failures, retained legacy input and cleared
  in-memory password. These are not real Keychain locking/ACL denial tests.
- No synthetic canary in post-migration configuration, generated backup or
  captured test/build output. The known legacy input intentionally contains a
  synthetic plaintext password before migration. Fixture source and compiled
  binaries necessarily contain synthetic constants; they are not published and
  are not claimed to be plaintext-free.
- Scoped deletion and absence verification for created entries. Failure to
  confirm deletion fails the test. A timeout kills the test process group and
  reports Keychain cleanup **unknown**, with hosted-runner disposal as fallback.

No Keychain search list, default keychain, locking state or host service is
changed. The real backend uses the hosted runner's user Keychain with the test
namespace. This runner is intentionally refused on a developer's local Mac.
No real credentials or payroll data are provided to the jobs.

## Local validation and limits

Initially seven Python harness tests passed: wrong platform refusal, module hash refusal,
overlay placement/reapplication refusal, nonzero exit, secret-output rejection,
timeout and successful capture. Ruby Psych parsed the workflow and checks passed
for manual trigger, read permissions, architecture matrix, action SHA pins and
`bash -n` shell syntax. Rustfmt parsed/checked the Rust fixture successfully.
No actionlint executable was available; these checks are not a claim of full
GitHub workflow schema validation. Native macOS typechecking/build and execution
remain for GitHub; no mocked result is counted as Keychain acceptance.

The workflow has a 55-minute job limit, 45-minute native step, 40-minute build
deadline and 180-second native fixture deadline. Captured diagnostics are
withheld to avoid assertion/provider disclosure; jobs report fixed failure
categories rather than raw logs. This reduces build-failure diagnostic detail.

Fresh-process persistence is planned by the fixture; reboot/login persistence, GUI prompts,
actual locked/denied Keychain behaviour, code-signing/update ACL changes and
interactive app behaviour are not. Headless runners may fail because a user
Keychain cannot be accessed without interaction; no unlock workaround or mock
fallback hides that failure. Full filesystem, crash/core-dump and OS Keychain
internal audits are outside this focused fixture. ARMv7/Linux and Windows ARM64
coverage are not established by these macOS jobs.

The acquisition correction has 13 passing local harness tests, including
manifest and matching-drift adversarial cases. The original native fixture and
application source are unchanged. This is not a macOS native PASS.
Next approval: publish the reviewed acquisition correction. Any main workflow
synchronisation should remain workflow-only. A new execution requires separate
explicit approval; no approval is inferred from preparation.

Runner references checked against official GitHub documentation:
https://docs.github.com/en/actions/reference/runners/github-hosted-runners
and https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/choose-the-runner-for-a-job
