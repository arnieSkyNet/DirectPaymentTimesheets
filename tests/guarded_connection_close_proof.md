# Linux synthetic close-ownership proof

Run `cargo test --locked --offline --test guarded_connection_close_proof`.
This integration-test module is compiled only on Linux. Windows and macOS abort
semantics are unsupported and unverified; zero tests there is not proof.
No production module, database path, namespace validation or security policy is
changed. The wrapper owns synthetic directory/database handles plus a drop sentinel;
these handles are not a production directory identity guard.

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

Fifteen tests comprise thirteen ownership/database/cleanup cases, a child-entry
no-op in the parent, and the watchdog fixture. The raw native statement
intentionally survives only in aborting children; successful cases finalize it before close. Abort cannot clean
up staging or perform recovery; the parent removes only synthetic fixtures after
reaping. This proof does not establish production shutdown, retry policy, sidecar
security, close-error recovery, production guard acquisition, Windows handle retention or
cross-platform terminal behavior. No production connections are migrated.

## Synthetic handle and staging extension

Only this integration-test crate includes the unchanged existing Linux
`DirectoryCleanup` source. New staging requests mode 0700 and the helper verifies
access and captures the original directory and parent. Each connection owns
O_NOFOLLOW directory/database descriptors acquired before SQLite opens. Field
ordering and checked close keep descriptors until confirmed closure; the final
shared staging owner uses the existing anchored cleanup. Two same-database
connections share staging ownership so closing one cannot delete the directory
used by the other. No wrapper or API is exported to production, and the mutable
connection accessor is private to this synthetic proof.

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
