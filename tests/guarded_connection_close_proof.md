# Linux synthetic close-ownership proof

Run `cargo test --locked --offline --test guarded_connection_close_proof`.
This integration-test module is compiled only on Linux. Windows and macOS abort
semantics are unsupported and unverified; zero tests there is not proof.
No production module, database path, namespace validation or security policy is
changed. The guard is a drop sentinel, not an actual directory identity guard.

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

Eight tests comprise six ownership cases, a child-entry no-op in the parent, and
the watchdog fixture. The raw native statement intentionally survives only in
aborting children; successful cases finalize it before close. Abort cannot clean
up staging or perform recovery; the parent removes only synthetic fixtures after
reaping. This proof does not establish production shutdown, retry policy, sidecar
security, close-error recovery, guard acquisition, Windows handle retention or
cross-platform terminal behavior. No production connections are migrated.
