# SQLite connection ownership — v1.0.7 candidate status

Production writable/read-only openers and repository fields now use
`DatabaseConnection`. Existing query helpers retain shared `&Connection` access;
there is no `DerefMut`, ownership extraction or production raw-connection
constructor. Repository constructors accept an owner. Raw synthetic fixtures
have a test-only compatibility conversion, excluded from production builds.
Mutable transactions and backup handles borrow the owner(s). Existing mutable transaction helpers now borrow `&mut DatabaseConnection`,
retaining compile-time exclusivity and the same Immediate mode and existing
locks. SQL and payroll calculations are unchanged.

An unsuccessful explicit `close` returns the complete owner and SQLite error.
Writable registration runs after ownership is established. The owning opener
returns the live owner on registration failure; compatibility openers report the
original SQLite error after checked owner destruction. Read-only opening performs
neither registration nor migration.

The guard bundle is established before native opening. Because pinned rusqlite
cannot expose partial native handles/cleanup results on failed open, that bundle
is conservatively retained on native open error or unwind. Normal destruction
closes before releasing guards and the last shared staging reference. Failed
close retains the connection and bundle until process exit. Operation-boundary
`close_or_retain` reports failure without implicit retry or staging release.
An in-memory first-failure latch now blocks new owning opens and puts the GUI
in Restart Required. Normal closure respects existing workers and unsaved-change
confirmation. No emergency termination is active. Panic inside dependency
close/destruction and native failed-open cleanup remain unverified; retained
resources and a latch do not prove native closure. See [failure policy](SQLITE-FAILURE-POLICY.md).

Backup creation and restore staging use explicit source/destination owners and
borrowed `Backup`, preserving the previous 100-page steps and immediate
BUSY/LOCKED refusal. Existing read snapshots, WAL-inclusive copying, fingerprints,
integrity checks, writer reservations, source checks and flushes remain. Checked
closure precedes successful backup publication. Startup/migration/restore stages
hold `Arc<PrivateTempDir>` independently of caller-local directory variables.
Approval plans retain their own shared reference; a connection cannot outlive it
without its own staging reference. A reviewed restore stage is closed before the
live installation transaction commits. Post-commit close errors explicitly require
restart rather than claiming rollback.

Failed backup population no longer recursively removes its directory. A private
`.incomplete` completion marker remains on failure; discovery and validation
refuse that backup. The marker is removed only after successful population,
verification, checked closure and file/directory flushing. Old successful backups
without the marker remain compatible. This is ordinary backup completion
bookkeeping, not an independent observer journal or disaster-recovery ledger.

Windows owners retain the existing ancestor handles until confirmed closure.
Owner/ACL/inheritance/reparse policy is unchanged and is never repaired. Unix
currently supplies no additional handles through this existing guard API. Neither
retained descriptors nor this wrapper confine SQLite's internal pathname/sidecar
operations. Shared staging is trusted lifetime attachment, not proof of pathname
membership. No custom VFS, dependency fork or new catastrophic recovery system is
introduced. Catastrophic recovery remains restoration from a known-good user
backup; external business files require separate protection.

## Stage 6 acceptance reconciliation — 1 November 2026

This continues the existing six-stage review; it does not restart it. Stages 1–3
(connection owner, production migration, backup/staging ownership) and Stage 5
(failure admission and conservative normal shutdown) are implemented. No production
ownership or shutdown code was changed during this reconciliation.

| Evidence | Accepted result | Boundary |
| --- | --- | --- |
| Florence Linux | 239 distinct runtime tests and nine compiler borrowing checks; normal binary check passed | Reused verified evidence, not a new suite run. Includes repositories, backup/restore, ownership, failure admission and headless shutdown protections. |
| Molly Windows Stage 4 resume, 1 November | 10 exact-name unit tests and four supplemental production-module handle-release probes passed | Native x64 MSVC, private admitted NTFS fixture; no GUI or terminal execution. |
| macOS Intel, 1 November | Application compilation and 43 exact safe tests passed | Native `macos-15-intel`, `x86_64-apple-darwin`; headless synthetic fixtures only. |
| macOS Apple Silicon, 1 November | Application compilation and 43 exact safe tests passed | Native `macos-15`, `aarch64-apple-darwin`; headless synthetic fixtures only. |

