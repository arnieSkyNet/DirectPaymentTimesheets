# DirectPaymentTimesheets database schema

## Scope and versioning

This is the implemented SQLite schema at version 27. It is derived from `create_schema` and migrations in `src/database.rs`; those migrations are authoritative.

`schema_version` contains the current integer version. A new database begins at version 1 and receives each ordered migration through `CURRENT_SCHEMA_VERSION` 27. Existing databases are upgraded in place. Migration 23 removes the short-lived revision-only tables introduced by migration 22 while retaining the operational legacy snapshot tables. Migration 24 adds an explicit contracted/variable hours basis to effective-dated Personal Assistant contracted-hours history while preserving existing records as contracted.

During unreleased schema-20 development, an earlier local database shape contained `direct_shifts` without soft-deletion columns or the audit table. Startup therefore performs an idempotent schema-20 compatibility check after normal migrations. When that exact incomplete shape is found, it transactionally rebuilds `direct_shifts` into the final constrained form while preserving IDs and row values, then creates the audit table/indexes. It does not fabricate historical audit events, and repeated startup does not duplicate existing audit rows.

SQLite foreign-key constraints are not declared in schema 27. Relationships described below are logical relationships enforced by repository/application code and stored IDs/business keys.

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

No database uniqueness constraint implements duplicate detection. New CSV imports preflight possible collisions in application code: materially identical evidence is counted/skipped, while a same-PA/start material difference refuses the complete file rather than selecting or replacing a row. The complete file's new inserts and SUCCESS audit, including truthful imported/skipped counts, are committed in one SQLite transaction.

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

Schema 23 retains the schema-21 raw and effective repository queries, but current application, import collision and payroll paths continue to use raw rows. Corrected effective values are not yet consumed by Payroll Timesheet Preparation, snapshots or PDFs.

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
| `address`, `postcode`, `telephone`, `email` | nullable `TEXT` |
| `payroll_provider`, `payroll_provider_address`, `payroll_provider_phone` | nullable legacy `TEXT` |
| `employer_signature`, `email_signature`, `default_pdf_template` | nullable path/text fields |
| `sick_pay_enabled`, `mileage_enabled` | `INTEGER NOT NULL DEFAULT 0` booleans |
| `date_of_birth`, `national_insurance_number`, `reference_account_number` | nullable `TEXT` |

The application normally uses one employer record. The schema does not enforce a singleton.

### `personal_assistants`

| Column | Type/constraint |
|---|---|
| `id` | `INTEGER PRIMARY KEY` |
| `first_name`, `surname` | `TEXT NOT NULL` |
| `date_of_birth`, `national_insurance_number` | nullable `TEXT` |
| `address`, `postcode`, `telephone`, `email` | nullable `TEXT` |
| `employment_status` | nullable `TEXT`; `NULL` is treated as active by current workflows |
| `sick_pay_enabled`, `mileage_enabled` | `INTEGER NOT NULL DEFAULT 0` booleans |
| `start_date`, `signature` | nullable `TEXT` |

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
| `contracted_hours` | `TEXT NOT NULL` |
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

Dates are stored as `DD/MM/YYYY`. Repository logic treats `(payroll_year, cycle_number)` as schedule identity, but schema 23 does not declare that pair unique. Validated import supplies exactly 13 chronological four-week cycles and replaces one year transactionally.

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
| `sick_leave_hours` | `REAL NOT NULL DEFAULT 0` |
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
| `work_date` | nullable `TEXT` |
| `worked_minutes` | `INTEGER NOT NULL` |
| `pay_rate_id` | nullable `INTEGER`; logical rate reference |
| `pay_rate_effective_date` | nullable `TEXT` |
| `total_hourly_rate` | nullable `REAL` |
| `reason` | nullable `TEXT` |
| `captured_at` | `TEXT NOT NULL` |

Check constraint:

- `legacy_previous_cycle_adjustment` requires `timesheet_id`, work date and all rate fields to be `NULL`.
- Every other source type requires pay-rate ID, effective date and total rate to be non-null.

