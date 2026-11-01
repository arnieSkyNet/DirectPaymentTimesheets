# Linux synthetic close-ownership proof

Stage 2 nonterminal validation (do not run the terminal cases without separate
approval):

```text
cargo test --locked --offline --test guarded_connection_close_proof -- --test-threads=1 --skip busy_destructor --skip pending_write_abort --skip child_watchdog
python3 scripts/test_sqlite_owner_api.py
```
This integration-test module is compiled only on Linux. Windows and macOS abort
semantics are unsupported and unverified; zero tests there is not proof.
No production module, database path, namespace validation or security policy is
changed. The wrapper owns checked synthetic directory/database handles plus a drop
sentinel; these checks are not a production SQLite VFS or namespace guard.

The pinned rusqlite 0.31.0 `test_close_retry` supplies the deterministic fixture:
an outstanding `sqlite3_prepare_v2` statement makes actual `Connection::close`
return `SQLITE_BUSY` and the still-live connection. No SQLite error is mocked.
The proof returns the complete wrapper, checks the sentinel remains absent,
steps and finalizes the statement, and retries successfully. Successful explicit
close and destructor close set the sentinel's closure flag only after SQLite
reports success. Ordinary panic unwinds normally and closes before guard drop.
Terminal cases first independently confirm real busy close, then exercise
destructor abort both outside and during unwinding. The external sentinel must
remain absent, the child must terminate with SIGABRT, and no return marker may
exist. These checks demonstrate ordering, not production namespace protection.

Every substantive case runs in a separate child with an empty environment,
discarded output and a private synthetic temporary directory owned by the parent.
Before any deliberate abort, the child sets and verifies both soft and hard
RLIMIT_CORE to zero and PR_GET_DUMPABLE to zero after PR_SET_DUMPABLE. Failure
prevents entry into the abort fixture. The parent checks its own limits and
dumpability are unchanged, and rejects any core-dumped exit status. No database
contents or fixture paths are printed by the proof.

The child has no descendants. Before exec, its launcher installs and unblocks
default SIGALRM, arms a five-second watchdog preserved across exec, and sets
SIGKILL on parent-thread death with a parent-PID race check. A dedicated fixture
proves the watchdog terminates a waiting child and the parent reaps it. Parent
polling has a ten-second budget; timeout or polling failure triggers SIGKILL and
at most two seconds of nonblocking reap polling. Unconfirmed termination or reap
fails the test, never passes it. There is no blocking `Child::wait` call. These
are userspace bounds, not a guarantee against an unresponsive kernel or a process
stuck in uninterruptible kernel I/O. Such a failure blocks acceptance of the proof.

The historical sixteen tests comprise fourteen ownership/database/cleanup cases, a child-entry
no-op in the parent, and the watchdog fixture. The raw native statement
intentionally survives only in aborting children; successful cases finalize it before close. Abort cannot clean
up staging or perform recovery; the parent removes only synthetic fixtures after
reaping. This proof does not establish production shutdown, retry policy, sidecar
security, production close-error recovery, production guard acquisition, Windows handle retention or
cross-platform terminal behavior. No production connections are migrated.

## Synthetic handle and staging extension

Only test code includes the unchanged existing Linux
`DirectoryCleanup` source. New staging requests mode 0700 and the helper verifies
access and captures the original directory and parent. Each connection owns
O_NOFOLLOW directory/database descriptors acquired before SQLite opens. Field
ordering and checked close keep descriptors until confirmed closure; the final
shared staging owner uses the existing anchored cleanup. Two same-database
connections share staging ownership so closing one cannot delete the directory
used by the other. No wrapper or API is exported to production. Stage 2 removes
the mutable connection accessor in favor of borrowed transactions and backups.

Actual SQLite tests create and observe a DELETE rollback journal, explicit
rollback and transaction unwind, WAL/SHM, two simultaneous connections and a
TRUNCATE checkpoint. The staging destructor checks SQLite removed WAL/SHM after
final close BEFORE directory cleanup can mask them. Source and destination
backup connections each own their handles/staging; `backup::Backup` borrows both
and is dropped before either checked close. No pathname convenience backup is
used. The small fixture completes in one backup step inside the child budget.

