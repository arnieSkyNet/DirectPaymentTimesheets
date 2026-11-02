# v1.0.7 email-security acceptance — 2026-11-02

Automated acceptance: PASS. Full Linux regression: **1,030 passed, zero failed,
one intentionally ignored native credential-store roundtrip**, no filtered tests.
Production debug build, cargo check and git diff --check passed. Existing dead-code
warnings remain. Targeted selections overlap the full suite and are not additive.

| Requirement | Automated result | Evidence and scope |
| --- | --- | --- |
| Authenticated SMTP requires TLS | PASS | Password and XOAUTH2 helpers refuse cleartext AUTH; remote plaintext configuration fails before vault/socket access. Synthetic STARTTLS/implicit TLS and bad-certificate/hostname tests pass. Gmail/GMX/custom password usernames covered. |
| Unauthenticated localhost relay | PASS | Actual Local SMTP Server branch exercised against a loopback sink on a test-only ephemeral port. Remote host, username and unavailable-credential state cannot trigger authentication. Production remains localhost:25 with no runtime override; no real port-25 service was contacted. |
| Real-document test confirmation and integrity | PASS | Synthetic document matrix opens the actual review without sending automatically; changed recipients block confirmation; changed registered bytes refuse capture and changed legacy unregistered bytes invalidate the captured review. Final MIME uses captured bytes. Failed capture clears retained review/message state. Synthetic defaults remain attachment-free. |
| Cooldown and volume safeguards | PASS | Boundary checks at 30 seconds, 100 attempts/600 seconds, 10-message acknowledgement and 50-message cap; rejected attempts do not extend cooldown. Actual send entry points enforce the shared limiter before transport. Selection changes reset large-batch acknowledgement. |
| Microsoft local cleanup | PASS with mock stores | Known generations removed, unrelated references retained, wrong-account/corrupt-chain checks fail safely, partial deletion after manifest/first-chunk removal is retryable, restored references stay blocked and backend errors are redacted. No native credentials were read or deleted. |
| Save dirty-state highlighting | PASS headlessly | Actual painted controls and synthetic save lifecycles cover application/payroll/email settings, employer signature/payroll address, employer/PA, pay rates/hours basis, running notes, payroll preparation/returned hours, mileage and sickness. Save-before-navigation regression and shared helper tests pass. All seven themes and disabled/inherited-disabled states covered. Failed saves retain dirty state; successful saves reset it. |

Confirmed defects fixed during this pass:

- Employer/PA rendering materialises absent optional fields as empty drafts. Strict
  comparison could highlight Save without a user edit. Dirty comparison now treats
  absent and empty optional contact/date values equivalently; persisted records are
  not changed by this comparison. Real field/status edits remain dirty.
- Sickness dates displayed after saving can differ textually from stored ISO dates.
  Save now uses the same resolved-date comparison as the editor's unsaved-change
  guard, avoiding a false blue state after saving/equivalent date formatting.
- A failed document-test capture now releases the retained review/message state.

The test harness settles modal paint without animation and never opens the real
GUI. Credential operations use in-memory mocks. SMTP fixtures use loopback sinks
with fictional addresses; they do not submit mail to a provider or real relay.
Existing working changes/stashes were retained. The running isolated GUI and its
profile were not restarted or changed. No OAuth, Azure operation, commit, push or
release was performed. No live payroll records or native SQLite terminal fixtures
were accessed.

## Native/live gates — NOT TESTED in this run

- The explicitly ignored `smtp_credentials::tests::native_store_synthetic_roundtrip`
  requires separate approval for disposable native-store writes/readback/deletion.
- Native Microsoft cleanup, partial failures, locked/denied stores, account ACLs,
  restart and concurrency behaviour need disposable-entry acceptance on Windows
  Credential Manager, macOS Keychain and persistent Linux Secret Service. Mock
  success does not establish these backend guarantees or cross-platform build signoff.
- Microsoft/Gmail/GMX live SMTP authentication/submission/delivery and Microsoft
  refresh/revocation need separate explicit authorization; no real email was sent.
- Native desktop visual, keyboard/focus and DPI acceptance remains manual. Headless
  paint coverage is not a claim of native visual signoff.

Release assessment: **NO-GO for release signoff until applicable native/platform
credential and live-provider acceptance is approved and completed.** No unresolved
automated failure remains. Historical unindexed/unreachable token orphans and
already-held tokens cannot be erased/revoked by local cleanup; Microsoft consent
revocation is separate. Process-local limits and UI approval are not a security
boundary against an agent or modified process controlling the credential-owning
account. See EMAIL-ABUSE-SAFEGUARDS.md for these deliberate limits.

