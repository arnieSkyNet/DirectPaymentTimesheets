# SMTP password storage — development 1.0.7, 16 October 2026

SMTP passwords are now OS-store secrets. Host, port, username, transport, TLS
selection and test recipients remain ordinary settings; config.toml contains
only an opaque `email.smtp_credential_ref`, never a newly serialized password.
This does not encrypt usernames or other payroll/application settings.

## Native backends and target evidence

The shared implementation is src/smtp_credentials.rs. keyring is pinned to
3.6.3, with default features disabled and concrete backend constructors: a
missing backend fails compilation instead of selecting keyring's mock fallback.
Tests use an in-memory store, except the explicitly invoked native smoke test.

| Existing supported target | Selected backend | Evidence on Molly |
| --- | --- | --- |
| x86_64-pc-windows-msvc | Credential Manager, windows-native | Build/check and synthetic native write/read/delete |
| aarch64-pc-windows-msvc | Credential Manager, windows-native | Target dependency/feature graph and generated platform bindings inspected; native build/runtime still required |
| x86_64-unknown-linux-gnu | Secret Service | Target graph/source inspected; native build/runtime still required |
| aarch64-unknown-linux-gnu | Secret Service | Target graph/source inspected; native build/runtime still required |
| armv7-unknown-linux-gnueabihf (existing ARMhf release target) | Secret Service | Target graph/source and existing ARM cross-compiler configuration inspected; native build/runtime still required |
| x86_64-apple-darwin | Keychain Services, apple-native | Target graph/source inspected; native build/runtime still required |
| aarch64-apple-darwin | Keychain Services, apple-native | Target graph/source inspected; native build/runtime still required |

Linux uses sync-secret-service + crypto-rust + vendored, with the same pinned
dbus-secret-service 4.1.0 for collection preflight. DH-encrypted Secret Service
sessions are selected. Vendored libdbus uses the target C compiler, already
configured for ARMv7 in packaging/linux/build-binaries.sh, rather than linking a
host-architecture libdbus. It still needs a working desktop session bus and a
trusted Secret Service provider with a persistent, encrypted default collection.
No kernel-keyutils, file or session-only fallback is enabled. Missing/locked
collections are errors; known session-only defaults are refused. The app neither
creates an insecure default keyring nor changes desktop privileges/policy.
The provider remains responsible for actual encryption and durable persistence;
the API cannot independently prove its on-disk configuration or hardware durability.
Unlock through the desktop keyring manager and restart/retry as instructed.

Molly has only the x64 Rust target and lacks the ARM64 MSVC C compiler, Linux
toolchains and macOS SDKs. Dependency resolution is NOT a cross-build or native
runtime pass. Before release, compile all seven targets and run provider tests
on each architecture: missing/locked/denied/ambiguous entries, default/session
aliases, restart/logout persistence, TLS-authenticated loopback SMTP and denial
before any connection. macOS additionally needs signed/upgraded application
access-control/prompt tests; Windows needs ARM64 and logon-session persistence.
Linux provider compatibility, packaging and ARMhf crypto/C ABI remain release
gates, not assumed successes. Native keyring calls can involve OS prompts and
provider delays; they are not promised cancellable or time bounded.

