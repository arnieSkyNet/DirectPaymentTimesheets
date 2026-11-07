# Molly: isolated Windows v1.0.7 development

Update, 15 October 2026: the separately authorized production volume-root owner
policy is shared by normal and development builds. The
isolated feature, fixed paths, opt-in and release exclusion remain available;
normal builds use the normal profile and display no development warning. See
SECURITY-HARDENING.md for the current production owner-admission requirements.

This opt-in build runs the real application, Email Settings persistence and SMTP
implementation with a fresh test database. It never imports production settings
or payroll. It is specific to Molly's Windows user profile.

Fixed profile:
`C:\Users\Wizdom\AppData\Local\DirectPaymentTimesheets-v107-development`

Its `config.toml`, `database.sqlite`, sidecars, internal directories and five
business folders stay separate from `DirectPaymentTimesheets`. Initial settings
contain no SMTP credentials or recipients and select required STARTTLS. Existing
unrecognised/incomplete profiles are refused rather than adopted or repaired.
Nothing is automatically sent. Enter SMTP settings and choose to save or send
through the existing Email Settings screen. Sending uses current drafts; save
explicitly to persist them. Both normal and development builds use the shared
secure SMTP credential storage: Windows Credential Manager on Molly, Secret
Service on Linux and Keychain Services on macOS. New configuration serialization
omits passwords and stores only opaque credential references. Host, username and
other non-secret settings remain ordinary configuration data. See
[SMTP-CREDENTIAL-STORAGE.md](SMTP-CREDENTIAL-STORAGE.md) for platform validation
gates, provider requirements and the full recovery contract.

Loading a legacy plaintext password first writes and reads back a new secure-store
entry, verifying its context and value before atomically replacing the config with
a reference. Failure retains recoverable original configuration and/or verified
entries, clears usable runtime credentials and blocks authenticated SMTP; there is
no plaintext fallback. Passwordless relays remain supported. This is not a
transaction across the vault and filesystem: old entries are retained for recovery,
including after password rotation or failed publication. Unsaved password edits
also require secure-store write/read verification before a test can connect.

New backups and restore stages sanitize legacy passwords in memory before writing
any configuration copy; an unsafe or malformed input fails closed. Backups contain
references, not exported vault secrets. Moving a backup to another account or
computer, losing an entry or an unavailable provider can require password re-entry.
Existing plaintext originals, historical backups, snapshots and external copies
are not erased by this change; their review, rotation and disposal are user-directed.
Passwords can still exist in process memory and OS paging. The isolated file profile
uses the same account-level application vault namespace, not a separate OS identity
or vault; use synthetic credentials, not production credentials, for development.

From Git Bash at the repository root, build using the existing Molly MSVC wrapper:

```bash
MSYS2_ARG_CONV_EXCL='*' cmd.exe /d /c 'target\windows-v107-validation\cargo-msvc.cmd build --locked --offline --target x86_64-pc-windows-msvc --features windows-dev-isolation'
bash scripts/run-windows-development.sh
```

The launcher removes `DIRECTPAYMENTTIMESHEETS_HOME` only from its child environment.
Equivalent direct launch:

```bash
env -u DIRECTPAYMENTTIMESHEETS_HOME ./target/x86_64-pc-windows-msvc/debug/direct_payment_timesheets.exe --isolated-windows-development
```

The feature build refuses launches without exactly that argument and rejects any
home override. A normal executable rejects the development flag. Cargo's build
script refuses the feature outside the Windows debug profile; an independent
compile guard also requires Windows, debug assertions and the build-script
development marker. Release builds cannot include this mode, even if their debug
assertions are enabled. No installer/release configuration enables the feature.
The main window title and persistent banner identify DEVELOPMENT MODE and show
the isolation path. Normal builds retain their original paths and security policy.

## Production root policy and development isolation

The separately authorized production rule admits only the exact TrustedInstaller
owner SID at a verified fixed local NTFS volume root. It requires matching
handle identities, complete GUID-root identity, persistent ACLs, no reparse/query
errors and the unchanged DACL assessment. This is explicit owner-risk acceptance,
not service-name/signature trust. Normal and development builds share this root
policy; development mode provides no additional owner or ACL exemption.
All intermediate owners/ACLs, private immediate-parent and sidecar inheritance,
path/reparse checks, held identities and existing containment/publication checks
remain active. Ancestry handles for the profile remain held through process exit.
Database requests outside the profile fail closed while development is active.
Configuration reads/saves require the fixed private file. Existing development
configuration, database and sidecars must be private and have a single hard link.
Folder redirection is
rejected before application-setting drafts can mutate the active configuration.
This profile is outside the checkout because the checkout's ACL grants other
SIDs modification rights; those intermediate checks must not be bypassed.

## Limits

This is not an OS sandbox against the current user, administrators, SYSTEM,
TrustedInstaller or untrusted code. The earlier metadata-only ancestor opens
were insufficient to prevent directory rename on Molly. The completed correction
requires FILE_LIST_DIRECTORY with no delete sharing for held ancestor guards,
retains handle identities and rechecks pathname identity; there is no fallback
to metadata-only opens when the required access is denied.

Windows atomic publication requires verified fixed local NTFS, private staging,
validated owners/ACLs/reparse state and supported source-handle FileRenameInfoEx
POSIX replacement. Destination identity handles remain held. Per-user mutexes
keyed by parent directory identity coordinate participating publishers, including
configuration saves, credential migration and restore; timeouts, abandoned locks
or unverifiable conditions fail closed. No TxF, copy or ACL/attribute bypass is
used. An absent destination is published without overwrite, and observed identity
changes before publication are refused.

The destination identity check and rename are not an atomic expected-ID
compare-and-swap. Malicious or uncooperative processes with the user's full account
authority are explicitly outside the approved threat model; a process bypassing
coordination can substitute a destination in that interval. Held directories and
private ACLs do not establish a boundary against equivalent user authority, and
arbitrary same-user file-manager races are not claimed to be prevented. Atomic
namespace replacement is not a guarantee of crash durability. Database commit
and configuration replacement also remain separate: a later publication failure
requires restart and recovery using the verified safety copy. See
[SECURITY-HARDENING.md](SECURITY-HARDENING.md) for the precise boundary.

Filesystem proofs are point-in-time and synchronous Windows queries can block.
Native credential-store calls can also block or prompt. Deliberate file-picker actions remain
real application actions; use synthetic data. Some backup/restore workflows that
stage databases outside the profile will be refused. Do not use this executable
for production payroll. No successful real-server SMTP delivery is claimed until
the user explicitly enters credentials and requests a standalone test.

Regression coverage includes launch/no-fallback decisions, independent compile
exclusion, exact-owner/volume proof negatives, native isolated database startup,
configuration persistence, private journal/WAL/SHM, unsafe intermediate and
inheritance refusal, junction rejection and held-handle identity comparisons. Native
fixture mutations are confined to freshly created synthetic directories in a
separate test process; normal tests never acquire its development authorization.
Windows private staging fixtures remain retained under the existing cleanup policy.