The registration-failure fixture sets SQLite's busy timeout and executes invalid
synthetic SQL, returning the complete live owner and real error. It checks the
connection remains usable and no guards or staging were destroyed. The open
failure fixture uses invalid flags rejected by pinned rusqlite BEFORE its native
open call; it confirms SQLITE_MISUSE and safe disposal of unopened resources.
It does not prove native VFS open-failure cleanup or all production registration
failures. Successful-close ordering for this unopened case means no SQLite
connection ever existed, not a successful sqlite3_close call.

The namespace-substitution fixture renames the original staging while open and
creates a replacement with harmless contents. It executes no further SQL after that substitution, then closes the connection.
This does not assert that SQLite close avoids internal pathname operations. Existing anchored
cleanup must refuse, leaving BOTH original database and replacement contents.
Ordinary other-user protection and malicious same-user/privileged exclusions
remain exactly those of the existing helper. The parent removes only its entire
synthetic temporary fixture after the child is reaped; this test teardown is not
production cleanup.

Unix held directory descriptors do NOT confine SQLite internal pathname opens,
prevent ancestor rename, or establish database/sidecar path identity. The test
uses a controlled synthetic namespace; it demonstrates resource ownership and
cleanup ordering, not a VFS security boundary. Windows/macOS implementation and
abort behavior, native VFS failure cleanup, production terminal privacy/data-loss
policy and complete production connection migration remain unverified or blocked.

## Pending-write rollback recovery experiment

One additional Linux test commits a synthetic baseline row, verifies DELETE
journal mode, begins an uncommitted insert with an 8192-byte synthetic blob, and
calls real `sqlite3_db_cacheflush` to force dirty database pages to disk. A native
prepared statement then causes actual SQLITE_BUSY on checked close. The returned
owner must retain its pending transaction, descriptors and staging. A fixed phase
marker is written only after these assertions, immediately before destructor
abort. The existing verified child-local dump controls and watchdog apply.

After reaping the child, the parent requires SIGABRT, busy and pending-phase
markers, and absence of guard/staging/return markers. Before any recovery open,
it requires the original database, a nonempty rollback journal with SQLite's
journal magic and recorded pages, and database bytes different from the saved
committed baseline image. Only synthetic bytes are compared; none are printed.
These observations establish that the test needs on-disk rollback recovery rather
than merely losing a cached insert. The retained directory is found from the
single-directory inventory of the parent's isolated fixture, never from a
production path or child-supplied arbitrary pathname.

A second independently bounded Linux child, with its own verified dump controls,
opens that SAME database using ordinary rusqlite READ_WRITE/NOFOLLOW without
CREATE. SQLite performs recovery; integrity_check must return ok, the baseline
row must remain, and the uncommitted row must be absent. Explicit close must
succeed and the DELETE journal must be removed. Staging remains present until
parent test teardown, with no wrapper cleanup markers. Recovery failures use
fixed messages and child output is discarded. Parent dump settings are checked
unchanged for both children.

This demonstrates one real rollback-journal recovery case on the tested bundled
SQLite/Linux configuration. It is not WAL evidence, a power-loss/durability test,
a custom-VFS failure proof, a production recovery-service test, or evidence that
all close errors preserve recoverability. It does not decide whether production
may abort with unsaved work or how recovery material and crash reports should be
handled. Windows/macOS remain unsupported and unverified.

## Stage 2 owning prototype — 20 October 2026

The existing `ProofConnection`, `Staging` and close/drop implementation were moved,
not duplicated, into `tests/support/sqlite_owner_prototype.rs`. Only the existing
Linux integration target includes this module. Production repositories and all
Stage 1 production changes remain untouched. The module is Linux-specific, not
a new cross-platform storage policy.

The private connection and guard fields cannot be extracted by callers. A
shared staging accessor allows additional owners, but cannot detach the wrapper's
own reference. There is no raw connection constructor, `into_inner`, `Clone`,
`DerefMut` or `&mut Connection` accessor. Statements borrow a shared connection;
transactions and `backup_into` borrow the owner(s). Checked close returns
`Err((Self, Error))`, restoring the live connection before returning both namespace
and staging ownership. The order remains: finish borrowers, confirm SQLite close,
release namespace handles, then release the final staging owner/anchored cleanup.

