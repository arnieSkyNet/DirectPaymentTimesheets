# DirectPaymentTimesheets database schema

## Scope and versioning

This is the implemented SQLite schema at version 38 (development application version `1.0.6`). It is derived from `create_schema` and migrations in `src/database.rs`; those migrations are authoritative.

`schema_version` contains the current integer version. A new database begins at version 1 and receives each ordered migration through `CURRENT_SCHEMA_VERSION` 38. Existing databases receive a verified pre-upgrade database/configuration backup and an isolated migration chain before transactional installation. Migration 23 removes the short-lived revision-only tables introduced by migration 22 while retaining the operational legacy snapshot tables. Migration 24 adds an explicit contracted/variable hours basis to effective-dated Personal Assistant contracted-hours history while preserving existing records as contracted.

During unreleased schema-20 development, an earlier local database shape contained `direct_shifts` without soft-deletion columns or the audit table. Startup therefore performs an idempotent schema-20 compatibility check after normal migrations. When that exact incomplete shape is found, it transactionally rebuilds `direct_shifts` into the final constrained form while preserving IDs and row values, then creates the audit table/indexes. It does not fabricate historical audit events, and repeated startup does not duplicate existing audit rows.

Most older relationships are logical references enforced by repository/application code. Schemas 30 and 31 declare PA foreign-key references for sickness periods and imported payroll documents, without cascading deletes. Schema 35 adds a self-reference from a superseded supplement to its replacement. SQLite enforcement depends on each connection enabling foreign keys; connections do not uniformly do so.

## `schema_version`

| Column | Type/constraint | Purpose |
|---|---|---|
| `version` | `INTEGER NOT NULL` | Current migration level. |

There is no primary key or uniqueness constraint. Initialisation inserts one row when the table is empty and application code expects one effective version row.

## Work evidence and import audit

### `direct_shifts`

Application-created actual-shift evidence, deliberately separate from imported `timesheets` rows.

| Column | Type/constraint | Purpose |
|---|---|---|
| `id` | `INTEGER PRIMARY KEY` | Stable direct-shift evidence identity. |
| `personal_assistant_id` | `INTEGER NOT NULL` | Logical reference to the maintained PA. |
| `start_time` | `TEXT NOT NULL` | Canonical local minute, `YYYY-MM-DDTHH:MM`. |
| `end_time` | nullable `TEXT` | Canonical completed minute; `NULL` means running. |
| `break_minutes` | `INTEGER NOT NULL DEFAULT 0`, non-negative check | Actual break evidence. |
| `notes` | nullable `TEXT` | Optional source note. |
| `source_type` | `TEXT NOT NULL`, checked to `direct` | Explicit source provenance. |
| `created_at`, `updated_at` | `TEXT NOT NULL` | Audit metadata for creation and latest deliberate write. |
| `deleted_at`, `deleted_by` | nullable `TEXT`, both null or both present | Soft-deletion time and actor. |

The schema checks that a stored end does not sort before its canonical start. Repository validation additionally rejects end-before-start and a break longer than the elapsed shift. A partial unique index permits at most one non-deleted running direct shift per PA. Normal current/recent queries exclude soft-deleted rows. `(personal_assistant_id, start_time DESC, id DESC)` supports deterministic recent-shift retrieval.

### `direct_shift_audit`

Append-only direct-shift mutation evidence. Normal repository/UI operations provide no update or delete path for these rows.

| Column | Type/constraint | Purpose |
|---|---|---|
| `id` | `INTEGER PRIMARY KEY` | Ordered audit identity. |
| `direct_shift_id` | `INTEGER NOT NULL` | Stable subject ID, retained even after a cancelled running row is removed. |
| `actor_id` | `TEXT NOT NULL` | Current desktop value `local_employer`; future-compatible authenticated identity. |
| `action_type` | checked `TEXT` | `clock_in`, `clock_out`, `edit`, `delete` or `cancel_clock_in`. |
| `action_at` | `TEXT NOT NULL` | Timestamp of the action. |
| `before_personal_assistant_id`, `after_personal_assistant_id` | nullable `INTEGER` | PA identity before/after. |
| `before_start_time`, `after_start_time` | nullable `TEXT` | Exact start snapshots. |
| `before_end_time`, `after_end_time` | nullable `TEXT` | Exact end snapshots. |
| `before_break_minutes`, `after_break_minutes` | nullable `INTEGER` | Break snapshots. |
| `before_notes`, `after_notes` | nullable `TEXT` | Note snapshots. |
| `before_updated_at`, `after_updated_at` | nullable `TEXT` | Current-row update metadata snapshots. |
| `before_deleted_at`, `after_deleted_at` | nullable `TEXT` | Soft-deletion time snapshots. |
| `before_deleted_by`, `after_deleted_by` | nullable `TEXT` | Soft-deletion actor snapshots. |

Shift mutation and its audit insert share one SQLite transaction. Creation has no before snapshot; cancellation has no after snapshot. No foreign key is declared because cancellation deliberately retains history after removing the accidental current row.

### `timesheets`

Imported external work rows.

| Column | Type/constraint | Purpose |
|---|---|---|
| `id` | `INTEGER PRIMARY KEY` | Stable imported-row identity. |
| `pa_name` | `TEXT NOT NULL` | Source PA name. |
| `start_time` | `TEXT NOT NULL` | Source start value, including the work date. |
| `end_time` | `TEXT NOT NULL` | Source end value. |
| `break_minutes` | `INTEGER NOT NULL` | Imported break duration. |
| `worked_minutes` | `INTEGER NOT NULL` | Imported worked duration. |
| `hourly_rate` | `REAL NOT NULL` | Imported, non-authoritative rate. |
| `amount` | `REAL NOT NULL` | Imported, non-authoritative amount. |
| `notes` | `TEXT` | Optional source note. |
| `personal_assistant_id` | `INTEGER` | Logical reference to `personal_assistants.id`; nullable for unresolved/legacy imports. |

