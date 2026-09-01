# DirectPaymentTimesheets database schema

## Scope and versioning

This is the implemented SQLite schema at version 20. It is derived from `create_schema` and migrations in `src/database.rs`; those migrations are authoritative.

`schema_version` contains the current integer version. A new database begins at version 1 and receives each ordered migration through `CURRENT_SCHEMA_VERSION` 20. Existing databases are upgraded in place.

During unreleased schema-20 development, an earlier local database shape contained `direct_shifts` without soft-deletion columns or the audit table. Startup therefore performs an idempotent schema-20 compatibility check after normal migrations. When that exact incomplete shape is found, it transactionally rebuilds `direct_shifts` into the final constrained form while preserving IDs and row values, then creates the audit table/indexes. It does not fabricate historical audit events, and repeated startup does not duplicate existing audit rows.

SQLite foreign-key constraints are not declared in schema 20. Relationships described below are logical relationships enforced by repository/application code and stored IDs/business keys.

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

Dates are stored as `DD/MM/YYYY`. Repository logic treats `(payroll_year, cycle_number)` as schedule identity, but schema 20 does not declare that pair unique. Validated import supplies exactly 13 chronological four-week cycles and replaces one year transactionally.

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
- one occurrence of a non-null source TimesheetEntry ID per payroll-timesheet snapshot.

Schema 20 also declares the direct-shift running/recent indexes and direct-shift value checks described above. No foreign keys or cascading deletes are declared. Repository and service validation supplies the remaining business rules.