Evidence logs for this run: `/tmp/dpt-v107-acceptance-3pnulqyl/` (full.log,
security.log, confirmation-r2.log, all-screens-r3.log, check.log, build.log).

## Native acceptance continuation — 2 November 2026

**Overall release gate: NOT COMPLETE / NO-GO.** The earlier full-suite result is
retained as the baseline; the additions below are test-only and do not change
production behaviour. No confirmed production defect was found in this pass.

| Current candidate / platform | Result | Exact scope / blocker |
| --- | --- | --- |
| Florence, Linux x86_64, native persistent Secret Service | PASS | Existing native synthetic roundtrip plus new cross-process SMTP-password/token persistence and actual OAuth cleanup fixture. Two native parent tests and one child execution passed. Both recorded generations' token chunks and manifest were removed, cleaned reference could not be read/reused, repeat cleanup succeeded, unrelated synthetic authorization remained readable. Teardown verified removal of every entry written by this fixture, including its metadata/tombstone. Only unique synthetic namespaces were accessed. |
| Florence, native X11 GUI at 100% and explicit 150% scale | PASS, bounded visual scope | Screenshots inspected for all four Email Settings tabs, enabled blue Save after a noncredential option edit, normal Save after successful saving. No observed clipping in these tested layouts. 150% is a launch-time X11 scale override, not a system display setting or mixed-monitor DPI transition test. |
| Native confirmation / real-document review; other screens' native Save lifecycle | NOT COMPLETE | Earlier headless synthetic confirmation/all-screen lifecycle coverage remains PASS. This run did not open native real-document or destructive-cleanup confirmations. No native modal, keyboard/focus, full theme/large-font or mixed-DPI signoff is asserted. Navigating away from an Email Settings draft retains that draft; the capture named unsaved-confirmation.png is an employer screen, not a confirmation pass. |
| Molly, Windows x86_64 | BLOCKED | The existing Windows host resolved; strict-host-key SSH authentication was refused with Permission denied (publickey,password,keyboard-interactive). No host-key override, guessed password, access repair, transfer, VM change or remote execution occurred. This does not establish that Molly is offline. No current Windows credential, compilation or GUI pass is claimed. |
| macOS arm64 and x86_64 | PREPARED, NOT RUN | Read-only GitHub API confirmed active workflow 389909211 (.github/workflows/macos-smtp-keychain.yml). It is manual-only, contents:read, pinned checkouts, no publishing/secrets/artifact upload. However source is fixed at d3006b7c526042a7ea5a2ab5f108a2f0d393d5dd with an older reviewed manifest; it cannot test current uncommitted OAuth cleanup. It was not dispatched or edited. The older phase1-security-native workflow likewise pins older source and includes unrelated security work, so was not used. |

New ignored, explicit native fixture:
`microsoft_oauth::tests::native_synthetic_persistence_and_cleanup` (invokes its
ignored child by exact name). It exercises the current token write/read/cleanup
functions through concrete native keyring backends, a test-only service
`DirectPaymentTimesheets.Tests.v107.acceptance`, and a fresh UUID namespace. The
child receives only the nonsecret namespace; neither credentials nor token bodies
are passed on the command line. No network or OAuth endpoint is involved. An RAII
tracker records writes before backend calls and verifies deletion on completion.
It does not enumerate any credential collection or production service. Existing
profiles, backups, Microsoft authorization and stashes were not changed.

Affected automated regressions: **30 OAuth tests PASS; 8 SMTP credential tests
PASS; cargo check --offline --bin direct_payment_timesheets PASS; git diff --check
PASS.** One initial sandboxed regression failed at creating its loopback listener
with Operation not permitted; its authorized rerun outside that network restriction
passed all eight tests. Existing dead-code warnings remain. The 1,030-pass baseline
was not repeated: only ignored native acceptance fixtures/helpers were added.

Evidence:

- `/tmp/dpt-v107-native-acceptance-wqj5jszw/`: 100% native screenshots, isolated
  synthetic GUI profile, candidate-build-inputs.json and candidate-build-inputs.tar.gz.
- `/tmp/dpt-v107-dpi150-jg642d2n/`: 150% native screenshots and second disposable profile.
- Terminal results for the two native parent tests and their child were observed
  directly; these are not represented as separately captured raw log files.
- The local source packet contains 112 source/build files, hashes, byte sizes and
  modes; no Git metadata, credential vaults, profiles, databases or payroll data.
  It is preparation only and has not been uploaded to GitHub or another machine.

