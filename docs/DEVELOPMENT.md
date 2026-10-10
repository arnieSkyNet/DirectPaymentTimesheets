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

Keep recovery details in `BackupService` and `database_recovery`. Use SQLite's
online backup API with a consistent read snapshot, complete schema/typed-row
verification and read-only integrity checks, never a main-file-only copy of an
open WAL database. Startup must preserve a verified original before any legacy
migration. Run the entire chain on an isolated copy; install transactionally with
the live writer reservation and dispatch OS lock held. Do not call `create_schema`
on a production connection to bypass recovery.

Restore remains confined to recognised application backup directories. Legacy
backups migrate in isolation; approval binds exact live/selected/staged evidence
and configuration. Require a nonblank reason and rollback acknowledgement, reject
stale approval and unresolved sends, and flush a separately verified recovery
snapshot/decision manifest before installation. Restart after success. Inventory
external file paths only: never silently copy/restore business files. Full recovery
also requires separately protected business folders and signatures. Tests use
isolated temporary roots, including WAL and child-process interruption fixtures.

## Current document and maintenance boundaries

Unified Import Payroll Documents classifies individual files/ZIPs before any optional ordinary-payslip period choice. P60/P45 use cycle-independent schema31 document IDs, extended by schema33 history reconciliation and schema35 supersession. P30 and other shared information use their own filename year or configured root. Preserve safe one-PA token matching, ZIP limits, idempotency, archival exclusions and mixed-import partial-result reporting described in [Architecture](ARCHITECTURE.md#payroll-document-import-and-delivery). Replacement, inactive filing and cleanup-journal rules are documented in the architecture's schema34/35 sections.

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

At the schema32 milestone, sickness dates did not invalidate candidates or participate
in candidate evidence signatures. Stage 2/schema37 closes that gap; see the Stage 2
workflow below.

## Individual PA payroll production (schema 32)

Dashboard **Generate Payroll Timesheets** opens a PA selection for the captured
operational period. Available saved preparations are selected initially; use
**Clear selection**, individual checkboxes or **Select all available** for one,
some or all PAs. Missing preparation, settled and indeterminate records
are shown with reasons and cannot be selected. **Generate selected PAs** uses the
ordinary per-PA candidate publication, including saved Payroll Department notes.

Production **Email Payroll Timesheets** starts with a separate recipient selection.
Only PAs with an unsent current PDF at the registered path and matching digest are
initially selected. Continue to the existing optional email-body Additional Note
controls, then review the selected names, period and candidate details before Send.
The email-body note remains separate from the saved note printed in the PDF.
Current evidence and candidate integrity are checked again for first sends. Sent versions remain visible unticked and can be explicitly acknowledged for immutable-byte resend. **Select all unsent** applies only to timesheet emailing.

Selected IDs and period dates/revision are captured. Changing the operational
period (even away and back), deleting it or changing its dates rejects execution.
Evidence loading, duplicate preflight and reconciliation are scoped to the PA
being processed. Displaying other PAs' availability is read-only. A malformed
unselected PA's work/preparation cannot block the selected PA or obsolete that
other PA's duplicate decisions. Global import/review preflight remains available.

Results list every selected PA as completed, skipped, failed/needs attention or
not attempted. Processing stops on the first failure; earlier successful results
remain committed. A new selection leaves sent current documents unticked. Replaying
an accepted intent skips it; a rejected intent requires a fresh selection.
Indeterminate delivery stays protected; only established non-acceptance releases
retry eligibility. Corrected regeneration and authorised correction workflows
retain original submissions, evidence and settlement records. Returned actual
in-lieu hours are unaffected. Selection/acknowledgement is transient; schema36
persists document identities, transport attempts, frozen payloads and reviews.

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

## Artifact-only package rehearsals

See [Release-build rehearsals](RELEASE-BUILDS.md) for the six architecture targets,
nine packages, explicit target/tool pins, local validation and required GitHub
proof runs. This infrastructure does not publish releases or change application
version/schema. Public downloads remain those documented in README.


## Schema36 delivery invariants

Use `database::open` for writable connections: it registers the schema compatibility
function used by migration36 guards. Ordinary SQL mutations by raw older
connections fail closed; SQLite backup restoration bypasses those triggers.
Published 1.0.4/1.0.5 binaries do not have the new maintenance safeguards and must
not open upgraded production data. `create_schema` rejects unsupported future versions before migration.
Do not remove guards or silently reset schema_version. Schema35 is the upgrade
source for published Linux/Windows 1.0.4 and macOS 1.0.5; earlier supported schema
fixtures exercise the ordered chain through 36.

Production timesheet dispatch must go through `timesheet_delivery::execute` with a
captured intent and routing validation under the writer lock. Prepare MIME from
approved verified bytes; never reopen a pathname during transport. Retain the OS
`*.timesheet-delivery.lock` sidecar (do not unlink it while instances can be active).
Do not treat an exception as proof of non-acceptance. Never create another payroll
submission to record a resend. Test claim races, acknowledgement loss, stale
confirmation, restart/review, partial retry and original correction applications.


## Stage 2 sickness protection (1.0.6/schema37)

Sickness remains manual inclusive dates, separate from CSV worked evidence, leave and public holidays. Open Sick / SSP in preparation; protected preparation has a separate sickness-review control outside its read-only hours grid. Enter a reason, choose normal editable scope, explicit editable-only scope, or authorised sickness-only correction, then review the old/new dates and every affected period. Authorised correction additionally requires confirmation that original PDFs and original dates were reviewed. Delivery-uncertain periods remain blocked until audited resolution.

Editable-only scope retains a protected cycle's full-date projection, even if the PA-wide period changes or is deleted. The cycle view can subsequently receive its own authorised correction without changing another cycle. Date IDs are never reused after deletion. Audits and immutable document, attempt and submission snapshots preserve the original evidence. A new correction document starts as a first send; the accepted version subsequently defaults unticked and uses the existing deliberate resend acknowledgement.

For settled corrections, no settlement marker is cleared and no worked-minute correction is created. Generation uses the retained submission's worked membership/totals and refuses changed payroll totals. The new filename includes `Sickness-Correction-<id>` and preserves the original file. Full dates stay in their ordinary weekly cells. Only the notes area receives `Sickness Information Correction`, numbered with the next asterisk count after populated notes; no empty notes are created. The footnote tells Payroll to assess financial implications, with settled amounts unchanged and no SSP calculation. Existing font/layout and overflow refusal remain in force; long existing notes may exceed the remaining space before generation can succeed.

Migration37 is atomic and uses the existing verified database/config backup, isolated migration chain and dispatch/writer locks before live installation. It does not invent structured sickness snapshots for older PDFs. Unsent older PDFs must be regenerated before first delivery; previously submitted PDFs retain deliberate immutable-byte resend. Legacy original dates without a captured structured snapshot require review of the retained original PDF before correction authorisation. Business PDFs/signatures are external to database/config backup coverage; protect them separately.

Regression coverage is in `sickness_service_tests.rs`, the sickness editor/PDF tests, recovery tests and the Stage 2 production SMTP/PDF tests. All fixtures use temporary databases/folders. Public-holiday identification/subtraction, annual-leave accrual and SSP calculations are outside this change.


### Retained-period transfers and original financial evidence

Every reviewed sickness change validates the complete union of original and proposed ranges, including local edits to a retained cycle view. Delivery uncertainty in any overlapping cycle blocks the operation. Historical delivery markers without a preparation/submission baseline also refuse mutation until the original payroll evidence is recovered.

A retained period whose PA-wide row was deleted can be shortened or deleted locally with authorised correction while other historical views remain retained. Moving or extending its dates outside the selected historical cycle is an explicit reviewed transfer: it creates a fresh, non-reused active period identity and publishes its full inclusive dates into every destination projection, including future preparation records. Changed unsent candidates are invalidated in the same transaction. Protected destinations require correction authority; uncertain destinations refuse every scope. If an existing PA-wide version or a conflicting retained version would be overwritten, the transfer is refused without an audit claiming success. Reconcile those dates first. Editable-only scope still freezes protected views rather than altering their evidence. The editor displays the persisted selected-cycle view after saving, and preparation candidate indicators refresh immediately without replacing other unsaved drafts.

Sickness-only correction authority binds the exact original submission's financial evidence fingerprint. Generation and first delivery compare retained worked membership, correction applications, weekly values, dated leave/public-holiday records and carry-forward. New publication timestamps and row IDs are excluded from business-value comparison. Carry-forward is recovered from original late-work/legacy-carry/correction items, never inferred from today's mutable preparation value; inconsistent or insufficient legacy evidence refuses the correction and requests recovery/reconciliation. Original settlement values and original PDFs remain unchanged. This adds no SSP calculation or public-holiday classification/subtraction changes.

Regression tests include populated schema35/schema36 databases created by the actual ordered migration steps (with the original non-AUTOINCREMENT sickness table), WAL-backed upgrade/restore, a failure after schema36 commits on the isolated copy, retained transfers across lifecycle states, nonzero worked/carry/correction evidence, parent indicators and full PA archive/reactivation with retained sickness snapshots and delivery history. Schema remains 37 and application version remains 1.0.6.

## Stage 3 signature validation and drawing

`signature.rs` shares content validation across import, drawing, pre-generation
review and PDF embedding. Limits are 8 MiB, 4096 pixels per dimension and bounded
decoder allocation. PNG container completion and strict JPEG decoding reject
malformed/truncated inputs; detection follows content rather than extensions.
The JPEG container walk ignores embedded Exif thumbnail end markers. Exif
orientation is normalised into PNG pixels before PDF placement. Cargo explicitly
enables both printpdf PNG and JPEG features.

Drawn signatures are 1000×300 RGBA PNGs, black with transparent background.
Full-name files (`Name.png`, `Name (2).png`, etc.) are allocated with conservative
Unicode caseless/normalisation collision checks, published atomically without
overwrite and flushed before a signature-column-only database transaction commits. The
existing production/recovery OS lock and immediate writer transaction protect
publication, including concurrent instances; stale references refuse updates.
Names come from the database inside the save transaction; IDs remain the ownership and
authorisation keys. Blank names fall back to Employer/PA plus ID. Cross-platform
sanitisation preserves Unicode/punctuation, replaces forbidden characters,
protects Windows device names and caps the stem at 180 UTF-8 bytes to reserve
filesystem component space for numbering. Very long names are therefore truncated.
Directories, symlinks and unreferenced files reserve names; identical ink also
receives a new filename. Legacy hash/external references are never rewritten.
An interrupted save may leave an unreferenced immutable asset, never a partially
replaced signature. Existing assets are deliberately retained, not cleaned up.
Stage 3 required no schema migration and retained 1.0.6/schema37; Stage 4 subsequently introduces schema38.

`signature`-filtered tests cover format variants, bounds, corruption, renamed
files, replacement preservation, drawing pixels/ownership, stale saves and
cross-instance locking. Production-workflow tests verify immediate and repeated
PDF use, unsigned email, failed-generation preservation and immutable historical
resends using temporary databases/assets and localhost SMTP fixtures. Test JPEGs
under `src/test_fixtures/` are project-generated black rectangles, not signatures.
Touch/stylus drawing depends on eframe/egui receiving primary-pointer events;
pressure and multi-pointer input are not interpreted. No production GUI/hardware
rehearsal is part of automated validation.

Database/config backups do not copy managed `signatures/` assets or external
images. Preserve the full configured data root and external assets separately;
restoration keeps those files untouched and restores only stored references.


## Stage 4 shift change protection (1.0.6/schema38)

`shift_changes` owns additive schema38, content identity, legacy-archive registration, retained counterpart links and prospective review signatures. CSV imports hold the production OS lock and a writer reservation before resolving PAs/classifying rows; source/archive bytes are captured once. All logger writes use the same lock and transactional writer reservation. Edits compare an original record and a reviewed complete payload, checking both old/new ranges plus retained submission membership. Never seed legacy authorisation events or rewrite historical correction obligations during migration.

The CSV parser maps semantic headers independently of ordering. Supported aliases are Client Name/PA, Start Time/Start, End Time/End, Break Time/Break, Worked Hours/Worked, Rate/h/Rate, Amount and Note. Names ignore case, surrounding whitespace and a leading BOM; missing, unknown and duplicate semantic fields are refused. Both real and synthetic test variants remain supported. Tests use only synthetic temporary CSVs/databases; production exports must not be opened by development tests.

Duplicate review retains original sources, supports explicit retain-all and audited deferral, and requires fresh authorisation if displayed evidence/payroll changes. New protected edits cannot use the repository's ordinary editing API as a bypass. Financial changes use existing contextual resubmission/carry-forward, with original settlements and PDFs preserved. Regression coverage is in `payroll_evidence/stage4_tests.rs`, header tests, populated schema37 migration tests, and the existing production/recovery suites. No public-holiday, sickness, signature, PDF-layout or SMTP policy changes are part of Stage 4.