No database uniqueness constraint implements duplicate detection. New CSV imports preflight possible collisions in application code: exact already-stored raw evidence is counted/skipped; conflicting same-start and new within-file duplicate candidates are retained for audited duplicate resolution, never silently selected or replaced. The complete file's new inserts and SUCCESS audit, including truthful imported/skipped counts, are committed in one SQLite transaction.

### `timesheet_correction_events`

Append-only corrections to the effective interpretation of an immutable imported `timesheets` row. The original row, imported PA identity, rate and amount are never changed.

| Column | Type/constraint | Purpose |
|---|---|---|
| `id` | `INTEGER PRIMARY KEY` | Ordered correction-event identity. |
| `timesheet_id` | `INTEGER NOT NULL` | Logical reference to immutable `timesheets.id`. |
| `actor_id` | non-empty `TEXT` | Actor responsible for the correction; current desktop identity is `local_employer`. |
| `action_type` | checked `TEXT` | `edit` or `revert`. |
| `action_at` | non-empty `TEXT` | Time of the correction action. |
| `reason` | nullable `TEXT` | Optional correction explanation. |
| `before_start_time`, `after_start_time` | `TEXT NOT NULL` | Complete effective start values before and after. |
| `before_end_time`, `after_end_time` | `TEXT NOT NULL` | Complete effective end values before and after. |
| `before_break_minutes`, `after_break_minutes` | non-negative `INTEGER` | Complete effective break values. |
| `before_worked_minutes`, `after_worked_minutes` | non-negative `INTEGER` | Complete independently authoritative worked values. |
| `before_notes`, `after_notes` | nullable `TEXT` | Complete effective notes. |

Correction timestamps use canonical local-minute text `YYYY-MM-DDTHH:MM`. `(timesheet_id, id)` supports ordered history and latest-event lookup. Repository validation requires end after start but deliberately does not derive worked minutes from clock times or break. Reversion appends another event whose after-values match the raw evidence; events are not updated or deleted.

Schema 28 retains raw imported rows for the import/view audit paths. Payroll evidence reads the latest correction-event values, while preserving raw source identity.

### `import_audit`

| Column | Type/constraint | Purpose |
|---|---|---|
| `id` | `INTEGER PRIMARY KEY` | Audit identity. |
| `import_time` | `TEXT NOT NULL` | Import timestamp. |
| `original_filename` | `TEXT NOT NULL` | Source path/name. |
| `archive_filename` | `TEXT NOT NULL` | Archived copy path, or empty on failure. |
| `rows_processed` | `INTEGER NOT NULL` | Rows read. |
| `rows_imported` | `INTEGER NOT NULL` | Rows inserted. |
| `rows_skipped` | `INTEGER NOT NULL` | Duplicate/skipped rows. |
| `status` | `TEXT NOT NULL` | Import outcome. |
| `error_message` | `TEXT` | Optional failure detail. |

## People and provider

### `employers`

| Column | Type/constraint |
|---|---|
| `id` | `INTEGER PRIMARY KEY` |
| `name` | `TEXT NOT NULL` |
| `address`, `telephone`, `email` | nullable `TEXT` |
| `postcode` | nullable legacy `TEXT`; not mapped by current Employer model/editor/repository |
| `payroll_provider`, `payroll_provider_address`, `payroll_provider_phone` | nullable legacy `TEXT` |
| `employer_signature`, `email_signature`, `default_pdf_template` | nullable path/text fields |
| `sick_pay_enabled`, `mileage_enabled` | `INTEGER NOT NULL DEFAULT 0`; retained legacy booleans, no runtime feature gates or UI controls |
| `date_of_birth`, `national_insurance_number`, `reference_account_number` | nullable `TEXT` |

The current Employer editor preserves one multiline address string, including any entered postcode, rather than mapping the legacy postcode column. The application normally uses one employer record. The schema does not enforce a singleton.

### `personal_assistants`

| Column | Type/constraint |
|---|---|
| `id` | `INTEGER PRIMARY KEY` |
| `first_name`, `surname` | `TEXT NOT NULL` |
| `date_of_birth`, `national_insurance_number` | nullable `TEXT` |
| `address`, `postcode`, `telephone`, `email` | nullable `TEXT` |
| `employment_status` | nullable `TEXT`; retained status, not the payroll-period eligibility gate |
| `sick_pay_enabled` | `INTEGER NOT NULL DEFAULT 0`; unused legacy PA flag, retained for compatibility |
| `mileage_enabled` | `INTEGER NOT NULL DEFAULT 0` boolean |
| `start_date`, `signature` | nullable `TEXT` |
| `leaving_date` | nullable `TEXT` added in schema27; current writes use `DD/MM/YYYY` |

### `payroll_provider`

| Column | Type/constraint |
|---|---|
| `id` | `INTEGER PRIMARY KEY` |
| `name`, `email`, `address`, `telephone` | nullable `TEXT` |
| `payroll_department_email` | nullable `TEXT` |

Repository save uses an explicit ID and upsert. The intended singleton is not otherwise constrained.

## Effective-dated PA history

### `personal_assistant_pay_rates`

| Column | Type/constraint |
|---|---|
| `id` | `INTEGER PRIMARY KEY` |
| `personal_assistant_id` | `INTEGER NOT NULL`; logical PA reference |
| `effective_date` | `TEXT NOT NULL`, normalised as `DD/MM/YYYY` by current repository writes |
| `base_hourly_rate` | `REAL NOT NULL` |
| `employer_top_up_rate` | `REAL NOT NULL` |
| `created_at` | `TEXT NOT NULL` |

There is no uniqueness constraint on PA/effective date. Repository lookup orders effective date descending and then ID descending, making equal-date selection deterministic.

### `personal_assistant_contracted_hours`

| Column | Type/constraint |
|---|---|
| `id` | `INTEGER PRIMARY KEY` |
| `personal_assistant_id` | `INTEGER NOT NULL`; logical PA reference |
| `effective_date` | `TEXT NOT NULL`, current format `DD/MM/YYYY` |
| `contracted_hours` | `TEXT NOT NULL`; weekly value for Contracted, empty for Variable |
| `hours_basis` | `TEXT NOT NULL DEFAULT 'contracted' CHECK (hours_basis IN ('contracted', 'variable'))`; schema24 |
| `created_at` | `TEXT NOT NULL` |

