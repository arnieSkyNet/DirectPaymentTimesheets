# DirectPaymentTimesheets development guide

## Scope

This is a local Rust/egui/SQLite application. Prefer small, evidence-led changes that preserve payroll history and existing provider output. Inspect the current source, schema and tests before changing behaviour; documentation and conversation history are secondary evidence.

Current package version is `0.0.12`; current SQLite schema version is 27. Do not change either unless a task explicitly requires it.

## Local setup

From the repository root:

```bash
cargo check
cargo test
cargo run
```

By default, running the application uses `~/.directpaymenttimesheets`. For development against disposable data, set `DIRECTPAYMENTTIMESHEETS_HOME` to a dedicated temporary/test directory. Never point exploratory runs or tests at the real database.

The application creates `database.sqlite`, `config.toml` and its internal subdirectories beneath that root. Configured PDF, payslip, payroll-information and other business folders can be elsewhere, so inspect configuration before any manual integration test that writes files.

Real runtime and payroll data must remain outside Git and must never be committed. This includes databases, configuration containing credentials or personal paths, imported/archived files, generated payroll documents, payslips and backups.

## Required validation

For a normal Rust change, run:

```bash
cargo fmt
cargo fmt -- --check
cargo check
cargo test
git diff --check
git status --short
```

For documentation-only work, `git diff --check`, `cargo check` and `cargo test` provide formatting and regression assurance. Review `git diff --stat` and the complete diff before handoff. Do not commit or push unless explicitly requested.

## Source organisation

Key entry/orchestration modules:

- `main.rs`: module wiring, environment/database/config initialisation and eframe startup.
- `application.rs` and `app.rs`: application operations and repository access.
- `gui.rs`: Dashboard, selector/dialog state, navigation and email workflow orchestration.
- `context.rs`, `environment.rs`, `config.rs`, `paths.rs`: runtime context and paths.

Persistence and domain repositories:

- `database.rs`: schema creation and migrations.
- `repository.rs`, `personal_assistant_repository.rs`, `employer_repository.rs`, `payroll_provider_repository.rs`.
- `pay_rate_repository.rs`, `contracted_hours_repository.rs`.
- `payroll_schedule_repository.rs`, `payroll_timesheet_repository.rs`, `payroll_timesheet_email_repository.rs`, `payroll_worked_item_repository.rs`.

Services/adapters:

- `csv_import.rs`, `import_service.rs`, `archive.rs`.
- `payroll_prep_sheet_import_service.rs`.
- `pay_rate_allocation.rs` and `payroll_snapshot_service.rs`.
- `payroll_file_naming.rs`, `pdf_generator.rs`, `email_service.rs`, `backup_service.rs`.

Dedicated screens include employer, PA, payroll settings, payroll preparation and application settings. Email settings and several workflow dialogs are currently coordinated from `gui.rs`.

`src/main.rs.before_application_refactor` is a retained historical/reference file, not the compiled application path; do not treat it as production behaviour.

CSV import parses `worked_minutes` from the supplied worked-duration field. It does not derive payable time from start/end values or consult the persisted rounding setting. Successful CSV files are copied to the internal `archive/YYYY/MM/` hierarchy with timestamped names and recorded in `import_audit`.

## Database conventions

The database is upgraded by sequential functions in `database.rs`. To add persisted state:

1. increment `CURRENT_SCHEMA_VERSION` only when authorised;
2. add one ordered migration that preserves existing data;
3. update repository/model code;
4. add tests for fresh initialisation and the relevant upgrade/compatibility path; and
5. do not mutate a real database during automated work.

Schema 19 added manual adjustments, worked-item snapshot rows and candidate/submitted/indeterminate state with PDF path and SHA-256 digest. Preserve its constraints: opaque legacy previous-cycle rows alone may have null rate/date evidence; ordinary allocated items require it.

Use stable business keys where the workflow does. Payroll operations identify schedules by `(payroll_year, cycle_number)` and then validate material dates. Row IDs must not replace this identity in UI state.

## Dates and payroll periods

Stored business dates are commonly `DD/MM/YYYY`; timestamps use existing ISO-like formats. Parse dates before chronological comparison rather than relying on string sort.

Do not derive operational payroll year from the calendar. Use `PayrollScheduleRepository::resolve_for_date` for a date-based default and lookup by year/internal cycle immediately before an operation. Reject gaps and ambiguity. Use the chronological predecessor resolver for previous-cycle work, including year rollover.

Payroll Prep Sheet PDF extraction is implemented. DOCX is recognised but deliberately returns not implemented. On re-import, preserve `payslips_sent` for cycles whose material dates are unchanged; safely replaced changed cycles use reset sent state, and prepared payroll history blocks unsafe date changes.

Keep the three selector concerns separate:

- Dashboard operational payroll period;
- Payroll Return's explicit period; and
- View Payroll Schedule's selected year.

Normal labels use Payroll Week, period dates and pay date. Internal cycle numbers may appear in code/tests/diagnostics but not ordinary user-facing labels.

## Payroll file conventions

Use `payroll_file_naming` for every producer and consumer. Do not reimplement period strings, PAYE weeks or paths.

- PAYE week comes from `pay_date` relative to the applicable 6 April.
- Period code is `YYYYMMwWW`, with `YYYYMM` from first week commencing.
- Payslip filename is `Payslip for Week N for Name.pdf`.
- A root ending in `YYYY to YYYY` is normalised back to its parent before applying another schedule year.
- Generic timesheet PDF roots stay flat; year-suffixed timesheet roots roll to sibling years.
- Create directories only immediately before an actual write/import.

