# Phase 1 storage hardening — development 1.0.7

This phase follows the read-only audit of
`ebe3801aa88d7990294dd38652242582e2b1a46d`. Schema remains 41. Published
v1.0.6 downloads and release information are unchanged. No email transport,
recipient policy, payroll calculation, signature bytes, document content or
immutable historical evidence is migrated by this phase.

## New storage objects

| Object | Creation policy |
| --- | --- |
| Application root/internal directories, signatures, backup directories | Unix 0700; Windows protected current-user/SYSTEM DACL |
| New live SQLite database, backup/restore/migration database copies | Privately reserve the empty file before SQLite writes; Unix 0600 |
| Restore-preview temporary directory | Randomized exclusive private creation, retained through approval; automatic cleanup only where its namespace is verified |
| Configuration/CSV archives/import staging/filing copies/new PDFs | Exclusive 0600 Unix creation; protected current-user/SYSTEM DACL on Windows |
| Configured business directories | Retain existing directory access; new directories follow the location's ordinary/inherited policy |
| Completed filing move | Stage privately, then retain the original document access policy before publication |
| Deliberately replaced PDF/configuration | Stage privately; retain the existing destination's permissions/DACL and refuse owner/group changes |

On Unix SQLite derives its journal/WAL/SHM modes from the database. New 0600
files are exercised under umask 000 in an isolated test process. Existing
database and sidecar modes are not changed; assess them separately. On Windows
SQLite creates sidecars with the parent directory's ACL. Writable database
opening refuses a parent whose ACL cannot be verified as limited to the current
user, SYSTEM, owner rights and administrators (creator-owner inherit-only ACEs
are accepted). This is a refusal with separate-repair guidance, not an automatic
ACL repair. Filesystems without enforceable Windows ACLs are not suitable for
private application storage.

The locked tempfile 3.27.0 creates Unix named files with 0600 but temporary
directories with default directory permissions. Phase 1 uses its random naming
with custom exclusive creation. Windows named temporary creation also supplies
an explicit protected DACL; it does not assume inherited ACLs are private.

Copying a legacy configuration or database into a new backup/staging object does
not propagate broad source permissions. Symlink/reparse-point storage paths are
refused; macOS's fixed OS aliases `/var` and `/tmp` to `/private` are allowed.
SQLite opens use NOFOLLOW. The dispatch lock is created privately if missing;
existing lock inodes are opened without truncation/replacement or permission
repair, and symlink/reparse-point locks are refused. Private file creation never truncates an existing
object. Publication into an originally absent destination uses no-clobber
semantics. Recoverable filing retains its original randomized ledger staging
path, state machine, digest checks and interruption behavior.

## Existing installations and intentional sharing

Application Settings provides **Assess Storage Permissions**. It reads metadata
only, follows no symbolic links/reparse points, runs off the GUI thread, and is
bounded to 10,000 entries per selected root. It reports broader Unix access,
non-current ownership, Windows ACL uncertainty and unavailable storage. Business
locations are labelled as possibly intentionally shared. Reports contain paths,
so treat them as private when seeking support. The assessment does not read
payroll/configuration contents, chmod files, set ACLs, chown, delete or repair.

Existing directory/file access and ownership are not silently changed. Filing
moves retain the original document policy after private staging; private backup
copies remain private regardless of source sharing. Atomic
replacement creates a new inode with the prior access policy; on Unix an
owner/group mismatch is refused. Shared output directories remain usable, and
existing shared documents retain their access on deliberate replacement. New
files are private by default, including files created in shared locations; grant
sharing separately and deliberately. New owner-only creation does not promise
privacy against people who can replace entries in a writable shared parent.

A separate repair review should:

1. Close all application instances and inventory the data root, database and
   sidecars, external documents/signatures, archives, original ZIPs and backups.
2. Record ownership, Unix modes and any POSIX ACLs, or Windows owner/DACLs;
   establish which folders/documents intentionally need shared access.
3. Protect a consistent verified database/config backup and the full external
   file estate before any approved access changes. Preserve WAL recovery data.
4. Review exact proposed paths/access changes with the operator. Do not blindly
   recurse through shared storage, symlinks or another account's files.
5. Apply only separately approved changes using platform administrative tools,
   then reassess and verify recovery using synthetic isolated data.

This phase provides no repair button and makes no production permission changes
on the developer's behalf.

## Limits and subsequent phases

- Passwords, payroll SQLite/BLOBs, documents and backup copies remain unencrypted
  at the application layer. OS credential vaults and encrypted backup/export are
  subsequent work. SMTP plaintext compatibility and redacted diagnostics remain.
- SQL triggers, digests and local UI approvals protect workflow integrity; they
  do not authenticate a separate operator or defend against privileged/same-user
  malicious processes. Writable shared parents retain namespace-race risk.
