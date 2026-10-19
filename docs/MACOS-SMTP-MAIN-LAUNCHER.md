# Main-only macOS SMTP Keychain launcher — preparation

This candidate runs only after manual dispatch from main. It tests published
application commit `d3006b7c526042a7ea5a2ab5f108a2f0d393d5dd` on separate
`macos-15` ARM64 and `macos-15-intel` x86-64 jobs. Main supplies test control files,
not the application under test. No application merge or development-branch
change is required. Publication and execution require separate approval.

## Three explicit checkouts

1. `launcher`: the exact main dispatch event SHA (`github.sha`), containing this
   workflow, the corrected runner, original-review manifest and launcher tests.
2. `harness`: immutable `d3006b7`, supplying the application context and existing
   synthetic native fixture.
3. `source`: an independent pristine checkout of the same immutable `d3006b7`,
   supplying the actual application compiled by the native runner.

The workflow rejects other refs/events before acquisition. It checks out no
historical `77d3268` object, branch tip or fallback. The historical commit/tree
remain provenance inside the independently reviewed, hash-pinned manifest.
All runtime files must come from the future published main commit or d300;
no local checkout, uncommitted input or new remote reference is needed at run time.

## Checks before native execution

The new `scripts/macos_smtp_launcher.py` requires clean, distinct checkouts;
controller HEAD must equal the full event SHA and both application HEADs must
be d300. Git queries have ten-second deadlines. Root and support-file symlinks
are refused. It loads the independently reviewed corrected runner from main.

Before any copy, both d300 trees must match all **111** original reviewed file
paths, SHA-256 values and Git executable modes in
`scripts/macos_smtp_reviewed_source.json`. Its pinned SHA-256 remains
`8972066d653ef483a161aabb8f0ba5c5a596f6a87b65ea690437b5afff17142c`.
Original provenance remains commit `77d3268a5a17b4585371a249135a0cceecfbe00b`,
tree `4748f5d329a8d6a029ee2b802240a930d256b21e`. Acquired identity remains
d300, explicitly distinguished from that historical commit. Contents/modes
were independently compared from both sets of Git objects during preparation.
Mutual application-tree equality and the runner's separate module hash remain.

The native fixture is taken from d300, never supplied or rewritten by main.
Its additional pinned SHA-256 is
`907a05c66bc6fd1e55d433d5e98ccebfb78f450a3658c75863e19ec6b015f590`.
Only three support files are copied main -> harness after verification:
`macos_smtp_checks.py`, `macos_smtp_reviewed_source.json` and
`test_macos_smtp_checks.py`, all inside scripts. The helper checks copied bytes,
fixture preservation, the 111-input inventories again and pristine source HEAD
and status. Main's application inventory is intentionally never asserted to be
reviewed application code.

The unchanged corrected runner then runs from harness against source. Thus its
existing full input comparison, exact source HEAD/clean checks, original manifest
checks, module hash and test-only overlay work without modification. It refuses
Linux/local host use and requires the expected native hosted macOS architecture.

## Security and evidence boundary

The workflow remains workflow_dispatch only, contents-read only, with pinned
actions, no repository secret inputs, persisted Git checkout credentials,
caches, artifact uploads, packaging or release. Job/build/native deadlines are
unchanged. Existing other workflows and application source are untouched.

The d300 Rust fixture creates UUID accounts only in
`DirectPaymentTimesheets.Tests.macos.smtp`, using synthetic constants. Native
migration, retrieval, fresh-process persistence, missing-entry refusal and
cleanup are distinguished from mocked provider denial. Config/backup/captured
output canary checks remain scoped; the fixture does not audit the entire
runner or all child stderr. Checkout cleanup is not Keychain cleanup proof.

No native macOS result follows from local tests. Headless Keychain availability,
real lock/ACL denial, GUI prompts, signing/upgrades and login/reboot persistence
remain limits. The retained runner deliberately withholds raw build/provider
diagnostics on failure; a future exact runtime diagnosis may need separately
reviewed safe logging. No bypass, retry or plaintext fallback is added here.

## Local preparation evidence

The candidate has only seven file changes: this workflow, this document, the
three previously reviewed support files and the new launcher/helper tests.
The 13 existing harness tests plus launcher tests validate gating, SHA/clean
checks, deadlines, manifest/fixture drift, symlink/root rejection, exact copy
scope and source preservation. These use synthetic files/mocks and are not
Apple Keychain integration tests.

A local handoff simulation used real clean d300 source/harness checkouts and
real 111-input verification; only the unpublished controller's Git identity was
mocked. Exactly three test-support files changed; original source stayed clean
and the native fixture stayed identical. This is not a claim of full GitHub
execution: a real clean published-controller check is impossible before an
authorised commit. Runtime has no bypass option for that check.

See the separately preserved preparation report and independent review under
`macos-smtp-main-launcher-v107-20261019-1` in the Florence workspace. Next step
is approval to publish this main-only candidate, followed by separate approval
to dispatch its verified published main commit. Development dispatch is refused.