Sources inspected:
- [keyring features and fallback contract](https://docs.rs/keyring/3.6.3/keyring/)
- [pinned manifest/platform dependencies](https://github.com/hwchen/keyring-rs/blob/v3.6.3/Cargo.toml)
- [Secret Service backend](https://github.com/hwchen/keyring-rs/blob/v3.6.3/src/secret_service.rs)
- [Windows backend](https://github.com/hwchen/keyring-rs/blob/v3.6.3/src/windows.rs)
- Local locked dbus-secret-service/libdbus-sys source and the repository's
  existing seven-target release matrix.

## Migration, saving and sending

An entry has a random UUID reference in a dedicated application namespace. Its
OS-protected value contains the password and canonical transport/host/port/user/
TLS context. Entry labels do not include SMTP passwords, hosts or usernames.
References are validated strictly; there is no search by guessed username,
default credential or plaintext fallback. Provider diagnostic/error objects are
never formatted because they may contain raw secret bytes.

For a legacy password, load writes a new entry and reads it back, verifying
context and exact password. Only then does it remove the legacy field from the
raw TOML and atomically publish a configuration containing the new reference.
Unrelated/unknown TOML values are preserved by migration. Windows holds the
existing checked ancestor handles and cooperating-writer mutex throughout the
compare/publication boundary; unexpected source changes refuse migration.

A failure retains the original config and/or verified entry, clears the usable
runtime password and exposes a redacted recoverable error in Email Settings.
Authenticated SMTP is blocked. Payroll use and passwordless relays do not require
a credential fetch. On upgrade, a backup that cannot safely protect a legacy
config stops before database installation rather than copying plaintext.

Saving or rotating a password similarly writes and reads a NEW entry before
config publication. Existing entries are never intentionally overwritten or
automatically deleted. Old configurations/backups retain their usable references;
interrupted publication cannot erase their password. Repeated unchanged saves
reuse the loaded reference after retrieval verification. After a successful GUI
save the in-memory reference is refreshed through the normal config loader.

Sending resolves and verifies the OS credential BEFORE DNS/socket/TLS setup.
Cached passwords do not override a missing, denied, locked or mismatched entry.
Explicit unsaved password edits retain the standalone-test workflow, but also
require secure-store write/read verification before connection. This may retain
an unreferenced entry after a failed test/save; cleanup is user-directed, never
an automatic deletion of potentially needed history. Save changed host/user/TLS
settings before testing against an existing reference.

There is no distributed transaction between an OS vault and the filesystem.
The ordering favours recoverability: verified entry first, atomic config second,
retain old entries on failure. Provider/storage corruption or loss after
verification cannot be prevented by the application; it fails closed on the
next retrieval. Close older application builds/other profile writers before
first migration: they do not understand references and can rewrite legacy
settings. Do not run v1.0.6 against this profile or transfer it to Florence.
The previously approved full-authority same-user threat boundary is unchanged.

## Backup, restore, logging and recovery

Backup and both restore staging paths protect raw config IN MEMORY before
allocating any new config file. Legacy passwords are verified in the OS store,
then replaced with references. Malformed TOML, unknown secret locations or
conflicting legacy/reference fields fail closed without a plaintext copy.
Restore approvals separately bind the original backup fingerprint and the
sanitized staged fingerprint; originals are never rewritten. Settings, database
schema, row counts and payroll recovery semantics otherwise remain unchanged.

On Windows, restore holds the same cooperating-writer publication lock and
checked ancestor handles as configuration saves and credential migration, from
approval validation through database commit and configuration publication. It
rechecks the live configuration after preparation, before committing the database.
The configuration is installed using the existing identity-checked atomic writer,
not a direct pathname rename. A competing save invalidates the restore approval;
it cannot silently interleave with an approved restore. This retains the approved
exclusion of malicious/uncooperative full-authority same-user processes.
Database commit and configuration replacement are still separate operations, not
a distributed transaction: a later publication failure reports RESTART REQUIRED
and identifies the verified safety backup; private sanitized recovery stages may
remain. Neither that stage nor the safety backup contains a newly copied plaintext
SMTP password.

Backups do NOT export the vault. Another computer/account, a deleted entry or an
unavailable provider requires password re-entry; no historical plaintext is
silently substituted. Keep the original backup and native-vault recovery options
until successfully recovering the intended account. No automatic secret deletion
is provided; obsolete/orphan vault entries need a separate reviewed cleanup.

Password and runtime baseline/error fields are excluded from TOML/JSON
serialization. EmailConfig Debug remains redacted. Approval/dirty detection
still detects password edits without serializing the password or persisting an
unkeyed password verifier. Serialized vault buffers use best-effort zeroization;
ordinary password editor/cache strings still exist in process memory. This is
not protection against full-authority same-user malware, memory dumps or OS
paging. SMTP diagnostics continue to redact credentials and message contents.
Transport-security settings, asynchronous sending, duplicate protection,
both Save buttons, compact layout and standalone previews remain in place.

Historical plaintext files are NOT deleted or rewritten by this change. Native
vault migration does not erase old file blocks, cloud versions, snapshots,
exports, crash dumps or off-machine backups. Identify affected copies privately,
rotate the SMTP password if exposure is possible, and choose retention or secure
disposal yourself. Deletion/rotation requires a separate user-directed action.
Local private audit findings and exact test results are in the ignored
target/windows-v107-diagnostics/SMTP-CREDENTIALS.md report. Production migration
has not been invoked by automated tests; it occurs on the next normal load.

Windows validation additionally exercises synthetic native migration, refusal
when atomic publication is blocked, recovery after removing that fixture-only
block, and retrieval by a freshly started process. Only the separate
DirectPaymentTimesheets.Tests.smtp namespace is used, and entries created by the
test are removed and absence verified. This proves process-restart retrieval on
Molly, not logout/reboot durability, ARM64 behaviour or other providers' behaviour.

On a correctly configured native Windows test host, the explicit test is:

```text
cargo test --locked --target x86_64-pc-windows-msvc smtp_credentials::tests::native_synthetic_migration_recovery_and_restart -- --exact --ignored --test-threads=1
```

Use the existing private synthetic TEMP/TMP root and normal user token for native
storage fixtures. The restricted development token intentionally cannot access
that root. The native restart child is a harness called by its parent, not an
independent test: do not run all ignored tests indiscriminately. Ordinary full
suite results list native tests as ignored; report the explicit native parent
run separately, and do not count unavailable symlink privilege coverage as passed.