### Prepared remaining platform procedure (not executed)

1. Provide an existing authorized Molly execution route; independently verify
   host/user and acquire the immutable, hash-verified current source packet into a
   fresh private synthetic workspace. Preserve all existing Molly work/profiles.
   Use an admitted user-owned temporary root for native fixtures; do not change
   existing ACLs, production paths or development profile ownership.
2. For macOS, approve a secure immutable delivery path for this exact uncommitted
   source and a narrowly reviewed update to the dedicated manual SMTP workflow's
   source/manifest pins. No commit/push/workflow mutation is authorized by this
   preparation. Keep contents:read and no publisher, cache, secrets, artifacts,
   packaging, release or terminal SQLite fixture steps. Both macos-15 (arm64) and
   macos-15-intel (x86_64) must report architecture and verify the packet hashes.
3. In each admitted native workspace run the following exact selected tests; do
   not run all ignored tests, the child directly, or native SQLite fixtures:

   ```text
   cargo check --locked --bin direct_payment_timesheets
   cargo test --locked --bin direct_payment_timesheets smtp_credentials::tests::native_store_synthetic_roundtrip -- --ignored --exact --test-threads=1
   cargo test --locked --bin direct_payment_timesheets microsoft_oauth::tests::native_synthetic_persistence_and_cleanup -- --ignored --exact --test-threads=1
   cargo test --locked --bin direct_payment_timesheets microsoft_oauth::tests -- --skip native_synthetic
   cargo test --locked --bin direct_payment_timesheets smtp_credentials::tests -- --skip native_
   ```

   The fixture uses each target's concrete Secret Service/Credential Manager/
   Keychain backend; success on Florence does not establish either other backend.
   Native denied/locked-store and concurrent-process refresh/cleanup acceptance
   remains separate; do not lock or change the user's real vault to simulate it.
4. GUI acceptance needs an interactive native session. Hosted macOS CI can compile
   and perform bounded automated GUI checks if prepared, but the existing SMTP
   workflow contains no GUI check and cannot establish human visual acceptance.
   In new synthetic profiles inspect all four tabs, normal/dirty/saved/failed-save
   button states, representative employer/PA/payroll screens, keyboard/focus,
   100%/150%/200% and monitor transitions. Exercise document-review and cleanup
   confirmations with synthetic fixtures, canceling before sending or any real
   credential deletion. Bind screenshots, binary and fixtures to source hashes.
5. Current-candidate supported architecture gaps: Windows x86_64 and arm64;
   macOS x86_64 and arm64; Linux aarch64 and armv7. Florence only establishes
   x86_64 Linux within the scope above. Cross-compilation alone is not native
   credential or desktop acceptance. Native GUI confirmation and vault failure/
   concurrency/relogin coverage remains incomplete on Florence too.

Release blockers are therefore current-source Windows/macOS native execution and
GUI acceptance, remaining supported-architecture disposition, and the explicitly
uncompleted native confirmation/failure/concurrency scope above. Live provider
submission/refresh/revocation remains untested under the user's no-send/no-OAuth
restriction; any requirement for live-provider release acceptance needs separate
approval. Existing SQLite release-gate limitations in SQLITE-CONNECTION-OWNERSHIP.md
are unchanged and are not signed off by these email checks.

Both newly created synthetic desktop instances exited normally after inspection;
their profile/evidence directories remain. The existing Microsoft test GUI/profile
was not restarted or altered.

## Pre-push review — 2 November 2026

The current source passed a fresh full application suite:
`cargo test --offline --bin direct_payment_timesheets -- --test-threads=2`:
**1,030 passed, zero failed, three intentionally ignored native tests, zero
filtered out** (267.84 seconds). The two additional ignored tests are the new
native OAuth acceptance parent and its process-restart child; their earlier
explicit Florence execution is recorded above. Production cargo check and diff
whitespace checks also passed. No native vault operations were repeated by this
full suite. Platform/live acceptance blockers above remain open.

Outgoing source and documentation were checked for known PA identities, real
credentials, private keys, credential-bearing URLs, token-shaped secrets and the
isolated live application ID. No such secret was found; deliberate synthetic
canaries and fictional mailbox fixtures remain in tests. Unrelated untracked
investigation/evidence directories, screenshots, profiles, caches and backups are
excluded from the commits. Existing stash IDs were verified unchanged. An
unnecessary local Windows SSH account identifier was removed from this public
report; the authentication failure and its acceptance consequence are retained.
