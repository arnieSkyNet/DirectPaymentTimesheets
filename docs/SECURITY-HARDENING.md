# Phase 1 storage hardening — development 1.0.7

This phase follows the read-only audit of
`ebe3801aa88d7990294dd38652242582e2b1a46d`. Schema remains 41. Published
v1.0.6 downloads and release information are unchanged. No email transport,
recipient policy, payroll calculation, signature bytes, document content or
immutable historical evidence is migrated by this phase.

## Approved account-authority boundary (Molly, 15 October 2026)

The operator explicitly excludes malicious or uncooperative processes already
running with the legitimate user's full account authority. This is a local
per-user application, not a security boundary between equivalent user tokens.
It does not defend against such a process changing names, bytes, ACLs, invoking
the app or modifying its memory. Restricted same-SID tokens can have fewer rights;
that does not make them representative of full-authority same-user peers.

Other-user protection, strict owners/DACLs, exact verified-root admission,
reparse/junction/symlink refusal, private creation, held directory identities,
SQLite containment and no-clobber publication remain required. Shared read
permissions deliberately retained for business documents are not private data
grants for application settings/databases. Untrusted destructive publication
ancestry is refused, not repaired or silently accepted. Administrator/SYSTEM
and the previously approved exact volume-root owner's authority remain existing
explicit trust assumptions; this revision grants no new owner/ACL exception.

Windows publishers hold FILE_LIST_DIRECTORY/no-delete-sharing ancestry guards
and per-user global mutexes keyed by parent FILE_ID_INFO (ordered for two-parent
operations). Cooperating new-version publishers serialize validation/publication;
timeouts, abandoned locks and unverifiable mutex policy fail closed. Old versions
and unrelated full-authority tools that bypass this protocol do not participate.
Absent-name collisions are refused, and observed identity changes before the
final check refuse publication. Source-handle rename prevents publishing a
re-resolved staging-name substitute. Destination identity handles remain held
using FileRenameInfoEx POSIX replacement; no TxF, copy or permission fallback.

There is still a destination validation-to-native-publication interval. The API
does NOT compare an expected destination ID atomically: a full-authority actor
bypassing coordination can substitute a target during that interval. This is
explicitly outside the approved boundary, whether the actor's behaviour was
malicious or merely uncooperative. It is not claimed that arbitrary same-user
file-manager accidents are prevented. Cooperating-writer races, no-clobber races
and detected stale identities remain covered and tested. A post-publication
check cannot undo an unsafe overwrite and is not presented as a security fix.

Publication requires verified fixed local NTFS and supported FileRenameInfoEx;
unsupported conditions fail closed. Staging is flushed before rename; atomic
namespace publication is not a promise of crash durability across arbitrary
hardware failures. Windows failure stages are retained when pathname cleanup
cannot be authorised. The bounded database guard lifetime and remaining sidecar
limitations documented below are unchanged. SMTP credential storage was deferred
in that publication pass. The subsequent 16 October implementation and remaining
native platform validation gates are documented in SMTP-CREDENTIAL-STORAGE.md;
it does not change the approved publication threat boundary.

The native API contract is Microsoft's
[FILE_RENAME_INFORMATION flags documentation](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information)
and [SetFileInformationByHandle](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle),
with the Win32 [FILE_RENAME_INFO flags/class contract](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info).
REPLACE_IF_EXISTS | POSIX_SEMANTICS permits replacement while existing target
handles remain valid; it does not accept an expected target identity. No
IGNORE_READONLY_ATTRIBUTE or access-check bypass flag is used. The trusted PDF
renderer can intentionally replace its reserved stage: its completed private
file is opened and validated under the same guards/mutex before hashing and
ledger approval. Publication itself never refreshes an unexpected stage ID.
The existing destination's owner/DACL is also checked under the writer guard
before stage allocation and before copying any access policy onto new bytes.
An unsafe policy cannot be copied first and merely rejected at rename time.
The mutex coordinates participating publication APIs, not arbitrary filesystem
operations. The subsequent SMTP storage correction integrates BackupService
configuration restore: it holds the same Windows mutex/ancestor guards across
approval validation, preparation, database commit and guarded atomic configuration
publication, with a pre-commit configuration fingerprint recheck. A competing-save
regression verifies waiting and rejection of stale approval. Database/configuration
commit is not a distributed transaction; publication failure still requires restart
and the verified recovery copy. See SMTP-CREDENTIAL-STORAGE.md.

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