When adding file writes, preserve collision handling and propagate directory/write errors before changing related database state.

## Payroll preparation and snapshots

`PayrollTimesheetScreen` must be supplied a complete operational schedule. Its bound key and material dates are a stale-save guard. Do not add an independent `Local::now()` period resolver; current-time calls in this screen are for timestamps only.

Preparation PA eligibility is active/legacy-NULL-active plus any PA with an existing selected-period payroll record. Generation deliberately uses the same union. Do not create selected-period records for unrelated inactive PAs.

Submitted and indeterminate records are persisted-only/read-only. Loading them must not create missing weeks/holidays, reconcile imports or update timestamps. Candidate/unsent records are editable.

Before changing data represented by a candidate:

1. compare the represented preparation/worked-item data;
2. discard/invalidate the candidate before the first preparation write when material data changed; and
3. abort the mutation if invalidation fails.

Material changes include item membership/date/rate/manual allocation, previous-cycle minutes, weekly worked totals, annual leave, sick/SSP, individual holiday hours and mileage. Merely opening unchanged data must not discard the candidate.

Generate via `payroll_snapshot_service::publish_candidate`, not direct final-path PDF writing. Production timesheet send must verify the candidate path/digest and preserve submitted/indeterminate transitions.

## Effective-dated data

Pay-rate lookup is as of each imported shift's start date and returns newest effective date then newest ID. It excludes future rates and includes employer top-up. Missing effective rate for positive work is an error. Manual adjustment allocation rules and integer-minute reconciliation live in `pay_rate_allocation`; do not use imported CSV rate/amount.

Contracted-hours lookup is independently as of each payroll week commencing date. It is PDF information only. Do not apply pay-rate midweek rules to contracted hours or make it affect worked totals.

## Email development

Keep preview, test and production semantics distinct:

- preview composes only;
- test sends use configured test recipients and markers and do not freeze snapshots;
- production uses established employer/payroll/PA routing and updates status/snapshot state.

That production route is the employer as sender, payroll department as recipient, employer as CC and PA as BCC when available for both timesheets and payslips. Payslip test email instead targets the configured PA test address. `PendingEmailBatch` captures selected PA IDs and period facts/revision, not resolved email addresses. Production batches currently include active/legacy-`NULL` PAs only; the inactive-with-existing-record eligibility rule is limited to preparation and generation.

Production batches capture the selected schedule and selection revision. Final dispatch must re-fetch and validate that schedule, refuse a changed global selection and use only captured/re-fetched period facts for attachments, subject and status. Never re-resolve today during dispatch.

## Backup/restore development

Keep filesystem and SQLite details in `BackupService`. A backup must use SQLite's online backup API, not copy an open database file. Restore must stay confined to recognised application backup directories, validate read-only integrity/schema, create a safety backup first and require restart after success. Tests use temporary roots only.

## Testing conventions

Prefer focused module tests near the implementation. Use `Connection::open_in_memory()` or a temporary database/file tree. Inject dates/schedules and test stable helpers rather than depending on today's date.

Important regression areas include:

- all-years schedule import, replacement, resolution and predecessor rollover;
- PAYE week/year-directory path agreement between import and lookup;
- effective-date boundaries and deterministic equal-date rows;
- preparation read-only, candidate invalidation and stale-save behaviour;
- snapshot membership, digest verification and indeterminate transport handling;
- inactive historical PA eligibility without unrelated record creation;
- PDF totals/configured font roles without exposing internal rates;
- email batch period binding and test/production routing; and
- backup validation/restore with no access to runtime data.

Avoid brittle pixel assertions for PDFs. Test prepared content/data and extract text where practical.

## Configuration compatibility

Serde ignores unknown TOML fields, so removed settings such as `public_holiday_enabled` remain load-compatible but are not saved. New optional fields should normally have defaults. Preserve legacy timesheet email-body reconciliation unless a separately scoped migration removes it.

The persisted payroll frequency, rounding, workweek and overtime settings are not currently applied as downstream configurable calculation rules. Do not wire them into unrelated behaviour merely because they exist. Email subject/body fields live in `PayrollConfig` but are edited through Email Settings. The configured `email_archive` path currently has no production consumer; Payroll Return information must continue to use `payroll_information_folder`. Public holidays are permanently active.

## Current boundaries

Do not document or build these as if already present: overtime calculations, arbitrary/scheduled/cloud restore, backup retention, P60-specific Payroll Return handling, multi-user/authentication, or a general payroll calculation engine. Keep future-work descriptions explicit and separate from implemented behaviour.

Annual-leave settings foundation: schema 26 uses a singleton `annual_leave_settings` SQLite row for two recurring DD/MM boundaries, statutory weeks and accrual percentage. Missing settings load defaults 01/04, 5.6, 01/04, 12.07 without persistence. Save Payroll Settings writes all four together. There is no separate config leave-year start or effective-dated rule history. Annual periods will end inclusively the day before the next recurrence of their boundary; no entitlement or statistics calculations are implemented. Earlier development-only schema-26 databases require manual reset, not production repair logic.

Personal Assistants have an optional Leaving date, stored as canonical `DD/MM/YYYY`. Schema 27 adds nullable `personal_assistants.leaving_date` without backfilling dates or altering historical data. A supplied date must be real and not precede Start date. Stored Active/Inactive status is never changed automatically. Ordinary selected-period payroll inclusion requires Active (or legacy unset status) and inclusive employment-date overlap with the four-week period. Existing selected-period preparation records remain included regardless of status or employment dates. No annual-leave calculations are implemented; the leaving boundary is available for a future inclusive entitlement cap.
