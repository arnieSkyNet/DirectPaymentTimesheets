# DirectPaymentTimesheets development guide

## Scope

This is a local Rust/egui/SQLite application. Prefer small, evidence-led changes that preserve payroll history and existing provider output. Inspect the current source, schema and tests before changing behaviour; documentation and conversation history are secondary evidence.

Current package version is `1.0.0`; current SQLite schema version is 31. Do not change either unless a task explicitly requires it.

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

- `main.rs`: module wiring and the call to `application::run`.
- `application.rs`: startup/database orchestration and eframe launch; `app.rs` / `Application::initialise` and context/environment code: configuration, environment and repository access.
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

Schema 19 added manual adjustments, worked-item snapshot rows and candidate/submitted/indeterminate state with PDF path and SHA-256 digest. Preserve its constraints: opaque legacy previous-cycle rows and explicit schema-28 correction items may retain unavailable rate/date evidence; ordinary allocated source items require it.

Use stable business keys where the workflow does. Payroll operations identify schedules by `(payroll_year, cycle_number)` and then validate material dates. Row IDs must not replace this identity in UI state.

## Dates and payroll periods

Stored business dates are commonly `DD/MM/YYYY`; timestamps use existing ISO-like formats. Parse dates before chronological comparison rather than relying on string sort.

Do not derive operational payroll year from the calendar. Use `PayrollScheduleRepository::resolve_for_date` for a date-based default and lookup by year/internal cycle immediately before an operation. Reject gaps and ambiguity. Use the chronological predecessor resolver for previous-cycle work, including year rollover.

Payroll Prep Sheet PDF extraction is implemented. DOCX is recognised but deliberately returns not implemented. On re-import, preserve `payslips_sent` for cycles whose material dates are unchanged; safely replaced changed cycles use reset sent state, and prepared payroll history blocks unsafe date changes.

Keep the three selector concerns separate:

- Dashboard operational payroll period;
- Import Payroll Documents' optional period after source classification; and
- View Payroll Schedule's selected year.

Normal labels use Payroll Week, period dates and pay date. Internal cycle numbers may appear in code/tests/diagnostics but not ordinary user-facing labels.

## Payroll file conventions

Use `payroll_file_naming` for every producer and consumer. Do not reimplement period strings, PAYE weeks or paths.

- PAYE week comes from `pay_date` relative to the applicable 6 April.
- Period code is `YYYYMMwWW`, with `YYYYMM` from first week commencing.
- Payslip filename is `Payslip for Week N for Name.pdf`.
- A root ending in `YYYY to YYYY` is normalised back to its parent before applying another schedule year.
- Generic timesheet PDF roots stay flat; year-suffixed timesheet roots roll to sibling years.
- Create schedule-derived directories only immediately before an actual write/import. Saving configuration also ensures configured business roots exist.

When adding file writes, preserve collision handling and propagate directory/write errors before changing related database state.

## Payroll preparation and snapshots

`PayrollTimesheetScreen` must be supplied a complete operational schedule. Its bound key and material dates are a stale-save guard. Do not add an independent `Local::now()` period resolver; current-time calls in this screen are for timestamps only.

Preparation, generation and timesheet email use inclusive Start/Leaving date overlap independent of current Active status, plus any PA with an existing selected-period record even outside those dates. Missing boundaries are unbounded.

Submitted and indeterminate records are persisted-only/read-only. Loading them must not create missing weeks/holidays, reconcile imports or update timestamps. Candidate/unsent records are editable.

Before changing data represented by a candidate:

1. compare the represented preparation/worked-item data;
2. discard/invalidate the candidate before the first preparation write when material data changed; and
3. abort the mutation if invalidation fails.

Material changes include item membership/date/rate/manual allocation, previous-cycle minutes, weekly worked totals, annual leave, sick/SSP, individual holiday hours and mileage. Merely opening unchanged data must not discard the candidate.

Generate via `payroll_snapshot_service::publish_candidate`, not direct final-path PDF writing. Production timesheet send must verify the candidate path/digest and preserve submitted/indeterminate transitions.

