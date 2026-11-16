# Security remediation and acceptance boundary — 16 November 2026

This working-tree change descends from clean source commit
`995c3c8cf179dd522d595c20bbacce40a9b29304`. It is not a release, a new immutable
approval identity or a claim of current-source native acceptance. No production
records, private original repository, live-provider accounts or GitHub settings
were accessed or changed. Existing native controller/source gates remain intact.

## Dependencies and compatibility

The lockfile resolves Rustls 0.23.46, rustls-webpki 0.103.17 and webpki-roots
1.0.12, fixing the audit's active HTTPS trust findings. Microsoft OAuth endpoint,
scope, device authorization, refresh, token storage and redacted diagnostics are
retained. Synthetic transport/token regressions verify local behaviour; they do
not claim a new live Microsoft authorization or refresh.

Imported PDFs use pdf-extract 0.12.1/lopdf 0.42.0, with bounded parser nesting and
a 64 MiB streaming input ceiling. Generated PDF layout stays on printpdf 0.8.2.
Its lopdf 0.35.0 dependency is retained solely for rendering and synthetic output
tests; production must never route imported PDF bytes through that parser.

Rusqlite 0.41.1 bundles SQLite 3.54.1 (libsqlite3-sys 0.39.1), fixing the crafted
WAL memory-disclosure advisory. Schema remains 42. Owning connections, held guards,
close uncertainty/restart requirements and transaction-based recovery remain.
The bounded quick-check uses a scoped native progress callback because the new
Rusqlite hook registration needs a mutable connection while the existing writer
transaction borrows it. The callback state outlives the guard, which clears it on
return/unwind; no connection is reconstructed from a raw handle or disowned.

Tokio, Rustix and Zstd dependencies were updated to compatible patched versions.
The exact remaining exceptions and their 16 February 2027 review deadline are in
[the machine-readable policy](dependency-advisory-exceptions.json). These cover
unused ZIP extraction APIs, the renderer-only PDF parser, build-time Rand logger
conditions and maintenance advisories. They do not exempt new advisories or
versions. The checker rejects expired/stale/unknown exceptions and unsupported
version-range syntax. Reassess reachability whenever these call paths change.

```sh
curl --fail --location https://api.github.com/repos/RustSec/advisory-db/tarball --output /tmp/dpts-rustsec.tar.gz
python3 scripts/check_dependency_advisories.py /tmp/dpts-rustsec.tar.gz
```

RustSec does not cover every C-library issue. Vendored libdbus 1.14.4 remains a
client library, not the dbus-daemon server covered by several November D-Bus DoS
reports. Do not count daemon-only findings as proven application exploits.
System OpenSSL and every copied AppImage library require target-specific patch
inventory; an optional openssl-src lock entry does not prove it was linked.

## Private storage, mail and memory

On Unix, application roots/internal directories are assessed at startup, and database/config file admission now refuses foreign owners, group/other
permission bits, multiple hard links and (on macOS) extended allow ACLs. Existing
database sidecars receive the same checks. No chmod/ACL repair is performed.
New private creation remains 0600/0700. Deliberately shared external business
documents retain their existing publication policy. This does not provide a
descriptor-relative SQLite VFS or atomically eliminate pathname races, and does
not certify ancestor ACLs or remote filesystems. Use a private trusted local root.

Before using an installation that is refused, close every instance, preserve a
consistent verified recovery copy and the separate external file estate, assess
the exact paths, then review a specific permission repair independently. Do not
delete journals, recreate the database or recursively chmod shared folders.

Unauthenticated remote plaintext SMTP remains available only after explicit
`allow_remote_plaintext` consent in SMTP Config. Missing legacy settings default
to refusal. Consent never permits plaintext password/OAuth authentication. Local
SMTP remains localhost:25; its administrator must secure onward transport. SMTP
providers, STARTTLS, implicit TLS, recipient rules and send uncertainty are retained.

Discarded EmailConfig drafts/clones zeroize their password and baseline strings
on Drop. This is partial hygiene, not proof of erasing GUI/dependency copies,
swap, kernel buffers or a privileged debugger's memory. Unix startup disables
ordinary process core dumps before loading private data. Windows dump policies,
macOS diagnostic reports and externally collected crash reports remain OS/privacy
acceptance matters; they are not claimed disabled by this change.

## Backups and retention

Payroll SQLite/BLOBs, external documents, signatures and backup snapshots remain
unencrypted at the application layer. Use appropriately encrypted storage and
encrypted, access-controlled offline backups. Do not upload raw profiles, crash
dumps or support logs containing paths/identities to public issues. Historical
plaintext credential files/backups need a separate retention/exposure review;
migration does not erase them or revoke a possibly exposed password.

Database/config snapshots are not complete backups: separately protect external
PDFs, payslips, signatures, CSV/import/archive evidence and the required OS-vault
recovery arrangements. Review legal/accounting retention with the operator and
implement approved deletion across copies, exports and backups. No automatic
retention purge, secure-erasure claim or business-record deletion was introduced.
Perform restoration rehearsals with synthetic copies in isolation.

## Licensing, history and remaining acceptance

The generated notice inventory covers every exact locked registry entry and
nested native notices. See [corresponding-source arrangements](CORRESPONDING-SOURCE.md).
Packaging refuses a dirty source tree. Original publisher/maintainer attribution
and third-party author attribution are retained intentionally. The fixed Windows
development profile remains restricted to its previously reviewed path; its local
account identifier is not silently substituted, since that would change its
confinement boundary. Historical workstation approval records remain evidence,
not employee records. The obsolete alternate main source file is removed.