## Follow-up to native run 38290913085 (uncommitted at 026b53f)

Both macOS 15 architectures compiled and passed all focused groups at source
026b53f (Phase 1 17, backup 7, database recovery 29, filing 31; Python 5).
Windows compiled, passed Python 5 and backup 6/database recovery 29/filing 30,
and failed five Phase 1 tests: two write_atomic calls returned error 5, unsafe
ancestor admission was not refused, and tempfile keep/persist cleared read-only.
The old logs do not identify the internal operation that returned access denied.
Do not assert that it was rename, ACL application, sharing or fixture setup.

The current follow-up labels original open/identity, stage creation/write/sync,
access preservation, identity recheck and publication. Win32 security descriptor
reads, DACL application and MoveFileExW emit operation/code/class only. Error
contents, identities, paths and full ACLs are not rendered by the new operation
wrapper. A native Windows run is required to identify the exact error-5 operation;
no identity handle was released and no sharing/security guard was weakened to
work around it. MoveFileExW replacement of an open destination needs explicit
native evidence before selecting a different rename API or lifetime policy.

Private Windows keep avoids TempPath.keep's SetFileAttributesW(NORMAL) by
retaining the existing file and disabling only tempfile cleanup. It refuses a
TEMPORARY-marked file rather than silently carrying that attribute. Publication
uses the same MoveFileExW flags as tempfile (0 or REPLACE_EXISTING), without the
attribute reset, cross-volume copy fallback or readonly repair. Failure returns
the pending file through PersistError. Native read-only keep/publication tests
now exercise these application wrappers, not the known-broken dependency path.
Config, signature, snapshot PDF, imported-document and backup staging callers
use the policy wrappers. Unix still delegates to the same tempfile methods.
A Windows rename refusal is reported, not retried with cleared attributes;
this is a proposed policy correction, not native proof of successful publication.

Database admission now captures and retains ancestor directory handles without
delete sharing, rejects reparse points/unknown ACE forms/untrusted owners and
broad destructive or attribute/ACL-changing access, and rechecks identities.
SYSTEM and Administrators remain trusted for ancestors; the immediate parent
must retain current-user ownership and an inheritable current-user FullControl
allow ACE. Non-inheriting private parent DACLs are refused before reserving
SQLite. Existing permissions/ownership are never repaired. Broad readonly
ancestor access and additive subdirectory creation remain allowed; that policy,
the explicit access mask and owner set require native review on standard-user
and administrator profiles. TrustedInstaller-owned/custom/remote ancestors may
be conservatively refused. No fallback bypasses a failed inspection.

Limits: handles are held through prepare_database only, not the full rusqlite
connection lifetime or each later sidecar operation. This is not handle-relative
SQLite I/O. ACL changes by trusted/privileged processes, the path-based publication
identity-check/rename interval, shared output parents, and named tempfile failure
cleanup remain open. same-file identity relies on its existing Windows file-ID
implementation; ReFS/remote filesystem semantics are not verified. This guard
is not universal path-substitution sign-off and no production acceptance is claimed.
New synthetic tests cover missing inheritable parent access, held-ancestor rename
refusal, blocking destination handles, wrapper no-clobber/policy preservation,
and diagnostic redaction. Existing unsafe-parent, read-only, restore lifetime,
rollback and substituted-directory regressions remain active. No workflow SHA
pin, main, stashes or production files have been changed. No native run is
permitted by this local remediation task; a separately reviewed source commit
and approved verification pin/run are necessary to obtain that evidence.