There is no uniqueness constraint on PA/effective date. The as-of lookup uses effective date descending and ID descending.

## Payroll schedules

### `payroll_schedules`

| Column | Type/constraint |
|---|---|
| `id` | `INTEGER PRIMARY KEY` |
| `payroll_year` | `TEXT NOT NULL` |
| `cycle_number` | `INTEGER NOT NULL` |
| `first_week_commencing` | `TEXT NOT NULL` |
| `latest_posting_date` | `TEXT NOT NULL` |
| `pay_date` | `TEXT NOT NULL` |
| `created_at` | `TEXT NOT NULL` |
| `payslips_sent` | `INTEGER NOT NULL DEFAULT 0` boolean |

Dates are stored as `DD/MM/YYYY`. Repository logic treats `(payroll_year, cycle_number)` as schedule identity, but the current schema does not declare that pair unique. Validated import supplies exactly 13 chronological four-week cycles and replaces one year transactionally.

## Payroll preparation

### `payroll_timesheets`

One PA's preparation record for one schedule.

| Column | Type/constraint |
|---|---|
| `id` | `INTEGER PRIMARY KEY` |
| `personal_assistant_id` | `INTEGER NOT NULL`; logical PA reference |
| `payroll_year` | `TEXT NOT NULL` |
| `cycle_number` | `INTEGER NOT NULL` |
| `previous_cycle_hours` | nullable `REAL` compatibility/display aggregate |
| `payroll_department_notes` | `TEXT NOT NULL DEFAULT ''`; maximum 256 Unicode characters, checked by the repository and UI, with a SQLite length constraint |
| `actual_in_lieu_hours` | nullable `REAL`; finite and non-negative; NULL means not recorded, zero means confirmed zero |
| `actual_in_lieu_updated_at` | nullable `TEXT`; RFC3339 time of independent result save/clear |
| `created_at`, `updated_at` | `TEXT NOT NULL` |

Unique constraint: `(personal_assistant_id, payroll_year, cycle_number)`.

### `payroll_timesheet_weeks`

| Column | Type/constraint |
|---|---|
| `id` | `INTEGER PRIMARY KEY` |
| `payroll_timesheet_id` | `INTEGER NOT NULL`; logical parent reference |
| `week_number` | `INTEGER NOT NULL` |
| `week_commencing` | `TEXT NOT NULL` |
| `worked_hours` | `REAL NOT NULL DEFAULT 0` |
| `annual_leave_hours` | `REAL NOT NULL DEFAULT 0` |
| `sick_leave_hours` | `REAL NOT NULL DEFAULT 0`; legacy aggregate, not current sickness entry/PDF projection |
| `public_holiday_hours` | `REAL NOT NULL DEFAULT 0` |
| `travel_miles` | `REAL NOT NULL DEFAULT 0` |

Unique constraint: `(payroll_timesheet_id, week_number)`.

### `payroll_timesheet_public_holidays`

| Column | Type/constraint |
|---|---|
| `id` | `INTEGER PRIMARY KEY` |
| `payroll_timesheet_id` | `INTEGER NOT NULL`; logical parent reference |
| `week_number` | `INTEGER NOT NULL` |
| `holiday_date` | `TEXT NOT NULL` |
| `hours` | `REAL NOT NULL DEFAULT 0` |

Unique constraint: `(payroll_timesheet_id, week_number, holiday_date)`.

### `payroll_timesheet_manual_adjustments`

| Column | Type/constraint |
|---|---|
| `id` | `INTEGER PRIMARY KEY` |
| `payroll_timesheet_id` | `INTEGER NOT NULL`; logical parent reference |
| `week_number` | `INTEGER NOT NULL` |
| `adjustment_minutes` | `INTEGER NOT NULL` signed correction |
| `reason` | nullable `TEXT` |
| `updated_at` | `TEXT NOT NULL` |

Unique constraint: `(payroll_timesheet_id, week_number)`.

## Email status

### `payroll_timesheet_email_status`

| Column | Type/constraint |
|---|---|
| `id` | `INTEGER PRIMARY KEY` |
| `personal_assistant_id` | `INTEGER NOT NULL`; logical PA reference |
| `payroll_year` | `TEXT NOT NULL` |
| `cycle_number` | `INTEGER NOT NULL` |
| `email_type` | `TEXT NOT NULL` |
| `sent_at` | nullable `TEXT` |

Unique constraint: `(personal_assistant_id, payroll_year, cycle_number, email_type)`. Current repository values distinguish `timesheet` and `payslip`. Migration 18 converted every schema-17 legacy status to type `timesheet`.

## Worked-item evidence and publication state

### `payroll_timesheet_worked_item_snapshots`

| Column | Type/constraint |
|---|---|
| `id` | `INTEGER PRIMARY KEY` |
| `payroll_timesheet_id` | `INTEGER NOT NULL`; logical parent reference |
| `week_number` | `INTEGER NOT NULL` |
| `source_type` | `TEXT NOT NULL` |
| `timesheet_id` | nullable `INTEGER`; logical source `timesheets.id` |
| `direct_shift_id` | nullable `INTEGER`; logical source `direct_shifts.id`, added in schema28 |
| `source_evidence` | nullable `TEXT`; TOML capture of raw source fields, distinct from payable minutes |
| `work_date` | nullable `TEXT` |
| `worked_minutes` | `INTEGER NOT NULL` |
| `pay_rate_id` | nullable `INTEGER`; logical rate reference |
| `pay_rate_effective_date` | nullable `TEXT` |
| `total_hourly_rate` | nullable `REAL` |
| `reason` | nullable `TEXT` |
| `captured_at` | `TEXT NOT NULL` |

Check constraint:

