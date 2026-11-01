# Microsoft personal-account SMTP OAuth2 (v1.0.7 development)

This implementation adds Microsoft delegated device authorization, token refresh
and SMTP XOAUTH2 to the existing Email Settings form and standalone test-email
workflow. It does not redesign the settings tabs. Outlook.com, Hotmail.com and
Live.com personal mailboxes use `smtp-mail.outlook.com:587` with mandatory STARTTLS
(native certificate/hostname verification, TLS 1.2 minimum). There is no Basic
password/app-password fallback for these domains or known Microsoft SMTP hosts.
GMX, Gmail and other password SMTP settings retain their existing behaviour;
local relay mode still uses unauthenticated localhost:25.

Microsoft documents both device authorization and authorization-code flows for
SMTP OAuth, with delegated `https://outlook.office.com/SMTP.Send` and
`offline_access`. This increment uses device authorization against the fixed
`consumers` authority, so it needs no listener, redirect URI or client secret.
It does **not** implement organizational Microsoft 365 accounts, shared-mailbox
permissions, national clouds, client-credentials grants or Graph Mail.Send.
Selecting Microsoft OAuth2 therefore requires the personal-account endpoint;
`smtp.office365.com` cannot silently use password authentication instead.
[Microsoft SMTP OAuth requirements](https://learn.microsoft.com/en-us/exchange/client-developer/legacy-protocols/how-to-authenticate-an-imap-pop-smtp-application-by-using-oauth),
[device authorization protocol](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code),
[Outlook.com SMTP settings](https://support.microsoft.com/en-us/outlook/pop-imap-and-smtp-settings-for-outlook-com).

## App registration required before real use

An authorized application owner must register a Microsoft public-client app and
supply its actual application (client) UUID. No ID or secret is embedded or
invented by this implementation. The app registration must support personal
Microsoft accounts (personal only, or organizational plus personal accounts),
permit public-client/device flows, and permit delegated Exchange Online
`SMTP.Send` with offline access. Review the displayed consent and any publisher
verification/organizational approval requirements. Do not create a confidential
client secret for this desktop app. Device-code flow does not use a redirect URI.
[Public-client configuration](https://learn.microsoft.com/en-us/entra/identity-platform/scenario-desktop-app-configuration),
[SMTP delegated permission and registration](https://learn.microsoft.com/en-us/exchange/client-developer/legacy-protocols/how-to-authenticate-an-imap-pop-smtp-application-by-using-oauth).

OAuth consent does not prove that SMTP AUTH is enabled or the mailbox is entitled
to submit mail. The app never changes account or tenant policy. Authentication
rejection requires checking the configured mailbox, consent and SMTP AUTH policy
with the account/tenant administrator; disabling security defaults is not an
application recovery action. Organizational tenant support would require a
separate reviewed implementation.
[Microsoft SMTP AUTH controls](https://learn.microsoft.com/en-us/exchange/clients-and-mobile-in-exchange-online/authenticated-client-smtp-submission).

## User flow

1. Choose SMTP Server and Microsoft OAuth2. This explicit switch sets the personal
   Microsoft host, port 587 and required STARTTLS. Enter the full mailbox address
   and registered public-client application ID.
2. Click **Sign in / reconnect with Microsoft**, open the displayed Microsoft
   link, enter the one-time code, sign in as the configured mailbox and review
   consent. No mail is sent by sign-in. Never enter a code supplied by someone
   else. The app does not parse unverified access-token claims to infer identity;
   the SMTP service validates whether the authorized account can use the mailbox.
3. Save Email Settings after completion, then explicitly send a standalone test.
   SMTP acceptance does not establish inbox delivery. A test has no payroll
   attachments, CC/BCC, delivery-history writes or database prerequisite.
4. Consent denial, code expiry and cancellation leave the configured credential
   reference unchanged. Cancelling or changing connection identity discards a
   pending result. Cancellation waits for in-flight HTTP (30-second deadline) or
   platform-managed native-store operations;
   app closure is held until the worker terminates.
5. Access tokens within 60 seconds of expiry are refreshed before opening the
   SMTP connection. Revoked/expired refresh tokens require sign-in again. A locked,
   missing, denied or unverifiable OS store blocks sending. An SMTP rejection
   offers reconnect/policy guidance; it never retries a submitted message or falls
   back to a password.
6. Disconnect removes the current reference from the draft settings; save to
   persist that change. It does not revoke Microsoft consent or delete retained
   vault generations. Revoke consent separately in the Microsoft account portal
   when appropriate. Restored configs on another user/computer need a new sign-in.

## Secret storage and failure boundaries

Only public settings and an opaque `microsoft_credential_ref` are serialized in
`config.toml`. Access and refresh tokens use the existing native Windows Credential
Manager, macOS Keychain or persistent Linux Secret Service abstraction. There is
no plaintext/session-only fallback. Token records are bound to the application
ID and configured mailbox. Memory holding raw token responses/records is
zeroized on normal drop where owned by this module; HTTP, TLS, serialization and
SMTP dependencies can hold temporary copies, so this is not a process-memory
zeroization guarantee.

Vault payloads are split into immutable, read-verified 1,000-byte entries to fit
Windows credential limits. A small reference manifest is updated only after all
new entries verify, then the assembled record is read-verified again. Old
manifest generations/chunks are deliberately retained; this increment has no
credential garbage collection. An interrupted write can leave unreferenced vault
entries. In-process refresh is serialized. Native stores offer no cross-process
compare-and-swap; concurrent app instances can replace the manifest in either
order and fail closed if readback differs. Microsoft does not revoke the old
refresh token merely because it was used to obtain a new one; actual revocation
still requires reconnect. No crash-atomic transaction spanning all OS vault
entries is claimed.
[Refresh-token lifecycle](https://learn.microsoft.com/en-us/entra/identity-platform/refresh-tokens).

Configuration load and backup-copy paths reject nonempty token/client-secret
fields, including nested fields, instead of retaining unknown secret fields.
Errors omit response bodies, provider descriptions, tokens and SASL payloads.
The guard also rejects device user codes, authorization codes, PKCE verifiers and
complete verification links that might carry a code. Codes are displayed only
in the transient sign-in UI and are cleared on cancellation/completion.
Microsoft HTTP requests use fixed HTTPS endpoints, no redirects, a 30-second
request deadline and 64 KiB response bound. Only 2xx responses may carry success
payloads; structured 4xx OAuth errors remain available for pending/slow-down and
failure handling. UI links accept only known Microsoft
device sign-in URLs. The existing lettre dependency has its `tracing` feature
**disabled**; enabling SMTP wire tracing could expose AUTH payloads and must not
be enabled in builds that handle credentials. The `log` facade is capped at Info
at compile time in both debug and release profiles: ureq-proto otherwise dumps
raw response bodies at Trace. This deliberately removes Debug/Trace logging
through that facade application-wide, while retaining Info/Warn/Error. A unit
test asserts this cap so a future dependency change cannot silently re-enable
body dumps. Never log HTTP bodies, full config,
OAuth record values or test/mock request captures from real accounts.

## Validation and remaining acceptance

Unit tests inject HTTP and vault mocks; production Microsoft HTTP and real sign-in
are explicitly disabled in test builds. Coverage includes pending/slow-down
polling, consent rejection, expiry, cancellation, stale completion, refresh
rotation/omitted rotation, vault read/write failures, identity binding, secret
serialization/backup rejection, TLS/endpoint gates and exact XOAUTH2 formatting.
Existing SMTP tests exercise synthetic loopback TLS and password transports.
No test needs a live mailbox, real OS credentials or native terminal SQLite fixtures.

Before release, an authorized app registration and consenting test mailbox are
still required for real sign-in/send/reconnect acceptance. Check OS vault storage
and refresh on Windows, macOS and Linux, a rejected/disabled SMTP AUTH account,
consent revocation, expired credentials, closing during sign-in and both successful
and rejected test sends. Existing macOS SQLite acceptance run 41305531150 does not
validate these new OAuth changes. A distributable application ID and publisher/
consent policy need owner approval; this code creates no external registration.

### Local validation, 2026-11-01 (Linux)

| Command (`cargo test --offline --bin direct_payment_timesheets …`) | Result |
| --- | --- |
| `microsoft -- --test-threads=2` | 21 passed |
| `smtp -- --test-threads=2` | 38 passed; 1 native credential test intentionally ignored |
| `email_settings::tests -- --test-threads=2` | 9 passed |
| `config::tests -- --test-threads=2` | 15 passed |
| `gui::standalone_email_ui_tests -- --test-threads=2` | 9 passed |

Groups overlap; these counts must not be summed as distinct tests. Production
`cargo check --offline` and `git diff --check` also pass. Existing dead-code
warnings remain. These are mock/loopback results, not live Microsoft or native
OS-vault acceptance. No native terminal SQLite fixtures or Actions were run.

### Pre-commit security review, 2026-11-01

Reviewed the device grant, polling/backoff, cancellation and close guards, token
expiry/rotation, vault readback and context binding, SMTP mechanism/TLS selection,
configuration persistence, backup-copy paths and dependency diagnostics against
Microsoft's linked protocol documentation. The review closed three gaps:

- Extended config/backup rejection to user/authorization codes, PKCE verifiers
  and complete verification links, including nested and differently cased keys.
- Added the application-wide compile-time Info logging cap to prevent ureq-proto
  Trace body dumps. SMTP wire tracing remains disabled in the resolved feature
  graph. Cargo.lock adds only the root reference to the already locked log crate.
- Reject success-shaped bodies on failed HTTP responses; structured HTTP 4xx
  OAuth errors remain usable by the device polling state machine.

The final safe regression selection was:

```text
cargo test --offline --bin direct_payment_timesheets -- microsoft smtp email_service email_settings config::tests gui::standalone_email_ui_tests gui::preparation_navigation_tests backup_service::tests --test-threads=2
```

Result: **117 passed, 0 failed, 1 intentionally ignored native credential-store
roundtrip**. This includes Microsoft protocol/storage/leakage tests, synthetic
TLS/XOAUTH2 and password transports, Gmail/GMX/relay configuration checks,
standalone setup/UI, approval invalidation and backup/restore protection on
isolated temporary data. Production `cargo check --offline`, dependency feature
inspection and `git diff --check` passed. No real Microsoft network operation,
native credential-store acceptance, native terminal SQLite fixture, VM operation
or Actions dispatch was performed. The live acceptance requirements above remain.