Local validation of this follow-up (Linux only): cargo check --locked --offline
--bins --tests passed; Phase 1 18/18, private-storage tests 15/15 (including two
new publication/redaction tests), backup 7/7, database recovery 29/29, filing
recovery 31/31, snapshot service 14/14, signatures 17/17 and configuration 15/15
passed. Python runner unit tests passed 5/5. Targeted rustfmt and diff whitespace
checks passed. No full-suite rerun was needed; the baseline 893-pass result is
not claimed for this follow-up. Native-only tests and implementation are not
compiled or exercised by these Linux checks. The final changes are unstaged;
no commit, push, pin update or dispatch has occurred.


## Diagnostic-only candidate after run 38302554351 at c11221b

This uncommitted follow-up adds Windows ancestor depth/role, SID-validity and
fixed owner-category diagnostics without changing owner trust, ACL admission,
identity checks or publication semantics. The rejected ancestor is still unknown:
66 failures report owner_mismatch/unsafe_ancestor. One unexpected replacement
reports MoveFileExW error 5; open-target identity lifetime is a hypothesis, not
an experimentally established cause. Rust default file sharing includes delete;
staging explicitly uses sharing 7. New tests retain production identity handles.

Positive ancestor controls now precede both negative fixtures; precise reason
assertions prevent unrelated failures from satisfying broad-ancestor or missing-
inheritance tests. The synthetic eight-case publication matrix uses the exact
production rename primitive with structured raw OS codes, checks bytes, available
identities, attributes and opaque owner/group/DACL snapshots, and records only
controlled masks and classifications. No live ACL contents or identities are
printed. The matrix may intentionally remain red for normal open-target
replacement. API success/refusal expectation mismatches and unexpected refusal
classes are counted and reported by the final contract assertion. Setup failures,
security/byte/identity/attribute invariant violations and cleanup failures terminate
the matrix immediately; later cases are not guaranteed to run after those failures.
No policy relaxation, permission repair, cross-volume copy, retry, workflow pin
change or native execution occurs. See NATIVE-SECURITY-VERIFICATION.md for the
current four-file inventory and separate immutable source approval procedure.
All prior production sign-off limitations remain open.

Linux validation of this diagnostic candidate: cargo check --locked --offline
--bins --tests passed; Phase 1 18/18 and private-storage 15/15 passed. Targeted
rustfmt checks and git diff --check passed. No full-suite rerun, native Windows
compilation/test execution or new macOS execution is claimed. All four changes
remain unstaged. Existing native results refer only to approved c11221b.

Review corrections: expected publication refusals require structured Win32 codes
5 (access), 32 (sharing), 33 (locking) or 1314 (missing privilege). These are
plausible denial classes, not proof of the intended reason; missing-path,
invalid-parameter, other codes and errors without an OS code are mismatches.
A native classifier regression covers accepted and rejected codes. Code meanings
are documented by [Microsoft codes 0–499](https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--0-499-)
and [1300–1699](https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--1300-1699-).
Ancestor handle-open, metadata and GetSecurityInfo failures now return and log
operation/code/class/depth/role together, including identity recheck opens.
The volume_root diagnostic describes the RootDir namespace component: on UNC
paths it is a share namespace root, not necessarily a physical volume root.
No admission or rename policy is changed by these corrections.


## Classification-only root-owner probe at b4e1565 (uncommitted)

Run 38312225467 established valid other ownership at depth-zero namespace root,
not any particular service principal. This candidate probes only that already
rejected case and still returns owner_mismatch. One fixed TrustedInstaller
service-SID hypothesis uses a qualified lookup starting locally, then is validated
and compared using native
SID equality; forward/reverse identity checks and read-only local SCM service-SID
configuration verification are required for a verified classification. No arbitrary
service name, SID prefix or root ownership confers trust. Unknown/disabled SID
configurations and missing/denied APIs remain indeterminate. Classification of a
configured service SID does not identify its process account or running token.

Diagnostics print fixed labels/booleans/error codes only, not identities, service
names, domains, paths or ACL contents. Probe buffers are bounded and aligned;
SCM handles use their own close function. The probe never mutates a service,
owner, DACL, filesystem object or token. Existing admission allowlists, destructive
ACL checks, inheritance rules, handle protections and publication flags remain.