The guard checks absolute fixture layout, each ancestor's type/owner/write policy,
private root/staging modes, and the regular, current-user, 0600, single-link database
reservation before SQLite opens. It holds no-follow directory/file descriptors
and rechecks named device/inode identities and access policy. Root-owned sticky
temporary ancestry is allowed; an unknown/unmapped owner is refused, not treated
as root. Strict fixtures may therefore require an authorized run outside a UID-
mapping sandbox. This is an environment limitation, not a reason to weaken checks.

These are observation-time checks of the held objects and their names. Unix
descriptors cannot prevent rename or constrain SQLite's internal pathname opens.
The guard does not obtain SQLite's own main-file descriptor, prove an atomic
binding to it, or guard every journal/WAL/SHM/implicit temporary file. Substitution
is detected by explicit revalidation; the existing cleanup test then refuses
substituted staging without running further SQL. Privileged/full-authority
same-user actors remain outside the established boundary. No Windows no-delete-
sharing, DACL or macOS ACL guarantee is inferred from Linux results.

Seven additional runtime regressions cover repeated real SQLITE_BUSY failures,
registration failure followed by busy close, another connection closing while a
failed-close owner retains staging, both backup owners failing close, broad
private-staging refusal, database-symlink refusal and main-file substitution
detection while the original handle remains live. Existing WAL, unwind,
registration, backup and substitution regressions use the strengthened owner.
There are now 23 entries: 19 selected nonterminal entries (including child-entry
control) and four deliberately unselected historical terminal/watchdog entries.
The selected set is not a fresh terminal-recovery acceptance run.

`scripts/test_sqlite_owner_api.py` compiles the actual support module without
executing it. It obtains exact dependency rlibs from Cargo's compile-only artifact
records. A positive borrowing control must compile; seven negative cases must
produce their expected compiler errors: raw connection/private staging access,
mutable connection escape, and close while a statement, transaction or either
side of a backup remains borrowed. Unrelated compiler errors cannot pass a case.

### Deliberate unresolved boundaries

- Pinned rusqlite 0.31.0 `InnerConnection::open_with_flags` calls
  `sqlite3_close` on native open/setup failure, discards the close result, and
  returns only `Error`. The prototype cannot recover an unknown partial C handle
  through that API. Its `?` error path is not proof of retained guards through
  native VFS cleanup failure. The existing invalid-flags case is rejected before
  native opening; it must never be reported as that missing proof.
- Synthetic registration failure is real invalid SQL after opening. It returns
  the complete usable owner. This proves that prototype path, not every native
  registration or error during cleanup/unwinding.
- The borrowed backup API owns no new pathname connection. Production still
  uses two pathname convenience backups whose pinned implementation opens a
  hidden destination with `Connection::open`; Stage 2 does not migrate them.
- Shared `&Connection` is a trusted borrowing surface, not a capability sandbox:
  callers could still issue ATTACH SQL or call its pathname backup convenience
  method. This prototype proves no safe raw *ownership* escape, not prohibition
  of all additional SQL/VFS opens.
- Ordinary successful destruction and panic unwind close before releasing
  guards. The historical test-only destructor's abort fallback is retained,
  not newly approved or exercised here. Unrecoverable destructor close failure
  still has no approved production policy. A panic *inside* close or a guard
  destructor is not covered by the ordinary caller-unwind test and can defeat
  the intended ordering; universal panic containment is not established.
  Indefinite quarantine, ignored close
  errors and `close_v2` zombie success do not establish the required invariant.
- Windows self-TerminateProcess/macOS _exit, native rollback/WAL recovery,
  external supervision, residual crash-report/privacy risk, unsaved-work loss,
  graceful application-wide shutdown and production activation require their
  previously specified separate native evidence and approvals. Linux success
  does not close those gates; no terminal case or watchdog is selected here.

Source references checked for this design: pinned local rusqlite `lib.rs`
(`Connection::close`), `inner_connection.rs` (native open/error cleanup and Drop),
`backup.rs` (pathname destination opens), plus the preserved 13 October Phase 1
instructions. Earlier proof results remain historical rather than relabelled
as current production acceptance.