- Imported and direct source IDs are mutually exclusive.
- `legacy_previous_cycle_adjustment` requires both source IDs, work date and all rate fields to be `NULL`.
- `carry_correction` may retain unavailable date/rate evidence.
- Other source types require pay-rate ID, effective date and total rate to be non-null.

Indexes:

- partial unique index `payroll_snapshot_raw_shift` on `(payroll_timesheet_id, timesheet_id)` where `timesheet_id IS NOT NULL`;
- non-unique index `payroll_snapshot_timesheet_id` on `timesheet_id`;
- schema28 partial unique index `payroll_snapshot_direct_shift` on `(payroll_timesheet_id, direct_shift_id)` where `direct_shift_id IS NOT NULL`.

The partial unique index prevents one raw imported row appearing twice in the same payroll snapshot.

### `payroll_timesheet_snapshot_states`

| Column | Type/constraint |
|---|---|
| `payroll_timesheet_id` | `INTEGER PRIMARY KEY`; logical one-to-one parent reference |
| `state` | `TEXT NOT NULL`, checked to `candidate`, `submitted` or `indeterminate` |
| `pdf_path` | `TEXT NOT NULL` |
| `pdf_sha256` | `TEXT NOT NULL` |
| `generated_at` | `TEXT NOT NULL` |
| `submitted_at` | nullable `TEXT` |
| `indeterminate_at` | nullable `TEXT` |

The primary key enforces at most one publication state per payroll timesheet.

## Constraint summary

Core preparation uniqueness beyond primary keys (later tables have additional constraints listed in their sections and `src/payroll_evidence/schema.sql`):

- one payroll-timesheet row per PA/year/cycle;
- one weekly row per payroll-timesheet/week;
- one public-holiday detail per payroll-timesheet/week/date;
- one manual adjustment per payroll-timesheet/week;
- one email status per PA/year/cycle/type; and
- one occurrence of a non-null source TimesheetEntry ID per legacy payroll-timesheet snapshot.

The retained direct-shift running/recent indexes, direct-shift value checks and correction-history index are described above. Later constraints include unique active duplicate-group fingerprints, reconciliation evidence keys, correction evidence keys and compound application/member identities (schema28), sickness date indexing (schema30), and unique document paths plus PA/document indexing (schema31). Schemas30/31 declare PA references without cascading deletes; connection-level enforcement is not uniform. Repository and service validation supplies other business rules.

### `payroll_timesheet_annual_leave` (schema 25)

Dated annual leave is a child of the preparation record. Columns: `id INTEGER PRIMARY KEY`, `payroll_timesheet_id INTEGER NOT NULL`, `week_number INTEGER NOT NULL` (1–4), `leave_date TEXT NOT NULL` (`DD/MM/YYYY`), `hours REAL NOT NULL` (finite, non-negative), `created_at TEXT NOT NULL`, and `updated_at TEXT NOT NULL`. The combination `(payroll_timesheet_id, week_number, leave_date)` is unique. Repository validation checks the date against the actual stored seven-day payroll week and normalises supported input dates before saving.

Migration 24 → 25 creates this table transactionally without backfilling dates or changing existing `payroll_timesheet_weeks.annual_leave_hours`. Non-zero weekly totals without child rows remain legacy undated leave. Dated weeks use the sum of child hours; removing all their rows sets the weekly aggregate to zero. Detail rows and weekly totals save in the existing preparation transaction, including candidate invalidation and submitted/indeterminate protection. PDFs continue to use only the weekly aggregate. Read-only annual-leave guidance uses actual dated leave dates, or week commencing for a legacy undated weekly total; it never counts both for the same week.

### `annual_leave_settings` (schema 26)

Migration 25 → 26 transactionally creates an empty singleton settings table, preserving all schema-25 data. `id INTEGER PRIMARY KEY CHECK (id = 1)` identifies the sole settings row. Its four values are `contracted_effective_from TEXT NOT NULL`, `statutory_weeks REAL NOT NULL` (finite, 0–52), `variable_effective_from TEXT NOT NULL`, and `accrual_percentage REAL NOT NULL` (finite, 0–100).

Both effective-from values are recurring `DD/MM` boundaries, not year-specific dates. Repository validation normalises day/month input and requires a date that exists every year (29 February is rejected). Different valid boundaries are permitted for the two groups. These dates are independent recurring rule effective dates, not annual-leave-year boundaries and not PA basis-change dates. Annual-leave guidance uses the single fixed April-to-March year: 2026/27 runs 01/04/2026 through 31/03/2027 inclusive. Differing rule dates are supported. The singleton has no historical numeric rule values; guidance, including historical years, uses the current saved/default values with this limitation stated in Details.

An absent row loads defaults of `01/04`, 5.6 weeks, `01/04`, and 12.07 percent without writing to the database. Save Payroll Settings validates these inputs before any payroll writes, then saves the four values in one atomic SQLite statement after the existing payroll-settings save. The config/provider and annual-leave writes are not a shared transaction; failures are reported explicitly, including when the other payroll settings have already saved. There is no separate leave-year-start config field, rule creation, history, editing/deletion workflow, or confirmation machinery. No compatibility repair for earlier development-only schema-26 layouts is included; local development databases may be reset manually.

Personal Assistants have an optional Leaving date, stored as canonical `DD/MM/YYYY`. Schema 27 adds nullable `personal_assistants.leaving_date` without backfilling dates or altering historical data. A supplied date must be real and not precede Start date. Stored Active/Inactive status is never changed automatically. Ordinary selected-period payroll inclusion uses inclusive Start/Leaving date overlap with the four-week period, independent of current Active/Inactive status. Missing employment boundaries remain unbounded. Existing selected-period preparation records remain included regardless of status or employment dates. Annual-leave guidance applies Start and Leaving dates inclusively without changing employment status.


## Schema 28: payroll evidence decisions and submission history

Migration 28 is transactional and preserves original imported/direct evidence and snapshot IDs. It extends worked snapshots with nullable `direct_shift_id` and `source_evidence` (a TOML capture of actual source fields), keeps imported/direct IDs mutually exclusive, and adds a unique per-preparation direct-shift index. Explicit `carry_correction` items may retain unavailable rate/date fields; ordinary source allocations still require maintained historical rate evidence. No revision-only tables are restored.