The three new Windows-only synthetic tests validate native SID comparisons and
probe gating, unresolved/unverified/false-match classifications and safe rendering.
They mock resolution/verification facts and do not establish LookupAccountNameW,
LookupAccountSidW or SCM runtime results. Independent native compilation, missing
candidate/access-denied testing and standard-user verification remain necessary.
Read-only service configuration may change concurrently; this is diagnostic data,
not an atomic security attestation. Bounded lookup failure requires review, not
an insecure fallback. No permission exception or rename remediation is implemented.

API contracts: [LookupAccountNameW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-lookupaccountnamew),
[service SID configuration](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_sid_info),
and [QueryServiceConfig2W](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-queryserviceconfig2w).

Local Linux validation: cargo check --locked --offline --bins --tests passed;
Phase 1 18/18 and private-storage 15/15 passed. Targeted rustfmt, documentation
inventory/policy checks and git diff --check passed. No native Windows build or
execution is claimed. All three files remain unstaged; no commit, push, pin
update, workflow dispatch or stash modification occurred.


Final review clarification: LookupAccountNameW and LookupAccountSidW with NULL
system parameters are not guaranteed local-only. Even qualified lookup may
consult trusted domains and has no guaranteed bounded latency. Repeated rejected
root assessments may synchronously repeat lookup and read-only SCM work while
directory handles are held. Bounded buffers bound memory, not execution time.
This diagnostic-only probe requires bounded or opt-in execution and availability
assessment before production integration; no such execution bound is implemented.

Synthetic classification tests do not verify actual API failure handling,
raw-error capture or SCM cleanup through the probe. Identity lookup and service-
SID configuration observations are non-atomic and confer no executable trust,
running-token identity or admission authority. The service SID, service process
account and running token are distinct concepts; verified_candidate_service_sid
is not authorization to admit the owner. Existing owner_mismatch refusal remains.


## Opt-in namespace-root and DACL diagnostics (candidate, not native acceptance)

The next candidate keeps all owner refusals and trustee restrictions intact.
`DPT_WINDOWS_ROOT_DIAGNOSTICS=1` explicitly enables diagnostics only for a valid,
rejected `other` owner at depth zero with the syntactic `volume_root` role. Without
that exact opt-in, neither new root queries nor the existing account-resolution /
SCM probe runs. No workflow or pin has been changed to enable it.

The read-only root query requires an exact handle-derived volume GUID root,
DRIVE_FIXED, NTFS with FILE_PERSISTENT_ACLS, no observed reparse attribute, and
matching FILE_ID_INFO (64-bit volume serial plus 128-bit file ID) from an
independently opened GUID root and a repeat query of the original handle.
GUID strings and identities stay in memory and are never logged. Unsupported,
remote, malformed, descendant and ambiguous namespaces cannot produce a verified
classification. After a GUID-query failure, an opened handle-derived DOS UNC name reports
network_namespace with the original numeric query error. If that observation is
unavailable, the failure remains indeterminate; no particular network type is inferred. The query is conservative and is not a general filesystem
compatibility or adversarial-driver attestation. Classification never authorizes
admission, including when it reports verified_local_ntfs_root.

All root/DACL records carry fixed depth=0 and role=volume_root context. Root
errors retain fixed metadata, final-path, volume-information, independent-open
and file-ID operation labels. Fallback errors preserve both primary and fallback
codes; a successful fallback records whether UNC was observed. Insufficient
final-path buffers use derived ERROR_INSUFFICIENT_BUFFER (122), rather than a
stale GetLastError; an oversized GUID-root result remains ambiguous and does not
open an independent root. GetDriveType supplies a drive classification, not a documented
GetLastError failure; non-fixed results remain unsupported.

Root DACL diagnostics use the same extracted policy evaluator as admission,
independently of the unchanged owner refusal. policy_pass means only that the
existing conservative ACE scan passed; it is not effective AccessCheck, does not
attest owner rights, and does not authorize an otherwise foreign owner. OW and
inherit-only handling, unsupported-ACE refusal and destructive-access masks are
unchanged. No TrustedInstaller owner or trustee exception is added.