Indexes:

- partial unique index `payroll_snapshot_raw_shift` on `(payroll_timesheet_id, timesheet_id)` where `timesheet_id IS NOT NULL`;
- non-unique index `payroll_snapshot_timesheet_id` on `timesheet_id`.

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

Declared uniqueness beyond primary keys:

- one payroll-timesheet row per PA/year/cycle;
- one weekly row per payroll-timesheet/week;
- one public-holiday detail per payroll-timesheet/week/date;
- one manual adjustment per payroll-timesheet/week;
- one email status per PA/year/cycle/type; and
- one occurrence of a non-null source TimesheetEntry ID per legacy payroll-timesheet snapshot.

Schema 23 also declares the direct-shift running/recent indexes, direct-shift value checks and correction-history index described above. No foreign keys or cascading deletes are declared. Repository and service validation supplies the remaining business rules.

### `payroll_timesheet_annual_leave` (schema 25)

Dated annual leave is a child of the preparation record. Columns: `id INTEGER PRIMARY KEY`, `payroll_timesheet_id INTEGER NOT NULL`, `week_number INTEGER NOT NULL` (1–4), `leave_date TEXT NOT NULL` (`DD/MM/YYYY`), `hours REAL NOT NULL` (finite, non-negative), `created_at TEXT NOT NULL`, and `updated_at TEXT NOT NULL`. The combination `(payroll_timesheet_id, week_number, leave_date)` is unique. Repository validation checks the date against the actual stored seven-day payroll week and normalises supported input dates before saving.

Migration 24 → 25 creates this table transactionally without backfilling dates or changing existing `payroll_timesheet_weeks.annual_leave_hours`. Non-zero weekly totals without child rows remain legacy undated leave. Dated weeks use the sum of child hours; removing all their rows sets the weekly aggregate to zero. Detail rows and weekly totals save in the existing preparation transaction, including candidate invalidation and submitted/indeterminate protection. PDFs continue to use only the weekly aggregate. Entitlement, accrual and statistics are not implemented.

### `annual_leave_settings` (schema 26)

The corrected uncommitted migration 25 → 26 transactionally creates an empty singleton settings table, preserving all schema-25 data. `id INTEGER PRIMARY KEY CHECK (id = 1)` identifies the sole settings row. Its four values are `contracted_effective_from TEXT NOT NULL`, `statutory_weeks REAL NOT NULL` (finite, 0–52), `variable_effective_from TEXT NOT NULL`, and `accrual_percentage REAL NOT NULL` (finite, 0–100).

Both effective-from values are recurring `DD/MM` boundaries, not year-specific dates. Repository validation normalises day/month input and requires a date that exists every year (29 February is rejected). Different valid boundaries are permitted for the two groups. A future annual period starts at the applicable boundary and ends inclusively the day before that boundary in the following year: `01/04` means, for example, 01/04/2026 through 31/03/2027. No period/entitlement calculations are implemented here.

An absent row loads defaults of `01/04`, 5.6 weeks, `01/04`, and 12.07 percent without writing to the database. Save Payroll Settings validates these inputs before any payroll writes, then saves the four values in one atomic SQLite statement after the existing payroll-settings save. The config/provider and annual-leave writes are not a shared transaction; failures are reported explicitly, including when the other payroll settings have already saved. There is no separate leave-year-start config field, rule creation, history, editing/deletion workflow, or confirmation machinery. No compatibility repair for earlier development-only schema-26 layouts is included; local development databases may be reset manually.

Personal Assistants have an optional Leaving date, stored as canonical `DD/MM/YYYY`. Schema 27 adds nullable `personal_assistants.leaving_date` without backfilling dates or altering historical data. A supplied date must be real and not precede Start date. Stored Active/Inactive status is never changed automatically. Ordinary selected-period payroll inclusion requires Active (or legacy unset status) and inclusive employment-date overlap with the four-week period. Existing selected-period preparation records remain included regardless of status or employment dates. No annual-leave calculations are implemented; the leaving boundary is available for a future inclusive entitlement cap.