All ten clean-baseline build artifacts exist locally; their hashes, targets and
build-run identity were checked against manifests. Their [inventory](CLEAN-BASELINE-PACKAGES.json)
is retained. Their contents can establish baseline packaging evidence only.
They cannot establish that later dependency, ACL or memory changes work natively.

Before a remediated binary release: review/commit an exact candidate; build a new
same-source package set; verify its corresponding-source/native annex and notice
payloads; run applicable changed-source Windows/macOS/private-storage/SQLite and
packaged HTTPS/OAuth acceptance. Reuse existing live-provider/hardware evidence
where source equivalence and scope justify it, documenting the binding; do not
repeat live tests merely because an old checkpoint lists them as pending. Do not
invent an equivalence or enable a disabled workflow without independent approval.
Unsigned Windows/ad-hoc macOS trust and minimum-OS/GUI limits remain explicit.

## Local verification record

The changed Rust dependency graph and test targets compile on Linux. Across the
bounded broad application run and focused/serial completion, all 1,131 listed
application cases were accounted for: 1,128 distinct passes and three explicitly
ignored native credential cases. The first broad attempt needed a synthetic
localhost hosts mapping, explicit plaintext consent in loopback fixtures, and
serial completion after its 300-second bound. These are combined results, not a
claim of one uninterrupted full-suite pass. The nested-PDF regression checks
import refusal (the bounded parser may recover by omitting malformed objects).

Coverage includes 30 synthetic Microsoft OAuth tests, 23 SMTP transport tests,
13 preparation-import regressions, existing payroll/calculation/recovery/GUI
regressions and 23 guarded SQLite integration passes. No live Microsoft or other
provider authorization, production profile, native Windows/macOS run or installer
launch was performed. Python controller checks and packaging checks also pass;
explicitly scoped/native cases and release packaging from an uncommitted source
remain intentionally skipped. Local raw test logs are ignored build evidence,
not publication payloads.

## Focused follow-up: three confirmed review defects

Notice staging now uses a generated, independently reviewed exact-file/hash
allowlist for Linux, macOS and Windows. Unexpected source/staging files are
rejected; ignored `.key`/`.db` files cannot be picked up recursively. Required
inventories and attribution fail closed, including empty manifest notice maps.
See the corresponding-source guide for regeneration and review requirements.

Unix managed-file opens use `O_NOFOLLOW | O_NONBLOCK`, reject non-regular opened
handles, then restore blocking mode for accepted regular files before existing
ownership, mode, hard-link and ACL admission checks. A bounded subprocess
regression covers FIFO input/lock/database paths and journal/WAL/SHM sidecars,
symlinks, directories and unchanged ordinary-file access. Test data is fictional.
This follow-up does not establish new native installer or macOS acceptance.

Focused verification for this follow-up: 11 Python dependency/notice regression
cases, one existing real-tree licence-staging case, and 13 private-storage Rust
cases passed on Linux (including the bounded FIFO child). The generated-notice
check verified 2,481 generated outputs and all locked archive checksums; exact
installer validation admitted 2,506 third-party files. Corresponding-source
preflight and `git diff --check` passed. No full application suite, release build,
download, native Windows/macOS execution or live-provider test was performed.

## Final independent source review — 16 November 2026

The complete application/documentation/packaging diff and intended new files were
reviewed against the clean baseline. Generated notices were verified byte-for-byte
against checksum-verified locked archives; public upstream author attribution was
retained. The candidate-file scan found no private-key PEM, GitHub/AWS credential
patterns, or newly introduced payroll/database/document/binary files. This is
scoped evidence, not a guarantee that arbitrary text contains no personal data.

Additional corrections bind Windows manifest parsing to the exact hashed bytes,
capture the final NSIS installer digest synchronously, and require the same digest
through snapshot promotion. macOS now verifies notices in the mounted image and
uses the same snapshot/provenance binding. Source exports verify complete Git-tree
coverage and each committed blob identity, rather than reading mutable working
files. Source archives are rechecked at inclusion; vendoring reuses verified
archive bytes and refuses Windows-style traversal. The generated approval JSON
also retains exact bytes across Windows checkouts.

Current verification: 28 focused Python cases passed; packaging regression checks
had 16 passes and one dirty-source-dependent synthetic package case skipped before
commit. Reused compiled Rust tests ran inside an isolated empty-profile/network
namespace: private storage 13, database recovery 29, preparation import 13,
Microsoft OAuth 30, SMTP transport 23 and connection ownership/SQLite 9 passed.
Two native OAuth cases remained intentionally ignored. No Rust sources or Cargo
manifests were newer than that test executable. Earlier broader regression
coverage remains historical evidence above; this was not a new full-suite run.

Notice verification covered 2,481 generated outputs, the unchanged 2,506-file
third-party inventory and 703 exact crate archives. The retained 16 November
RustSec database SHA-256 is
`0f42d5103eb52483efd5b3eb6f81b1db1c3686f044c0848030c877c3b7bd9011`:
19 exact dispositions matched, zero unreviewed and zero stale. These are 15
advisory IDs covering 19 package/version/advisory triples, not 19 independent
vulnerability fixes. Their review deadline remains 16 February 2027.

The installed Linux 7-Zip 25.01 lists the baseline NSIS installer with
`BadCmd=13` and omits its generated uninstaller. It is not accepted as the new
Windows payload verifier; native Windows must demonstrate a compatible extractor,
sharing/rename locks and exact installer/uninstaller hashes without relaxing gates.
Native macOS image/private-storage acceptance, same-source packages, packaged
HTTPS/OAuth evidence binding and the distribution-native source annex remain
binary-release requirements. No live-provider test was repeated, no native
acceptance invented, and no production data or original private repository used.
This review prepares a local source candidate; it does not approve publication.