Seven new Windows-only tests cover injected positive/negative root facts, strict
GUID/UNC/device/descendant parsing, failed-query classification and private
rendering, in-memory foreign-owner safe/unsafe/OW DACLs, and unchanged private
fixture identity/policy after queries. Injectable query stages additionally exercise
successful independent-root comparison, identity mismatch, reparse substitution,
each fallible stage, and RAII drop counts after open and subsequent failures.
Primary GUID errors and UNC fallback failure/non-UNC/success outcomes are asserted
separately. Structured DACL error tests retain GetAclInformation/GetAce operation
and original numeric code without changing admission error text or kind.
The foreign-owner fixture is a synthetic
stand-in, not a TrustedInstaller authorization test. Reparse and identity mismatch
facts are injected; no machine volume root is mutated. Invalid-handle failure is
queried read-only. Real local-root success, UNC behaviour, standard-user access,
API ABI/error paths, actual OS handle closure and concurrent substitution still
require native execution. Synthetic drop counts verify Rust orchestration, not
CloseHandle success or driver behaviour. Existing junction/substitution regressions remain.

Normal production refusals no longer synchronously repeat unbounded account
resolution. Explicit opt-in still permits repeated lookup/SCM work while directory
handles are held; memory bounds do not bound latency. Volume/file APIs may also
block. Independent root File handles use RAII; native cleanup and failure paths
are unverified. Identity, filesystem and service observations are non-atomic.
SQLite guard lifetime, pathname publication/cleanup and MoveFileExW open-target
error 5 remain unresolved; production sign-off is not implied.

Available Linux validation: cargo check --locked --offline --bins --tests passed;
Phase 1 tests 18/18 and private_storage tests 15/15 passed. Windows-only code/tests
are excluded from these Linux results; only the Linux Rust target is installed.
Targeted rustfmt and git diff --check passed. Native verification must explicitly
opt in through a separately reviewed change; this candidate does not dispatch it.

API references: [final path by handle](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getfinalpathnamebyhandlew),
[volume information by handle](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getvolumeinformationbyhandlew),
[file information by handle](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getfileinformationbyhandleex).

## Authorized production volume-root owner exception — 15 October 2026

The user explicitly approved the exact TrustedInstaller SID
`S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464` as owner
of a verified fixed local NTFS volume root, accepting that owner's implicit
WRITE_DAC authority. This supersedes earlier statements that this owner could
never be admitted. It does not make service names, signatures, LocalSystem
configuration or service-SID classification proofs of trust.

Only an ancestry assessment at depth zero with the volume_root role may use the
exception, never a private-object or inheritable-parent assessment. The exact
native SID comparison must match before any root queries execute. Admission
requires a complete handle-derived GUID root, fixed local NTFS with persistent
ACLs, no root-query error, no original/independent reparse observation and matching
independent and repeated FILE_ID_INFO. The existing DACL assessment must still
pass; no trustee permission exception is added. Query failure/ambiguity, foreign
owners, network/removable/non-NTFS volumes and identity mismatches fail closed.
Intermediate owners/ACLs, private parents, sidecar inheritance, reparse checks,
held-identity rechecks and SQLite/publication containment remain unchanged.

No account lookup, SCM query, executable-signature check or privilege enablement
is used by admission. Filesystem observations are point-in-time, and synchronous
volume/file queries have no intrinsic latency bound. The accepted owner authority
and the pre-existing trust in the current user, SYSTEM and administrators remain
residual risks; this is not protection from those authorized principals.

Molly's separately approved external ACL repair was completed on 15 October 2026
and normal Windows x64 startup with existing real data was verified. The app did
not acquire an automatic repair facility. A later read-only comparison found the
live configuration file had a different file identity and inherited trusted-only
grants; the application root and database retained their protected repair policy.
Mark, SYSTEM and Administrators remain the only allow trustees. Native access
checks grant the ordinary Mark token FullControl; the restricted Codex token has
a smaller effective mask, so owner/grant authority must not be confused with the
effective rights of that tool token. No ACL was changed in the correction pass.
Existing-policy preservation in configuration saves remains deliberate. The
replacement's initiating process is unrecorded; timestamps and file identity do
not prove whether the app, an editor or another actor replaced it.
