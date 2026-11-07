# SQLite failure and shutdown policy — development 1.0.7

This candidate stops accepting work after unconfirmed ownership, then asks for
normal application closure. It does not activate emergency process termination.
Selected Windows ownership/handle-release and macOS safe headless acceptance have passed. Remaining native
scope and the Stage 6 decision are recorded in [ownership status](SQLITE-CONNECTION-OWNERSHIP.md).

## Ordinary operation and recoverable close errors

Statements, transactions and backups borrow `DatabaseConnection`. Explicit close
returns the entire owner on error; a caller retaining it may release the known
borrower and retry. That returned error alone does not latch a fatal condition.
There is no blind retry, ownership extraction or `close_v2` success assumption.

A failed destructor close, an abandoned operation-boundary close, or uncertain
native failed-open cleanup retains the connection/guard/staging resources until
process exit. An ordinary Rust unwind through an unfinished open/close attempt
also records ownership uncertainty. The process-wide latch stores only the first
fixed reason in memory. It writes no journal, SQL row, observer record or file.
It neither formats the underlying error nor tries to repair or recover data.

Once latched, new owning opens are refused before touching storage. The GUI polls
the latch, preserves existing recovery guidance and enters Restart Required before
starting further screen/database work or background refresh. This is not atomic
cancellation of work already in progress or a cross-process security boundary.
Existing instances have their own state; close all instances before recovery.

## Normal closure and restart

Normal close requests, including save/discard confirmation, retain the active
email-test worker guard. Restart Required continues polling that existing worker;
it does not kill it, assume a timeout ended it, or permit duplicate sends.

After restore or an ownership failure, old preparation actions may refer to stale
data. The restart screen offers no Save action. Unsaved preparation remains in
memory until the user explicitly confirms discarding it and closing. Window-close
requests receive the same protection even when another screen is selected.

A restore requiring restart blocks further ordinary application work. A pending
navigation/save dialog is not resumed against restored data. Normal application
teardown checks each owned connection. If ownership uncertainty is observed during
or before teardown, normal application return reports failure with a shutdown
explanation, rather than claiming a clean exit or a startup failure. Reporting
never launches a new eframe window after shutdown.

## Emergency termination remains inactive

No abort, self-TerminateProcess, _exit, forced process exit or watchdog was added.
There is no runtime flag enabling one. A future emergency policy requires its own
native review: exact trigger, unsaved-work consequences, OS reporting/privacy,
resource ownership, and supported platform behavior. A native close failure or
panic inside SQLite cannot be certified by Linux policy tests. Retaining resources
is conservative but can retain locks and consume memory/disk until closure; it is
not proof that a partial native handle closed or a completed transaction rolled
back. Dependency-level panic/FFI failures remain release limitations.

Catastrophic database failure is handled by restoring a known-good user backup.
There is no independent observer journal, automatic salvage/reset, parallel
recovery database or elaborate terminal-recovery framework. See
[offline recovery](OFFLINE-RECOVERY.md).

## Accepted scope — 1 November 2026

The continuation review retains this conservative normal-close policy. Failed
native cleanup/close uncertainty, retained locks/resources, uncancelled in-flight
work and potentially indefinite worker delay are accepted limitations of the
ownership change, not verified native failure outcomes. No bounded shutdown time
or universal SQLite pathname/sidecar confinement is promised. Ordinary caller
unwind with successful close has evidence; panic inside native/dependency close
does not. Cross-platform sign-off remains subject to the outstanding native scope
in the ownership status. Emergency termination and custom VFS work are excluded.

## Bounded macOS acceptance — 1 November 2026

[Actions run 41305531150](https://github.com/arnieSkyNet/DirectPaymentTimesheets/actions/runs/41305531150)
compiled the application and passed 43 exact safe tests on Intel and 43 on Apple
Silicon for candidate `d9b046bed1739843f854e57968ab5ce9947b26ce`, using controller
`1afde972f46f0f66438952bbd56e6e8ec0aa1d6c`. The selection includes first-failure
admission, headless worker/discard/close protections and ordinary caller unwind.
It does not establish interactive GUI behaviour, bounded worker completion,
native failed-open cleanup, dependency/native-close panic, crash durability or
universal SQLite pathname confinement. No emergency termination or experimental
native fixture was executed. The limitations above remain unchanged.
