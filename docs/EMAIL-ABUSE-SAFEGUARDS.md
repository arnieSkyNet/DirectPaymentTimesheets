# Email safeguards — v1.0.7 development

Authenticated SMTP always requires STARTTLS or implicit TLS. The shared dispatch
path rejects password authentication over plaintext before opening the vault or a
socket, and the AUTH helper checks encryption again. This applies to Gmail, GMX,
Microsoft and custom servers. Legacy plaintext password configurations require an
explicit TLS configuration change; ports are never silently rewritten. Required
TLS still validates certificates/hostnames. Unauthenticated localhost:25 relay
and explicitly configured unauthenticated remote relay remain supported. The app
is an SMTP client, not a listening relay server; an external relay must enforce its
own access, sender, recipient and onward-delivery policies.

## Deliberate test and production sending

The default test is fixed, synthetic, attachment-free connection-test wording.
Real-document test controls are opt-in and clearly warn about sensitive data.
They open a separate confirmation showing From/To/CC/BCC, subject/body, attachment
paths and SHA-256s. Recipients, configuration and document bytes are checked again
while reviewing; changes invalidate confirmation. The final MIME uses captured
bytes, never a later reread of a mutable attachment path. Test sends do not mark
production documents delivered. Test messages may contain real records only when
that option has been deliberately enabled and the specific message confirmed.

Every production transport entry uses one process-wide limiter: at most 100
attempts in a rolling ten minutes, with at least 30 seconds between test attempts.
Failed attempts consume quota; blocked attempts do not send anything. A batch has
at most 50 messages; more than 10 requires separate acknowledgement, invalidated
when selection is reviewed again. These are local accident/abuse safeguards, not
provider quotas or a cross-process service. Restarting/altering the client can
bypass local limits. Production ledgers, immutable approval checks and uncertain
submission protection remain in place. No automatic mail retry is introduced.

## Microsoft local cleanup and external revocation

Disconnect still only detaches the draft reference. A separate confirmed
"Remove local Microsoft authorization" action blocks that reference with a
read-verified non-secret tombstone, then removes its manifest and current/indexed
historical token chunks. Cleanup is idempotent and retryable after store errors;
clients implementing this marker keep the reference blocked even after partial cleanup or config restoration.
Other references are not enumerated or deleted. Profiles/backups sharing this
reference will need a fresh sign-in. Refreshes register a non-secret generation
chain so future historical chunks can be identified safely. Cleanup does not
alter Gmail/GMX credentials. Removal is user-triggered only; no existing credential
is automatically deleted when this version starts.

Generations orphaned before indexing existed, during interrupted publication, or by a concurrent losing publication,
cannot all be discovered using the portable keyring interface. The app does not
claim to erase those unindexed entries. Non-secret tombstones/index metadata are
retained for retries and restored-config protection. A process already holding a
token may still use it. Revoke the application's consent separately through
https://account.live.com/consent/Manage and follow Microsoft account security
instructions when compromise is suspected. Local deletion is not Microsoft-side
revocation and does not change Azure registration.

## Human approval and agents

A button click or in-memory approval object does not prove a human authorised it.
OAuth consent grants delegated sending authority, not consent for each message.
An AI agent controlling the desktop can operate buttons; a malicious same-account
process or modified client can bypass UI checks, use accessible credentials or
alter data. OS vault encryption, opaque references and approval digests do not
create a separate security boundary inside that same account. Keep untrusted
agents outside the credential-owning OS session. Stronger enforcement requires a
separate protected broker/relay, restricted credentials and independently trusted
approval, rather than extra dialogs alone. Syntax validation cannot establish
that a recipient belongs to the intended person. Review recipients before sending.

No secrets, raw provider errors, AUTH payloads or message bodies are added to
transport diagnostics. Existing private production history contains sensitive
recipient/message facts by design. Test sends retain no persistent message log.

## Unsaved Save appearance

Enabled Save actions share a blue/white treatment when their own editable draft
differs from the saved baseline. Disabled and unchanged actions retain ordinary
theme styling. A failed save retains the dirty indicator; a successful save resets
it. This covers application/payroll/email settings, employer/PA maintenance,
hours-basis/pay-rate history, running notes, preparation, returned hours, mileage,
sickness and save-before-navigation actions. It changes appearance, not validation,
save scope or database schema.

## Validation — 2026-11-02, Linux

- Focused transport/OAuth cleanup/UI/settings selection: 70 passed.
- Broad regression: `cargo test --offline --bin direct_payment_timesheets -- --skip native --test-threads=2`: 1,008 passed, zero failures; four native acceptance tests filtered out.
- Final Save/editor selection: 31 passed. Selections overlap; counts are not additive.
- Production `cargo check --offline --bin direct_payment_timesheets`, debug build
  and `git diff --check` passed. Dead-code warnings remain, including legacy test
  adapters retained for compatibility.

Tests used synthetic temporary data, mock credential stores and localhost SMTP
sinks; no real provider authentication, mailbox submission, native-store cleanup,
Azure changes or interactive GUI acceptance was performed. The initial plaintext
credential compatibility expectations were revised to retain unauthenticated
relay coverage and require encrypted authentication. Existing uncommitted work
and stashes were preserved. Live visual/native acceptance is a separate step.

The follow-up [email-security acceptance report](EMAIL-SECURITY-ACCEPTANCE.md)
records the expanded 1,030-pass regression, UI dirty-state corrections and native
acceptance gates. It supersedes the earlier validation counts for the current tree.