- Linux assessment flags named file access ACLs and macOS assessment flags extended
  allow entries. Existing file ACLs are retained on replacement; new macOS private
  objects receive an explicit empty, non-inheriting ACL at creation through an
  Apple-SDK wrapper, avoiding post-creation access windows. Directory default ACLs,
  ancestor ownership/access and remote filesystem semantics need separate review.
- Normal cleanup is not secure erasure; abrupt termination may leave private
  staging files. The recovery file estate must include these remnants.
- Backup recovery continues to cover database/configuration, not the complete
  external document/signature/import estate. It retains historical attachment
  BLOBs and existing uncertain-email/rollback approval protections.
- Native Windows ACL and macOS acceptance are required; Linux tests cannot prove
  those platform behaviors. FAT/exFAT and remote filesystem ACL/locking limitations
  must be reviewed before using them for application-private storage.


The macOS native helper is compiled by the installed Apple SDK/clang during a
macOS build; SDK headers resolve FILESEC/ACL constants without guessed Rust
numeric definitions. macOS tests exercise a deliberately inheritable shared ACL
and verify private creation plus preservation on replacement. Those tests and
native helper compilation require macOS and were not exercised on Linux.

### Open findings from the review against 46f49ba

The randomized tempfile names are not a cryptographic guarantee; exclusive
creation is the collision protection. The review's security findings remain
open at the reviewed commit: Windows assessment did not validate the actual owner
SID or ancestor access; path-based checks/publication do not prevent writable-ancestor
substitution; macOS ACL text parsing missed comma-suffixed allow entries; and
Windows tempfile `keep`/`persist` resets staging attributes after read-only
permissions were copied. The earlier descriptions of assessment and replacement
policy must be read with these limitations. No permission repair is performed.

Completing cross-platform resolution requires native Windows/NTFS and macOS validation,
including ownership, inheritance, reparse points, read-only publication and
restore failure handling. Linux-only checks cannot establish those guarantees.
The naming comment and redundant closure are corrected. The cleanup follow-up
reproduced substituted-directory deletion, then retained original directory and
parent handles on Linux. Cleanup checks identity using `openat` with no-follow,
requires a trusted owner and non-writable or sticky parent, verifies private
directory ownership/modes, and anchors content deletion through the original
directory's procfs descriptor. Rust's Linux removal uses no-follow, handle-relative
child traversal. The empty root is rechecked and unlinked relative to the
protected parent handle. It refuses cleanup if identity/access/procfs cannot be verified;
no fallback deletes by the stored pathname. Ordinary other users cannot
substitute the protected entry. Privileged/malicious same-user processes remain
outside the boundary; this does not fix other publication/open namespace races.

Windows/macOS automatic staging cleanup is disabled pending native verification.
Private recovery staging is retained rather than risking deletion of unrelated
files. Linux also retains staging in unprotected parents or where the OS owner
cannot be established (including unmapped owners in a sandbox). Permissions and
ownership are never repaired to enable cleanup. This can consume temporary disk
space; inventory those private remnants during separately reviewed maintenance.
Restore-plan lifetime, fingerprints, database transactions and rollback checks
are retained. Only verified cleanup removes staging after the last reference.
This is not cross-platform security sign-off.

## Implementation validation before native-verification fixtures

The following full-suite result is historical. The verification branch adds
known-failing assertions without fixing the underlying vulnerabilities. See
[native verification](NATIVE-SECURITY-VERIFICATION.md) for current checks,
coverage gaps and the default-branch workflow-registration prerequisite.

Linux validation used locked, offline dependencies: `cargo check` passed; the
nine Phase 1 tests, 55 archive/recovery tests and 21 delivery/lock tests passed;
the final full `cargo test` passed all 886 tests. Synthetic loopback SMTP tests
required execution outside the socket-restricted sandbox. No production data,
real email dispatch, packaging or release operations were used.

`cargo fmt --check` remains unsuccessful on existing repository formatting:
unmodified HEAD had 41 difference sections; 38 remain after formatting the
changed code. New storage modules and edited sections are formatted.
`git diff --check` passed. Windows/macOS native compilation and ACL acceptance
remain outstanding; Linux success does not establish their native behavior.

## Remediation after native run 38264276658 (uncommitted)

The run dispatched main 33c3664 and checked out approved source 6329b6d on
Windows 2022, macOS 15 ARM64 and macOS 15 Intel. Guards/setup, cargo check and
native test compilation passed. Each platform failed temporary fixture setup;
this was not a Rust compilation or runner-label failure. The first Windows
error was `Filesystem did not enforce a private directory ACL`; both macOS
jobs first reported ENOENT. No native security acceptance is established.