[Actions run 41305531150](https://github.com/arnieSkyNet/DirectPaymentTimesheets/actions/runs/41305531150)
completed successfully on both architectures: 86 selected test executions in total.
Both completion receipts identify immutable candidate
`d9b046bed1739843f854e57968ab5ce9947b26ce` and controller
`1afde972f46f0f66438952bbd56e6e8ec0aa1d6c`. Each receipt verifies the 124-file
published packet, the source/runner/allowlist/lock hashes and effective candidate
workspace. The current development working tree matches all 117 candidate
source/support files in bytes, size and executable mode. This binds the result
to that source snapshot, not automatically to later edits or another branch tip.

The 43 selected cases per architecture cover ownership (8), failure policy (4),
headless shutdown (8), backup (8), recovery (8) and macOS/private-storage checks
(7). The application was compiled, not launched. No integration/terminal fixture,
real SMTP/Keychain test, native-close panic, interactive GUI or production data
was exercised. Safe real prepared-statement busy-close/retry and ordinary caller
unwind were included; dependency/native close failure injection was not.

Molly's successful scope includes writable registration, read-only refusal and
noncreation, registration-error ownership, shared staging, WAL-inclusive borrowed
backup, ordinary caller unwind, native object-owner checks and ancestor protection.
Four separate probe cases demonstrate native ancestor rename denial while owned
and release after explicit close, successful destructor close, read-only close and
the last of two owners. Observation handles were dropped before repeating the
live-owner denial check. These are four probe cases, not four extra unit tests.

The previous Molly-access, Projects-fixture and Stage-5-unimplemented blockers are
obsolete. Projects ancestry was correctly refused by the unchanged policy; the
resume used a new private child directly under the admitted profile ancestry.
No ancestor permissions were repaired. All 334 transferred leaves reverified
without mismatches; the prior comparison of 114 Florence build inputs remained
applicable to that unchanged snapshot. The prior review independently rechecked
those 114 inputs and three Florence evidence manifests. This reconciliation
carries those recorded results forward; it does not claim a new 114-input check.

### Accepted limitations for this ownership change

- Failed native open cleanup is opaque inside pinned rusqlite. Retaining the
  guard/staging bundle does not prove the partial native handle closed. Panic
  inside dependency/native close remains unverified. Explicit recoverable close
  errors return the complete owner; abandoned/uncertain closure retains resources
  and latches restart-required. Locks, memory and staging can remain until process
  exit. No guarantee of prompt release, rollback or clean native closure is made.
- The latch refuses new owning opens and gates subsequent GUI work; it does not
  atomically cancel in-flight operations or affect another process. An existing
  worker can delay normal closure indefinitely. Normal close continues to protect
  that worker and require explicit discard of unsaved preparation. No forced
  termination, watchdog or automatic timeout is required by this accepted policy.
- Lifetime ownership and Windows ancestor guards do not confine all SQLite
  pathname, journal, WAL or SHM operations. Unix guard acquisition adds no such
  confinement. Shared staging is trusted lifetime attachment, not a checked
  database-path membership proof. Universal pathname confinement is outside this
  acceptance claim; the existing account-authority boundary remains in force.

These are accepted scope limitations under the 1 November continuation instruction,
not evidence that untested failure paths passed. See [failure policy](SQLITE-FAILURE-POLICY.md).
Catastrophic recovery remains restoration from a known-good user backup.

### Outstanding native scope and sign-off

Windows production-owner real busy-close verification, broader applicable native
policy-refusal cases, and interactive native GUI shutdown remain unverified. The
existing real prepared-statement busy-close test is safe/nonterminal in intent,
but was excluded from Molly's executed selection and is not counted as a pass.
Florence headless worker/close tests do not establish Windows GUI acceptance.

At the earlier Stage 6 checkpoint, no further Windows run was practical: SSH socket creation was
denied by the command-runner sandbox. This was an execution restriction at that checkpoint,
not a renewed claim that Molly lacks a configured route. GitHub API inspection
also failed to connect. Local workflow inspection found no established safe
current-candidate macOS acceptance route; the experimental terminal workflow has
zero source pins and remains inactive. No workflow was enabled or dispatched,
no target installed and no Mac runner assumed at that checkpoint. These recorded
networking/macOS-route observations are historical: networking and SSH were later
verified, the separate manual-only controller was published, and run 41305531150
established the bounded native macOS results above. The experimental terminal
workflow remains inactive; it was not the acceptance route.

**Disposition:** the ownership implementation is ready for bounded source/policy
approval with these limitations accepted. Full cross-platform v1.0.7 SQLite
security sign-off remains pending disposition of the remaining Windows native
scope and interactive native GUI acceptance. The current-candidate macOS
compilation/safe headless acceptance gate is satisfied by run 41305531150, within
the documented limits; this is not full cross-platform or release sign-off.
Experimental Stage 3 terminal machinery, abort/watchdog tests, observer journals
and a custom VFS are not prerequisites and remain inactive.

### Evidence preservation

Durable continuation evidence is under
`/home/mark/DirectPaymentTimesheets-security-evidence/sqlite-acceptance-20261101/`.
Its `REPORT.md` records provenance and preservation checks. Earlier Florence
reports originated under `/tmp/dpt-sqlite-stages4-6-tqouqp1w/` and companion packets;
those temporary directories are unavailable after reboot. Recorded report outputs
were recovered from the previous review transcript without changing it. This is
report recovery, not reconstruction or fresh verification of the missing raw logs.
Molly's original `target\windows-sqlite-stage4-resume-20261101\REPORT.md` and
associated receipts remain the Windows evidence reference; they were not fetched
again in this restricted session. Historical reports retain their original status
statements as history, not current blockers. The macOS run and immutable identities
above supplement those historical records; they do not recreate missing raw logs
or broaden earlier Windows/Linux claims.