## Effective-dated data

Pay-rate lookup is as of each imported shift's start date and returns newest effective date then newest ID. It excludes future rates and includes employer top-up. Missing effective rate for positive work is an error. Manual adjustment allocation rules and integer-minute reconciliation live in `pay_rate_allocation`; do not use imported CSV rate/amount.

Contracted-hours lookup is independently as of each payroll week commencing date. It supplies PDF information and annual-leave guidance. Do not apply pay-rate midweek rules to contracted hours or make it affect worked totals.

## Email development

Keep preview, test and production semantics distinct:

- preview composes only;
- test sends use configured test recipients and markers and do not freeze snapshots;
- production uses established employer/payroll/PA routing and updates status/snapshot state.

That production route is the employer as sender, payroll department as recipient, employer as CC and PA as BCC when available for both timesheets and payslips. Payslip test email instead targets the configured PA test address. `PendingEmailBatch` captures selected PA IDs and period facts/revision, not resolved email addresses. Timesheets use selected-period eligibility; payslip/document batches additionally include unsent P60/P45 recipients. Standalone documents need no schedule; see [Email rules](DOMAIN.md#email).

Production batches capture the optional selected schedule and selection revision; timesheets require a schedule. For period-associated sends, final dispatch must re-fetch and validate that schedule, refuse a changed global selection and use only captured/re-fetched period facts. Standalone document attachments/status use document IDs and neutral wording, never a fabricated period. Never re-resolve today during dispatch.

## Backup/restore development

Keep filesystem and SQLite details in `BackupService`. A backup must use SQLite's online backup API, not copy an open database file. Restore must stay confined to recognised application backup directories, validate read-only integrity/schema, create a safety backup first and require restart after success. Tests use temporary roots only.

## Current document and maintenance boundaries

Unified Import Payroll Documents classifies individual files/ZIPs before any optional ordinary-payslip period choice. P60/P45 use independent schema31 document IDs; information uses its own filename year or configured root. Preserve idempotency, archival exclusions and mixed-import partial-result reporting described in [Architecture](ARCHITECTURE.md#schema31-payroll-documents).

Employer feature flags and PA sickness enablement are legacy storage, not feature gates. Structured sickness records report dates to Payroll; PA mileage remains the mileage gate. Maintenance layout, footer controls and user-facing rules are summarised in [README](../README.md#records-and-settings) and [Domain](DOMAIN.md#sickness-and-weekly-mileage). Do not reintroduce gates from old columns.

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

Email regression tests use temporary data and an isolated loopback SMTP sink; environments that prohibit binding local ports need permission to run those tests outside that restriction. They do not use production SMTP.

Avoid brittle pixel assertions for PDFs. Test prepared content/data and extract text where practical.

## Configuration compatibility

Serde ignores unknown TOML fields, so removed settings such as `public_holiday_enabled` remain load-compatible but are not saved. New optional fields should normally have defaults. Preserve legacy timesheet email-body reconciliation unless a separately scoped migration removes it.

The persisted payroll frequency, workweek and overtime settings are not currently applied as downstream configurable calculation rules. Do not wire them into unrelated behaviour merely because they exist. Email subject/body fields live in `PayrollConfig` but are edited through Email Settings. The configured `email_archive` path currently has no production consumer; Payroll Return information must continue to use `payroll_information_folder`. Public holidays are permanently active.

## Current boundaries

Do not document or build these as if already present: overtime calculations, arbitrary/scheduled/cloud restore, backup retention, multi-user/authentication, or a general payroll calculation engine. Keep future-work descriptions explicit and separate from implemented behaviour.

Annual-leave settings foundation: schema 26 uses a singleton `annual_leave_settings` SQLite row for two recurring DD/MM boundaries, statutory weeks and accrual percentage. Missing settings load defaults 01/04, 5.6, 01/04, 12.07 without persistence. Save Payroll Settings writes all four together. There is no separate config leave-year start or effective-dated rule history. The two recurring effective-from dates apply to their respective rules; they do not set leave-year boundaries or change a PA’s Hours Basis. Annual-leave guidance always uses 1 April through 31 March inclusive, even when the two settings dates differ. Earlier development-only schema-26 databases require manual reset, not production repair logic.

Personal Assistants have an optional Leaving date, stored as canonical `DD/MM/YYYY`. Schema 27 adds nullable `personal_assistants.leaving_date` without backfilling dates or altering historical data. A supplied date must be real and not precede Start date. Stored Active/Inactive status is never changed automatically. Ordinary selected-period payroll inclusion uses inclusive Start/Leaving date overlap with the four-week period, independent of current Active/Inactive status. Missing employment boundaries remain unbounded. Existing selected-period preparation records remain included regardless of status or employment dates. Annual-leave guidance applies Start and Leaving dates inclusively without changing employment status.


### Annual-leave guidance (schema 27 unchanged)

`annual_leave_summary` reads existing repositories and calculates guidance in memory. It never reconciles raw shifts, creates preparation records, writes settings defaults, caches derived totals in SQLite, or alters payroll/PDF/email behaviour. The PA screen refreshes saved evidence on PA selection, return to the screen, date rollover, successful PA/Hours Basis saves, and explicit Refresh. Historical selection only recalculates the loaded evidence.

- Years are fixed 1 April–31 March. The current year is always offered; previous years come from employment dates, Hours Basis history, preparation weeks, dated leave and actual worked dates (application schedules supply evidence where Start date is absent).
- Contracted entitlement is weekly hours × configured statutory weeks × inclusive segment days / actual days in the selected leave year. Effective-dated Hours Basis history is ordered by date and then id, preserving highest-id precedence on equal dates. Start/Leaving and midweek changes are applied on actual dates. The full attributable Contracted portion is included, including future contracted days; Variable work is never projected.
- Variable evidence comes from retained `PayrollWorkedItemRepository` submitted snapshots, with a safe historical fallback to weekly preparation worked totals when item evidence is unavailable and whole-week attribution is unambiguous, gated by this PA/cycle’s definitively Sent `email_type='payslip'` state from `PayrollTimesheetEmailRepository`. Neither a timesheet email nor the schedule-level sent flag suffices. Imported and previous-cycle late shifts use retained actual dates. Qualifying minutes are totalled per four-week cycle, multiplied by the configured percentage and rounded once to whole hours (half up). Annual leave, Sick/SSP, public-holiday hours and mileage are not worked hours. Calculated to is the latest contributing cycle’s scheduled end, which can follow a historical year when late shifts settle later.
- Dated leave uses actual leave dates; legacy undated leave uses the week commencing date, including crossing weeks. Dated rows supersede the weekly aggregate. Taken is recorded leave, independent of payslip Sent status; negative remaining is displayed.
- The two settings effective-from fields retain their separate recurring-rule meaning. With only a singleton numeric value per rule and no historical rule values, each recurrence uses the current saved/default value; no different historic value is invented. Details discloses this limitation. PA basis changes come exclusively from contracted-hours history.
- Undated manual worked adjustments can be allocated only when their entire week qualifies for the same year, employment and Variable basis and is not future. Ambiguous adjustments, undated previous-cycle legacy amounts, unusable submitted/aggregate evidence and missing/invalid Hours Basis history make guidance visibly incomplete; no dates or raw-shift reconstruction are invented. The current sickness warning checks legacy weekly `sick_leave_hours` in qualifying Variable weeks. It does not read structured sickness periods; do not describe those records as integrated into accrual. The sickness reference-period algorithm remains deferred.

Deterministic tests in `src/annual_leave_summary/tests.rs` cover year navigation, inclusive employment, actual-date history segments, mixed basis, per-cycle rounding, Sent gating, dated/legacy leave, incomplete evidence and read-only application loading with operational settings defaults. No GUI pixel tests or additional migrations are required.


## Evidence reconciliation (schema 28)

Read [PAYROLL-EVIDENCE.md](PAYROLL-EVIDENCE.md) before changing payroll source selection. Domain/repository logic lives in `payroll_evidence`, with thin duplicate and contextual review UIs. `pay_rate_allocation::add_evidence` accepts selected evidence and never decides duplicates. Both preparation and generation call the same reconciliation service. Imported-hours viewing still uses raw rows; payroll reads effective imported corrections plus completed direct evidence.

Do not replace source rows to resolve duplicates. Fingerprints omit notes but retain payroll-relevant identity and duration. Preflight checks stored evidence; it does not import CSVs. Source/correction consumption follows per-PA submission and definitive payslip state, not a schedule flag. Production submission archives its items, preparation data and attachment, and resubmission supersedes rather than deletes that history. Aggregate negative corrections require explicit actual-evidence completeness; unknown historical paid membership requires an audited paid/unpaid determination.

Tests in `payroll_evidence/tests.rs` cover source reconciliation, resolver invalidation, history, reviews, persistent corrections, zero-floor behaviour, stale sending and migration rollback. Existing import collision tests were intentionally updated: conflicting and within-file identical candidates now survive for user selection. Repeat exports of already-retained identical rows remain idempotent. Old migration fixtures remove schema-28 additions before simulating earlier versions, so upgrade tests exercise real old table shapes. Annual-leave tests retain their prior assertions apart from the current schema-version expectation and additional nullable snapshot source fields.

Schema 29 adds the verified legacy-settlement baseline described in
[Payroll evidence](PAYROLL-EVIDENCE.md#legacy-settlement-cutover-schema-29).
An existing schema-28 database needs its operator-verified inventory staged before
first startup of the schema-29 build to receive the exemption; current row IDs or
old work dates alone must never be used to reconstruct a missing cutover.

Shared payroll rounding is applied by `pay_rate_allocation::add_evidence` only to
eligible raw source durations for open payroll. It rounds each selected worked
item (including late work) using the configured increment and Up/Down direction,
after source break handling. It does not round source intervals, imported rows,
direct records, retained corrections or manual adjustments. Snapshots retain raw
`source_evidence` separately from payable `worked_minutes`. Historical discrepancy
checks recognise an unchanged raw duration as retaining its submitted payable
value, so rounding differences alone cannot produce a correction. Configuration
changes do not recalculate submitted or settled history.


## Payroll notes and actual in-lieu results (schema 32)

In Payroll Timesheet Preparation, select the PA and period. **Notes for Payroll
Department** appears directly beneath the weekly hours grid and above the existing
PA Save button. It accepts multiline text up to 256 Unicode characters. Overlong
input remains visible with an error and Save disabled; it is never silently
truncated. Save/Discard/Cancel, period/PA switching and window-close guards include
this draft. Saving a changed note atomically invalidates that record's PDF
candidate. Submitted/settled/indeterminate preparation remains protected.

The separate **Returned payroll information** panel has its own **Save returned
hours** and **Clear returned hours** controls. Leave **Actual in-lieu hours awarded
by Payroll** blank until known; an explicit zero is retained as 0.00. This panel
remains available for protected records, historical periods and departed PAs with
existing preparation records, including when worked-evidence review blocks the
grid. It saves only on its own buttons, not with PA preparation Save. Save the
returned result before leaving the screen. Stored precision is retained; display
uses two decimal places. No entitlement calculation or document extraction occurs.

PDFs remain one page. Blank/whitespace-only notes use the original layout without
a heading or additional gap. Nonblank notes use a 9 pt heading and 8 pt body in the
configured PDF fonts, after the hours table and before the declaration. Wrapping
uses measured font advances/ink bounds, preserves explicit line breaks, and
splits overlong tokens at Unicode character boundaries. The lower declaration,
signature labels, full-size signature images and dates move together according
to measured note height, with a reserved gap above the unchanged footer. Font
sizes are never reduced to force a fit. Excessive line breaks or font geometry
that cannot fit produce a clear generation error; text is retained and no
partial replacement PDF is published. Unsupported font glyphs also fail visibly.

Tests in `payroll_notes_return_tests.rs` and `payroll_notes_pdf_tests.rs` cover
migration/reopen, note limits/navigation/invalidation/rollback, single-page
wrapping/positioning and result isolation. The resubmission test also checks
retained original/replacement notes. Older migration fixtures explicitly remove
schema-32 columns before simulating older versions; their historical assertions
remain in force. Run `cargo fmt --check`, `cargo check --locked`, and
`cargo test --locked`; the existing mock SMTP tests require local TCP binding.

The pre-existing sickness-date editor candidate-invalidation gap is unchanged by
this work. These notes do not imply that structured sickness dates now participate
in candidate evidence signatures.

## Individual PA payroll production (schema 32)

Dashboard **Generate Payroll Timesheets** opens a PA selection for the captured
operational period. Available saved preparations are selected initially; use
**Clear selection**, individual checkboxes or **Select all available** for one,
some or all PAs. Missing preparation, submitted, settled and indeterminate records
are shown with reasons and cannot be selected. **Generate selected PAs** uses the
ordinary per-PA candidate publication, including saved Payroll Department notes.

Production **Email Payroll Timesheets** starts with a separate recipient selection.
Only PAs with a current candidate at the expected path and matching digest are
initially selected. Continue to the existing optional email-body Additional Note
controls, then review the selected names, period and candidate details before Send.
The email-body note remains separate from the saved note printed in the PDF.
Current evidence and candidate integrity are checked again for each actual send.

Selected IDs and period dates/revision are captured. Changing the operational
period (even away and back), deleting it or changing its dates rejects execution.
Evidence loading, duplicate preflight and reconciliation are scoped to the PA
being processed. Displaying other PAs' availability is read-only. A malformed
unselected PA's work/preparation cannot block the selected PA or obsolete that
other PA's duplicate decisions. Global import/review preflight remains available.

Results list every selected PA as completed, skipped, failed/needs attention or
not attempted. Processing stops on the first failure; earlier successful results
remain committed. A new selection excludes submitted/settled PAs. Retrying a
captured selection skips successful sends. Indeterminate delivery stays protected;
SMTP failure restores the candidate where possible. Legitimate replacements still
require **Correct / Resubmit Timesheet** and retain the original submission,
attachment, evidence and structured payroll note. Returned actual in-lieu hours
are unaffected by generation and email. Selection is transient: no schema change.

`src/payroll_production_tests.rs` exercises real generation and production dispatch
using temporary databases/PDF directories and localhost mock SMTP. It covers
five-PA isolation, one/some/all/empty selections, changed context, malformed
unselected evidence, scoped duplicate invalidation, candidate safety, notes and
returned-hour isolation, partial failures/retry, uncertain finalisation, authorised
replacement and historical/departed PA eligibility. These tests never open the
configured real payroll database or deliver email externally.

### Leaving-PA payroll note suggestion

When an editable PA's preparation is activated, a leaving date within the selected
four-week period (first and last day included) suggests:
`Leaving date DD/MM/YYYY. Please include any hours in lieu in final pay.`
The date comes from PA Maintenance and always uses UK date formatting. No hours
or CSV import is required. Only a blank/whitespace-only saved Payroll Department
note qualifies; any nonblank saved note remains exactly as entered, even if the
leaving date changes later. Protected or evidence-blocked preparation is unchanged.

The suggestion is an unsaved draft, marked dirty against the saved baseline. Edit,
replace or delete it freely within the existing 256-character limit. Save Personal
Assistant persists it through the normal note/candidate-invalidation transaction;
opening the screen does not save it. Save/Discard/Cancel navigation guards apply.
Discard writes nothing, and a later reload may suggest it again while the saved
note remains blank. Deleting or editing the draft does not regenerate it during
that load. Unselected PA drafts are not populated until activated. Returned
in-lieu hours, source shift notes and individual production are unaffected.

`src/payroll_leaving_note_tests.rs` covers date boundaries/formatting, blank and
nonblank notes, editing/deletion, navigation choices, PA/period isolation,
protected records, persistence/reopen and candidate invalidation on deliberate Save.