New tables are grouped by purpose:

- `payroll_duplicate_decisions`, `payroll_duplicate_members`: versioned group fingerprints, one winner, retained candidate data and invalidation audit.
- `payroll_submissions`, `payroll_submission_items`, `payroll_submission_weeks`, `payroll_submission_leave`, `payroll_submission_holidays`: immutable submitted contents, attachment identity/available bytes and supersession links. Existing submitted snapshots are backfilled only from retained facts; missing legacy timestamps and exact clock captures remain null.
- `payroll_reconciliation_decisions`: explicit resubmit, carry, actual-evidence-complete and historical paid/unpaid decisions, with actor/time and evidence.
- `payroll_corrections`, `payroll_correction_evidence`: individual signed corrections and actual evidence covered by aggregate reconciliation.
- `payroll_correction_applications`, `payroll_submission_corrections`: open reservations and immutable submitted applications; remaining amounts are derived, not overwritten as an anonymous net balance.
- `payroll_candidate_checks`: material evidence signatures used to reject stale generation before production send.

The SQL definition is `src/payroll_evidence/schema.sql`; migration mechanics are in `src/payroll_evidence.rs`. See [the evidence guide](PAYROLL-EVIDENCE.md) for lifecycle and calculation rules.

## Schema 29: verified legacy settlement baseline

