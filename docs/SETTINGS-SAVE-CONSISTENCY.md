# Payroll Settings save boundaries — v1.0.7/schema42

Assessment at source `d0d9ea877c9e50af3fc3b8fda1dd2ba30339d563`, 7 November
2026. The completed failed-save fix keeps effective rounding and holiday region
unchanged on error, preserves draft values and the dirty/blue Save state, and
activates matching persisted/runtime rules after successful publication. This
review adds precise error wording and isolated partial-write/retry coverage; it
does not introduce transactions, compensation, new persistence or a migration.

## Actual sequence

`PayrollSettingsScreen::save_settings` opens an owned database connection and
holds the existing production lock. It validates annual-leave input and builds
an in-memory configuration candidate, then:

1. Reads the existing payroll-department address and saves provider ID 1 through
   `PayrollProviderRepository::save` on that repository's connection. Name,
   general contact email, address and telephone are replaced; the separate
   payroll-department email is carried forward unchanged.
2. Validates/saves the singleton annual-leave row through its separate repository
   connection: recurring boundaries, statutory weeks and accrual percentage.
3. Calls `Application::save_config_candidate`. `AppConfig::save` serializes the
   candidate, verifies/prepares its credential reference, creates/writes/syncs
   a private temporary file, preserves target access and atomically publishes it.
   Unix then syncs the parent directory. The candidate's verified email state is
   returned without a post-publication configuration reload.
4. Only after success, installs the candidate into active configuration, normalizes
   the annual-leave editor, updates the form's saved baseline and reports success.

The provider and annual-leave statements are individual SQLite commits, not a
shared transaction with each other or with `config.toml`/the native vault.
The production lock serializes cooperating workflows; it does not make these
stores one transaction or exclude arbitrary noncooperating writers.

## Possible partial outcomes

| Failure point | Completed writes / consequence |
|---|---|
| Open, lock, input validation or provider read | No settings writes by this save. |
| Provider statement refused before commit | Later annual-leave/configuration stages do not run. |
| Annual-leave statement refused | Provider details can already be committed; annual-leave values/configuration remain old. |
| Credential preparation or configuration encoding/file creation/write/access/publication fails before rename | Provider and annual-leave values can both be committed; existing configuration remains old. A newly verified native credential entry can be retained without a published reference, as under the existing credential policy. No old entry is deleted. |
| Unix directory sync fails after successful rename | Provider/annual-leave and the new configuration file can already be installed, while the method returns an error and active rounding/region remain old. This is an uncertain durability boundary, not proof that the file was rolled back. Inspect/reload the disposable profile before retrying this exceptional case; no automatic rollback is claimed. |
| All stages succeed | Persisted and active rounding/region match, draft baseline resets, existing success/Save behaviour remains. |

On every returned failure the candidate is not installed in active configuration,
and the form stays available for retry. Error text now names **rounding and holiday
region**, rather than implying that every effective setting is unchanged. Already
saved annual-leave coefficients are read by guidance immediately, including
historical guidance computed with current singleton values. Provider details affect
later maintenance/document generation. These paths do not rewrite submitted
payroll amounts, original PDFs, amendment history, manual adjustments or delivery
records, and do not send email. Existing native-vault durability and interrupted
SQLite/filesystem failure limitations are not enlarged into success guarantees.

The new isolated regression forces annual-leave refusal and configuration
publication failure, checks the actual provider/annual-leave rows, retained
payroll-department routing, old configuration bytes and effective rules, dirty
editor state, then verifies successful idempotent retry with one singleton row
in each table. Existing tests cover validation/provider/credential failures,
actual blue Save pointer clicks, all twelve rounding options and source allocation.
No native credential or live-provider acceptance campaign is repeated.

## Why no all-or-nothing correction is implemented

Moving configuration first merely exchanges this partial state for a configuration
commit followed by a failed database write, reintroducing the original defect.
A shared database transaction could group the two SQL statements, but cannot
atomically commit a file rename or a native credential-store write. Holding that
transaction across publication still leaves a database-commit failure after file
publication. A rollback rewrite can itself fail, overwrite a newer writer, or
restore inappropriate credential references; crashes leave the same coordination
problem. Therefore no small change guarantees atomicity for this combined form.

**Recommended next step for approval:** make provider, annual-leave guidance and
configuration explicit independent save scopes, each with its own dirty baseline,
result and retry control. Preserve the staged configuration path and all twelve
rounding choices. This makes the existing storage boundary visible without a new
recovery journal. It is a UI/workflow decision, not implemented here. If one
all-or-nothing save is mandatory, approve a separate coordinated persistence design
and failure/crash model before implementation; do not add best-effort compensation.