Windows assessment now obtains owner/DACL from an opened no-follow handle with
GetSecurityInfo, compares valid binary SIDs using EqualSid, and requires the
actual owner to match the current process token user. Creation requests that
owner explicitly, rather than relying on an elevated token's default owner.
Unknown ACE types, null/invalid DACLs, foreign owners and untrusted allow ACEs
fail closed. Existing SYSTEM/Administrators and owner-rights policy is retained;
OW does not substitute for actual owner validation. Failed creation is retained
rather than removed through a potentially substituted pathname. SDDL remains
in creation and the existing access-preservation path, not privacy assessment.
The original rejection may involve SID aliases or ownership; the old run does
not expose sufficient evidence to attribute it to either conclusively.

macOS ACL inspection now uses fstatx_np plus filesec_query_property through the
SDK C helper. Only a successful handle inspection with an explicitly absent
ACL property is treated as absent; inspection errors are not suppressed.
Apple's filesec_get_property returns ENOENT for absent properties, explaining
why the old ACL reader could misclassify absence as a missing file. Native
entry/tag inspection replaces ACL text parsing, including inherited allow ACEs.
Only /var and /tmp aliases with exact /private targets (absolute or relative),
root ownership and a protected /private parent are eligible. Other symlinks and
parent traversal remain refused. These checks are snapshots, not a solution to
concurrent ancestor substitution. Actual creation/ACL acceptance still requires
native execution; operation/code/class diagnostics are provided without paths,
identities, document bytes or complete ACLs.

Synthetic backup/database/filing fixtures and consumers of the shared application
fixture now retain PrivateTempDir roots, preserving Linux anchored cleanup and
non-Linux fail-closed retention. Negative broad-parent, junction, inherited-ACL,
read-only, restore-lifetime and substituted-directory tests remain active.
New tests cover exact alias targets, absent ACL inspection, invalid native handles
and rejecting an actual foreign owner even with OW permissions. The Windows
foreign-owner fixture needs permission to assign the Administrators owner; a
setup failure must be reported, not skipped or interpreted as a security pass.

Still open: Windows ancestor access and concurrent path substitution, Windows
read-only tempfile publication (locked tempfile 3.27.0 explicitly resets attributes
to FILE_ATTRIBUTE_NORMAL in keep/persist), native replacement/preservation,
SQLite sidecar acceptance, macOS SDK compilation/ACL behaviour and native recovery.
No workflow pin was changed, no native run repeated and no existing permissions
were repaired. A new reviewed source commit and separately approved workflow
pin update are required before another native verification run.

Validation of this uncommitted follow-up on Linux:

- `cargo check --locked --offline --bins --tests`: passed.
- `cargo test --locked --offline phase1 -- --test-threads=1`: 16 passed,
  including all substituted-directory/symlink/ancestor cleanup regressions.
- Backup group: 7 passed; database recovery: 29 passed; filing recovery: 31 passed.
- Runner unit tests: 5 passed (`python3 -B -m unittest discover -s scripts -p test_phase1_security_checks.py`).
- Full `cargo test --locked --offline`: initial sandbox run had 858 passing and
  35 failing tests; isolated SMTP fixtures reproduced EPERM at loopback bind
  (`src/smtp_transport_tests.rs:19`, `src/payroll_production_tests.rs:131`).
  Approved execution outside the sandbox passed all 893 tests, with no real
  recipients or production data. The run took 220.75 seconds.
- Targeted rustfmt checks and `git diff --check`: passed. Comparing rustfmt
  output for every changed Rust file against HEAD found no introduced formatting
  deviations; existing deviations in unrelated sections were retained.
- `cargo clippy --locked --offline --bins --tests`: exit 101, blocked by the
  unchanged `src/config.rs:993` `Permissions::from_mode(0)` expression and
  default-denied `clippy::non_octal_unix_permissions`; 99 warnings were also
  emitted. The same offending expression exists at the starting commit. It was
  not changed as part of this remediation. Native-only code was not linted.

Only Linux target libraries are installed. Windows Rust/NTFS execution and
macOS SDK C compilation/execution were not performed. The API work was checked
against [Microsoft GetSecurityInfo](https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-getsecurityinfo),
[EqualSid](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-equalsid),
and Apple's [filesec property implementation](https://github.com/apple-oss-distributions/Libc/blob/main/gen/filesec.c)
and [ACL entry implementation](https://github.com/apple-oss-distributions/Libc/blob/main/posix1e/acl_entry.c).
Source inspection and Linux parsing are not substitutes for native validation.
Windows test diagnostics also avoid printing command stderr or complete SDDL
on assertion failures; fixture failures expose only operation/code/class.