Forward migration 29 adds `payroll_legacy_evidence` (source, source_id, material
fingerprint), `payroll_legacy_settlements` (payroll_timesheet_id, retained definitive
payslip sent_at), and singleton `payroll_legacy_cutover` (recorded_at, inventory TOML
including provenance). These are cutover facts, not per-shift payment assertions.
No original payroll, duplicate-decision, submission or evidence rows are rewritten.
The inventory is captured for upgrades from before 28 or validated from an explicit
operator-provided recovery inventory for existing 28 databases. Without such an
inventory an existing 28 database gets no exemption. See
[Payroll evidence](PAYROLL-EVIDENCE.md#legacy-settlement-cutover-schema-29) for recovery
file preparation, validation, and the distinction between cutover and recording time.


## Schema 30: structured sickness periods

Migration 30 transactionally creates `personal_assistant_sickness_periods` and its index, preserving legacy PA flags and weekly `sick_leave_hours` without backfilling or converting them.

| Column | Type/constraint |
|---|---|
| `id` | `INTEGER PRIMARY KEY` |
| `personal_assistant_id` | `INTEGER NOT NULL REFERENCES personal_assistants(id)`; no cascading delete |
| `start_date`, `end_date` | `TEXT NOT NULL`; length 10 and ISO digit-pattern checks, canonical `YYYY-MM-DD` |

The table checks `end_date >= start_date`; repository validation additionally requires real calendar dates and an existing PA. Dates are inclusive. `idx_sickness_periods_pa_dates(personal_assistant_id, start_date, end_date)` supports PA/date queries. Periods are independent of payroll weeks: overlapping weeks project the same full date record. Protected payroll editing and candidate invalidation are application rules, not triggers. The application records dates; Payroll calculates SSP. Legacy weekly sickness hours are not synchronised by period entry and remain a limitation of the annual-leave warning described in [Domain](DOMAIN.md#annual-leave-guidance).

## Schema 31: cycle-independent PA payroll documents

Migration 31 creates the following table and index in one transaction and advances `schema_version` to 31. It does not read, rewrite or migrate `payroll_timesheet_email_status`: existing payslip/timesheet rows and settlement semantics remain unchanged. There are no historical P60/P45 delivery associations to invent or backfill. The current development application is 1.0.6; schema 31 is the introduction point for this table, subsequently extended by schemas 33 and 35 below.

### `imported_payroll_documents`

| Column | Type/constraint |
|---|---|
| `id` | `INTEGER PRIMARY KEY`; stable document identity |
| `personal_assistant_id` | `INTEGER NOT NULL REFERENCES personal_assistants(id)`; no cascading delete |
| `document_type` | `TEXT NOT NULL CHECK (document_type IN ('p60', 'p45'))` |
| `stored_path` | `TEXT NOT NULL UNIQUE`; canonical absolute file path |
| `sha256` | `TEXT NOT NULL CHECK (length(sha256) = 64)`; imported content digest |
| `document_year` | nullable `TEXT`; explicit filename tax year in `YYYY/YY` form, otherwise NULL |
| `sent_at` | nullable `TEXT`; NULL = no application delivery evidence (consult `history_state`), `indeterminate:<attempt timestamp>` = protected/uncertain, successful-send timestamp = sent |

Indexes: the primary-key row identity, SQLite's unique index on `stored_path`, and `idx_imported_payroll_documents_pa(personal_assistant_id, id)`. There is no schedule foreign key, cycle column or sentinel cycle. Reimporting the same path/content/PA/type/year retains the same ID and delivery state. A conflicting identity/content is refused; alternate-path copies with identical PA/type/content are conservatively refused rather than creating a new delivery identity, regardless of filename year. Distinct content can be registered separately and starts unknown. File digests are checked again when selecting email attachments.

P60/P45 and an optional ordinary payslip transition atomically across their separate tables on one connection. Documents are definitively marked sent only after successful SMTP. Failed SMTP restores unsent state; failures persisting the final result leave durable indeterminate protection. P60/P45 states never count towards `payroll_schedules.payslips_sent` or evidence settlement. File publication precedes document registration; a registration failure is reported with stored-file paths, and reimport retries registration without overwriting the files.


Ordinary payslips with no applicable stored cycle may be archived in the PA's year-specific or general Payslip area. They are filesystem evidence only: they are not inserted into `imported_payroll_documents` (which remains P60/P45-only), `payroll_timesheet_email_status`, or payroll settlement tables. Their archival counts and paths are reported by the importer. This fallback requires no additional migration.


## Schema 32: outgoing payroll notes and returned in-lieu hours

Migration 32 atomically adds the three columns above to `payroll_timesheets` and
nullable `payroll_department_notes TEXT` to `payroll_submissions`, then advances
`schema_version` to 32. Existing preparation notes default to empty; existing
returned values/timestamps and historical submission notes remain NULL. No payroll
values, delivery statuses, candidates, or historical PDF bytes are backfilled or
rewritten. Fresh databases and older upgrades follow the same migration chain.

Notes are scoped by the existing unique PA/year/cycle identity. The repository
counts Unicode scalar values (not UTF-8 bytes), rejects more than 256 characters,
and retains exact text, including explicit line breaks and whitespace. A changed
note is written in the preparation transaction that invalidates an existing PDF
candidate; unchanged preparation saves preserve it. Submission archiving copies
the saved note into that submission's snapshot field. Superseding a submission
never replaces the old note or PDF. Source shift notes retain their existing,
non-invalidating behaviour.

`save_actual_in_lieu_hours(record_id, Option<f64>)` updates only the returned value
and its dedicated timestamp. `None` clears the result; zero is a known zero award.
The method validates finite, non-negative numbers and permits submitted, settled
and indeterminate records. It does not change preparation `updated_at`, totals,
corrections, snapshots, PDFs, submission/delivery state, or leave calculations.
Preparation saves do not write either returned-result column. Future importers
may call this same method after establishing the PA/period identity; extraction
is not implemented. Decimal formatting is presentation-only, with no shift
rounding or conversion to worked minutes.


## Schema 33: explicit supplement delivery history

Migration 33 atomically adds `imported_payroll_documents.history_state TEXT NOT NULL DEFAULT 'unknown'`, constrained to `unknown`, `needs_sending`, `external` or `application`. It preserves every existing document column and `sent_at` value. Rows with non-NULL `sent_at` receive `application` (existing Sent or Indeterminate evidence); NULL rows receive `unknown`. No historical send dates are inferred.

New registrations also start `unknown`, even when first imported today. The reconciliation UI records either `external` (Already delivered externally) or `needs_sending` (Needs sending), only from unknown and only while `sent_at IS NULL`. Neither decision changes `sent_at`. Decisions persist across restarts; Cancel does not undo saved history decisions. Sent and Indeterminate evidence always takes precedence. Only needs_sending documents with NULL `sent_at` can enter a new protected production send; protection and finalisation retain the history declaration and update only the actual application delivery marker/timestamp. Ordinary payslip status and timesheet delivery are unchanged.

Unknown/external documents are omitted from attachment selection; other eligible documents may still be sent. An indeterminate supplement continues to block automatic retry. Re-registering the same identity preserves all states. Alternate-path identical PA/type/content registrations are refused conservatively, preserving the original registry entry rather than guessing which copy should inherit its history.


## Schema 34: payroll filing cleanup journal

Migration 34 atomically creates `payroll_file_moves` and advances the schema version. It neither moves files nor changes schema33 supplement history or delivery evidence.

| Column | Meaning |
|---|---|
| `source_path TEXT PRIMARY KEY` | Original absolute application-owned path, retained until commit |
| `destination_path TEXT NOT NULL UNIQUE` | Published verified destination |
| `personal_assistant_id INTEGER NOT NULL` | PA whose explicit filing/repair operation owns cleanup |
| `sha256 TEXT NOT NULL CHECK(length(sha256)=64)` | Expected content at both paths |

Rows commit in the same transaction as registered-path changes and an optional explicit employment edit. After commit, cleanup verifies destination and source digests, removes the source and deletes the journal row. Missing/changed destinations or changed sources retain the row and surviving files for explicit retry. Repeated cleanup is idempotent. PA deletion is refused while registered payroll documents or pending cleanup rows reference the PA. The journal has no cascading deletion.


## Schema 35: explicit corrected document revisions

Migration 35 atomically adds nullable `imported_payroll_documents.superseded_by INTEGER REFERENCES imported_payroll_documents(id)` and creates `payslip_revisions`. Existing rows, paths, hashes, delivery history and timestamps are untouched; no files are inspected or moved.

`payslip_revisions` columns: `id INTEGER PRIMARY KEY`, `personal_assistant_id INTEGER NOT NULL`, `payroll_year TEXT NOT NULL`, `cycle_number INTEGER NOT NULL`, `stored_path TEXT NOT NULL UNIQUE`, `sha256 TEXT NOT NULL CHECK(length(sha256)=64)`, nullable `sent_at TEXT`, `is_current INTEGER NOT NULL CHECK(is_current IN (0,1))`, and `created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP`. A partial unique index permits exactly at most one current revision per PA/year/cycle. Legacy ordinary files are adopted on first deliberate replacement. Old delivery markers remain on the old revision; current cycle delivery and current revision markers transition together.

A supplement replacement inserts an unknown/NULL-sent registration and links the superseded row to it. Current-document readers filter `superseded_by IS NULL`; filing reads all revisions. Duplicate semantic import identities are refused conservatively at registration; the schema intentionally preserves legacy multiple registrations rather than merging their evidence.

The existing move journal now supports configured payslip and PDF roots. Filing updates `stored_path` in both document registries and `pdf_path` in snapshot/submission records, preserving all hash/evidence fields. Replacements retain originals permanently; they do not enqueue deletion of superseded evidence.

## Schema 36: immutable timesheets and transport-attempt evidence

Migration36 is transactional and follows schema35 (used by published Linux/Windows
1.0.4 and macOS 1.0.5). It adds `document_id INTEGER REFERENCES
timesheet_documents(id)` to `payroll_timesheet_snapshot_states` and
`payroll_submissions`. A partial unique index on submission `document_id` permits
one payroll submission per generated document. Original rows, represented
payloads, corrections, timestamps and settlement statuses are preserved.

| Table | Columns |
|---|---|
| `timesheet_documents` | `id INTEGER PRIMARY KEY`, `payroll_timesheet_id INTEGER NOT NULL`, `pdf_path TEXT NOT NULL` (original generation location), `pdf_sha256 TEXT NOT NULL`, `pdf_bytes BLOB`, `generated_at TEXT NOT NULL`, `legacy INTEGER NOT NULL DEFAULT 0`, `legacy_submission_id INTEGER UNIQUE` |
| `timesheet_delivery_attempts` | `id INTEGER PRIMARY KEY`, `intent_id TEXT NOT NULL UNIQUE`, `payroll_timesheet_id INTEGER NOT NULL`, `document_id INTEGER NOT NULL REFERENCES timesheet_documents(id)`, `submission_id INTEGER REFERENCES payroll_submissions(id)`, `classification TEXT NOT NULL` (`first_send`/`resend`), `recipients TEXT NOT NULL`, `pdf_path TEXT NOT NULL` (approved dispatch location), `message_id TEXT NOT NULL UNIQUE`, `started_at TEXT NOT NULL`, `completed_at TEXT`, `outcome TEXT NOT NULL` (`uncertain`/`accepted`/`confirmed_not_sent`), `detail TEXT NOT NULL`, `resolved_at TEXT`, `payroll_department_notes TEXT` |
| `timesheet_delivery_reviews` | `id INTEGER PRIMARY KEY`, `attempt_id INTEGER REFERENCES timesheet_delivery_attempts(id)` (NULL for legacy review), `payroll_timesheet_id INTEGER NOT NULL`, `decision TEXT NOT NULL` (`accepted`/`confirmed_not_sent`), nonblank `reason TEXT NOT NULL`, `actor TEXT NOT NULL`, `reviewed_at TEXT NOT NULL`, `original_evidence TEXT NOT NULL` |

The five `timesheet_attempt_items/weeks/leave/holidays/corrections` tables prepend
`attempt_id` to the corresponding current snapshot/preparation/correction-application
columns. Claiming a first send copies those rows before SMTP. Finalisation copies
them into existing submission tables, excluding the correction application's
current-timesheet association. These payload tables are retained and immutable.

The unresolved-attempt partial unique index covers `payroll_timesheet_id` where
`outcome='uncertain' AND resolved_at IS NULL`. Attempts begin uncertain before SMTP;
only established acceptance/non-acceptance finalises transport evidence. Reviews
retain the original uncertain outcome and append their decision, rather than
rewriting it. Identity/recipient/message fields and completed evidence are guarded
against replacement; attempts, documents and reviews cannot be deleted.

Document bytes may transition from NULL to verified bytes once; retained bytes,
digest and generation identity cannot be rewritten. Legacy submissions are each
assigned a document identity without inventing delivery attempts. Missing legacy
bytes are not guessed. Current registered snapshot paths remain authoritative
when filing moves a document; original document location remains provenance.

Every table has ordinary INSERT/UPDATE/DELETE compatibility triggers requiring
`dpt_schema_version()=36`. Compatible connections register it through
`database::open`. These guards are limited: published 1.0.4/1.0.5 executables have
no future-schema rejection or new maintenance lock, and their SQLite backup-based
restore can overwrite a schema36 database. Never reopen upgraded production data
with those executables. No retroactive protection is claimed.

### Verified startup upgrades and legacy restore

Normal startup (including `cargo run`) uses the configured application root; it
is not a development database unless `DIRECTPAYMENTTIMESHEETS_HOME` is explicitly
set. Before loading/saving configuration or creating external business folders,
`database_recovery::initialise` obtains the dispatch OS lock and a SQLite writer
reservation. Existing schema1–35 databases receive a separate verified database
and optional configuration snapshot before any upgrade. SQLite's backup API
includes committed WAL data without copying a live main file alone. Integrity,
complete schema/typed-row fingerprints and configuration bytes are verified and
recovery files flushed. Backup creation/verification failure aborts startup
without changing the original database. New empty databases need no original
recovery snapshot; future schemas are refused.

All ordered migrations, including individually committed older steps, run on an
isolated working copy. Only a fully migrated, verified current-schema copy is installed
in one live SQLite transaction, under the same writer reservation. Failure or
interruption before commit retains the original; restart does not adopt abandoned
work copies. SQL transfer preserves tables, rows, indexes, triggers, views,
AUTOINCREMENT high-water marks, application_id and user_version. Maintenance
connections temporarily disable foreign-key enforcement during transfer so
existing legacy logical references remain intact; normal connections are unchanged.

Recognised legacy backups are integrity/schema-validated, migrated in isolation,
and reviewed before restore. Approval binds the live database/configuration,
selected backup and isolated result. Changed inputs invalidate approval. The UI
lists current rows absent/different in restored data, requires a documented reason
and explicit destructive-rollback acknowledgement, and explains duplicate-email
risks. Unresolved ledger or legacy indeterminate email outcomes block rollback
regardless of acknowledgement. A separate verified live recovery snapshot and
rollback decision manifest are flushed before installation. Newer submissions,
accepted attempts and settlement evidence may leave the active database only with
this informed approval; their exact original data remains in the recovery copy.
Restoring older evidence requires reconciliation with Payroll before further sends.

Restore, upgrade, timesheet publication, production timesheet and payslip/P45/P60
sending, and uncertainty review use the same OS sidecar lock. SQLite writer locking prevents concurrent
edits during recovery capture/installation; the final install is transactional.
Database/configuration replacement is not one atomic filesystem operation: if
configuration installation fails after database commit, restart is required and
both originals remain in the verified recovery snapshot. Close all other instances
before a rollback and restart after it; other instances' cached UI is not refreshed.

Recovery snapshots do NOT copy or restore externally stored PDFs, payslips,
signatures, CSV/archive files or fonts. Their manifest inventories configured
business folders and registered document/signature paths without reading those
business files. Complete application-data recovery needs a separate protected
copy of the entire application root plus configured external folders/files. Raw
configuration can contain credentials; backup directories are private on Unix.


## Schema 37: sickness protection and historical information corrections

Stage 2 introduced schema37; current development 1.0.6 uses schema38. The existing verified pre-upgrade backup and isolated migration-chain installation apply to 36→37 and earlier supported databases. Migration37 transactionally rebuilds `personal_assistant_sickness_periods` with `INTEGER PRIMARY KEY AUTOINCREMENT`, preserving existing IDs, dates, PA references and the date index. It does not deduplicate existing records, backfill missing historic sickness dates, change legacy weekly hours or modify payroll settlement/submission records.

New tables:

| Table | Stored evidence |
| --- | --- |
| `sickness_changes` | ID, PA, period ID, structured before/after evidence, reason, scope, actor and timestamp; append-only. |
| `sickness_cycle_overrides` | Payroll record ID and structured scoped date projection, retaining protected dates when editable portions change. |
| `sickness_corrections` | Change, payroll record, original submission and reconciliation decision IDs; before/after evidence, unchanged payroll totals, reason, authorisation timestamp/actor, generated document ID and retirement timestamp. Authorisation fields are immutable; document association and retirement are limited lifecycle metadata. |
| `sickness_document_evidence` | Immutable document ID, payroll record, structured date evidence and optional correction association. |
| `sickness_attempt_evidence` | Immutable attempt ID, exact document/date evidence and correction association, including resends. |
| `sickness_submission_evidence` | Immutable submission ID, document/date evidence and correction association. |

Structured evidence contains the ordered inclusive PA-owned date periods and stable IDs. Candidate freshness incorporates a deterministic sickness signature. Correction freshness additionally binds the correction authority, original submission and preserved payroll totals. New document identities receive exact sickness snapshots atomically with candidate registration. Dispatch copies document evidence into the durable attempt before SMTP; acceptance copies the attempt evidence into the new submission. Resends append attempts against the existing submission.

No historical snapshots are fabricated for legacy PDFs. Missing capture blocks first delivery until regeneration, while retained submitted bytes can still be deliberately resent. For legacy authorised corrections without a captured original date snapshot, the operator must review the original PDF and verify the previously recorded dates; the application cannot reconstruct missing historic dates automatically.

Schema37 replaces schema36 compatibility triggers atomically with `dpt37_*` guards requiring `dpt_schema_version() = 37`. Published 1.0.4/1.0.5 and earlier schema36 development executables do not gain future-schema checks: guarded row writes fail, but these triggers do not prevent destructive DDL, file replacement or old restore operations. Never open the upgraded production database using an older executable. Current startup rejects future schemas. Restore reviews include all new sickness tables, require documented rollback approval, preserve a verified recovery copy and refuse unresolved delivery uncertainty; external business files are not silently restored.


Schema37 retained-period transfers allocate a new PA-wide sickness identity when the former row has been deleted; immutable original evidence keeps its old identity. Per-cycle projections are updated transactionally only after reviewing all overlapping source/destination cycles. Conflicting active/retained versions refuse transfer. No further migration is needed.

New sickness correction `original_totals` values also bind a fingerprint of the authorised original submission's financial payload and PDF digest. Business-value validation excludes generated row IDs and capture timestamps, checks exact worked membership/correction applications and weekly/dated evidence, and validates carry-forward from retained items. Older authorisation strings remain readable and still undergo original-submission comparison; missing legacy financial evidence is never invented.

### Stage 3 signature references (schema37 unchanged)

`employers.employer_signature` and `personal_assistants.signature` continue to
store nullable image paths, with no new table or migration. External PNG/JPEG
paths remain compatible. Drawn signatures point to immutable full-name/numbered PNGs
under `<configured data root>/signatures/`. Saving a drawing updates only its
owner's signature column transactionally under the delivery/recovery lock;
existing files, submissions and PDF evidence are retained. Signature image assets
are excluded from database/config backup even when inside the data root: protect
the full root separately. Unsigned generation never clears a stored reference.


### Stage 4 shift reviews and CSV content identity (1.0.6/schema38)

Migration38 is additive. `csv_import_contents` identifies successfully retained CSV bytes by SHA-256 plus pathname and associates a real import audit. `csv_import_rows` retains raw shift membership for those bytes. `shift_change_links` connects possible replacement/counterpart identities without deleting or changing either source. `shift_change_events` retains prospective before/after evidence, reason, review signature, actor and time. `shift_review_deferrals` is an append-only deferred-review history. Existing duplicate decisions/members retain approved payable choices; `winner_source='separate'` represents explicit authorisation to retain all displayed candidates as separate shifts.

No legacy source, audit, duplicate/reconciliation decision, correction component/application, submission, settlement or PDF is reclassified by migration38. New tables start empty. A user-requested import may register a legacy CSV archive's verified content and raw membership against its existing audit; it does not fabricate a new legacy audit or mark unchanged rows as new changes. An unavailable legacy archive refuses comparison with an actionable error. No archive/business files are read by migration38 itself.

The established verified WAL-safe backup and isolated installation cover 35→36→37→38 and 37→38. Migration38 and its schema guard replacement are transactional. Schema38 guards reject ordinary business/version writes from schema37 connections. This does not make previously published executables safe recovery tools: keep old binaries away from upgraded data, because filesystem-level restore and schema-altering operations are outside these guards.


### Stage 5 startup validation (schema38 unchanged)

Ordinary current-schema startup validates critical table/column and schema38 write-guard metadata, then runs a bounded `quick_check(1)`. Its progress callback stops at approximately 100,000 VM operations or 250 ms; an individual filesystem I/O can exceed the callback budget. Budget exhaustion is not treated as corruption and unchecked pages are not certified. Detected corruption/missing protections is fatal without automatic repair. Verified backup, staged upgrade and transactional installation still perform their full checks.

Fresh empty databases are now staged and installed transactionally rather than applying the migration chain directly to live data. Unknown/nonempty databases are never treated as fresh. Existing upgrade backup/WAL/rollback/recovery semantics are unchanged. Configuration or graphics failure after a successful upgrade leaves that upgrade applied and reports the location of its pre-upgrade recovery folder. Business-folder availability does not bypass database or dispatch locks. No schema migration accompanies Stage5.
