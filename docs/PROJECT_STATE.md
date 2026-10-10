# DirectPaymentTimesheets project state

This document describes the implementation on `main`. Source code, migrations and tests are the authority if this summary becomes stale.

## Current release and platform

- Development application version: `1.0.6`; published Linux/Windows: `1.0.4`; published macOS: `1.0.5`.
- Database schema: version 36, upgraded in place by ordered SQLite migrations.
- Desktop UI: Rust with `eframe`/`egui`.
- Persistence: SQLite through `rusqlite` (bundled SQLite).
- Documents and integration: `printpdf`, PDF text extraction, ZIP import and SMTP via `lettre`.

The application is a single-user desktop tool for importing externally recorded work, preparing four-week payroll timesheets, generating the provider PDF, sending timesheets and payslips, and retaining the evidence needed to reproduce submitted payroll.

## Runtime data and configuration

The application data root is `~/.directpaymenttimesheets`, or the path in `DIRECTPAYMENTTIMESHEETS_HOME`. Initialisation creates:

```text
<data-root>/
  database.sqlite
  config.toml
  import/
  archive/
  backups/
  logs/
  templates/
  cache/
```

Configured business folders are separate from this internal data root: CSV import, PDF output, email archive, payslips and payroll information. Paths and PDF fonts/sizes are edited in Application Settings. `email_archive` is persisted but currently has no production consumer; Payroll Return information deliberately uses `payroll_information_folder` instead. SMTP/test-address settings and payroll subject/body templates are edited in Email Settings, although the template fields are stored in `PayrollConfig`. Existing unknown TOML keys are ignored, including the removed `public_holiday_enabled` key.

First-run and missing-key business-folder defaults are provider-neutral `~/Documents/DirectPaymentTimesheets/` subdirectories. Runtime expansion uses the current user's home directory; explicitly persisted absolute or relative paths are not rewritten on upgrade.

## Implemented workflow

### Maintenance

The application maintains employer, payroll-provider and Personal Assistant (PA) records. PA maintenance retains pay-rate and contracted-hours histories, including future-dated rows. Themes include system, light/dark variants, blue and high contrast. See [maintenance and Dashboard layout](../README.md#implemented-functionality) for the current Employer/PA controls and three-row workflow. Annual-leave rule values are stored in SQLite, and guidance is read-only; see [Domain](DOMAIN.md#annual-leave-guidance).

Payroll Settings persists payroll frequency, rounding choice, workweek start and overtime enabled. Payable allocation applies the configured increment and direction (default 15 minutes Up) to selected imported/internal source durations, preserving raw minutes. Frequency and workweek choices do not override schedule dates, and no overtime calculation is implemented. Public-holiday handling is permanently active and has no enable/disable setting. Email subject/body templates are configured separately through Email Settings.

### Importing work

CSV import parses provider rows, skips exact already-stored raw matches, retains conflicting/within-file candidates for audited duplicate resolution and stores stable `TimesheetEntry` identities. Start/end values are retained unchanged, and `worked_minutes` is parsed directly from the CSV worked-duration field rather than calculated from those values or altered using the persisted rounding settings. The imported CSV rate and amount are not authoritative payroll rates.

Schema 21 adds append-only imported-row correction events with actor/action metadata, optional reason and complete before/after start, end, break, worked-minute and note values. Raw and effective repository queries are explicit; the immutable imported row remains the raw source and reversion appends history. Payroll preparation, duplicate resolution and generation consume effective imported corrections through shared reconciliation. Import repeat detection and View Imported Hours retain raw source values. Schema28 submission history does not restore the removed PDF revision architecture.

After a successful import, the source CSV is copied to the internal `archive/YYYY/MM/` hierarchy with a timestamped filename and is not subsequently modified by the application. Successes and failures are recorded in `import_audit`, including row counts, source/archive paths and errors where applicable.

### Direct hours/shift evidence

Enter Hours/Shifts records actual work independently from imported CSV rows. A clock-in is persisted immediately with no end time, so a running shift survives restart. Clock-out stores the exact local end minute, actual break minutes and optional notes. Actual duration is derived without payroll rounding.

Completed shifts can be corrected through the same exact-minute picker. Creation, completion, edits, soft deletion and running-clock cancellation atomically append immutable before/after audit evidence under the current `local_employer` desktop actor. Delete hides a completed row from normal use without removing its evidence. The actor field is designed for future authenticated identities, but accounts, roles and web/mobile access are not implemented. Completed, non-deleted shifts feed Payroll Timesheet Preparation alongside effective imported evidence; running shifts are excluded. See [Payroll evidence](PAYROLL-EVIDENCE.md).

### Payroll schedules and rollover

Importing a provider Payroll Prep Sheet is the only payroll-year creation trigger. PDF text extraction is implemented. DOCX is recognised as a file type but its import path currently returns a clear not-implemented error. Import validates:

- a payroll year such as `2027/28`, whose suffix is the following calendar year;
- exactly 13 cycles;
- unique, strictly chronological first-week dates exactly 28 days apart; and
- parseable posting/pay dates with the provider-supported relationship to the cycle start.

One year's replacement is transactional and does not remove other years. Each schedule row has a `payslips_sent` flag: a safely re-imported cycle with unchanged material dates preserves that flag, while a materially changed cycle is inserted with sent state reset when the re-import is otherwise allowed. A changed re-import is refused when downstream payroll history would make rewriting it unsafe. Multiple years can therefore coexist, and importing a future year does not make it current.

The authoritative resolver searches every imported schedule for a date within the inclusive four-week window from `first_week_commencing` through day 27. No match and overlapping matches are errors; the application does not guess from the calendar year. The preceding-cycle resolver is chronological and requires an exact 28-day boundary, so cycle 1 can use cycle 13 from the preceding payroll year.

### Payroll-period selections

There are deliberately separate selections:

- The Dashboard operational payroll period controls preparation, generation and period-associated email operations. It defaults to the schedule containing today, or the newest imported schedule when none contains today. Historical or future choices are explicit and can be reset with **Use current payroll period**.
- Import Payroll Documents validates and classifies an individual file or ZIP first; a separate period chooser is used only for unresolved ordinary payslips with applicable stored candidates. ZIP safety checks and independently staged entries allow partial success with per-entry failures and recovery details.
- View Payroll Schedule has a payroll-year selector and is display-only.

Normal UI labels use Payroll Week, date range and pay date. The database still uses `cycle_number` 1–13 as part of schedule identity; it is not the user-facing payroll week.

### Payroll Timesheet Preparation

Preparation is bound to the complete selected schedule and remembers payroll year, internal cycle, first-week date and pay date. Its PA list includes inclusive Start/Leaving date overlaps regardless of current status, plus any PA with an existing selected-period payroll record, even outside those dates.

Unsent records and generated candidates are editable. Submitted and indeterminate records load their persisted timesheet, week and public-holiday values read-only, without reconciliation or load-time writes. Saving rechecks the operational selection and the schedule facts; a changed selection, missing schedule or materially re-imported schedule requires the screen to be reloaded.

Worked Hours remains editable. Imported shifts are not changed: the persistent manual adjustment is the difference between imported/reconciled minutes and the employer's final value, with an optional reason. Dated annual leave and public-holiday rows remain preparation values. Sick / SSP opens structured sickness-date records; mileage opens a PA-enabled weekly editor. Submitted/settled protections apply; see [Domain rules](DOMAIN.md#sickness-and-weekly-mileage).

If editable reconciliation or a save changes data represented by an existing generated candidate, the candidate is invalidated before preparation writes. Merely viewing an unchanged candidate does not invalidate it. A failed invalidation aborts the associated mutation.

### Effective-dated values

For each positive imported shift, the authoritative pay rate is the newest PA rate effective on or before the shift's start calendar date. Stored `DD/MM/YYYY` dates are parsed as dates; equal effective dates are ordered deterministically by newest row ID. Base rate plus employer top-up forms the applicable total. Future rates never apply early, and generation fails clearly if worked time has no effective rate.

Rate allocation is retained internally by integer minutes. A cycle or week may contain several rates. Shifts are assigned wholly to their start date; the real workflow does not contain midnight-crossing shifts. A positive manual adjustment uses the higher/newer rate genuinely applicable within that week; a negative adjustment uses the lower/older rate; a one-rate week uses that rate. These allocations are snapshot/accounting data and are not printed on the provider PDF.

Contracted weekly hours are informational. Each PDF week resolves the newest contracted-hours record effective on or before that week's commencing date. Future records are excluded and a week before the first record is shown as unavailable. A single value retains the original header; differing values use a compact week-labelled header summary. Contracted hours do not change worked hours or pay.

### Previous-cycle adjustments and snapshots

Outstanding dated work can cross multiple periods, retaining its actual dates and historical rates. Submitted membership reserves evidence; definitive per-PA ordinary-payslip delivery establishes settlement. Uncertain historical payment requires audited paid/unpaid review; signed corrections preserve discrepancies without making payable hours negative. See [Payroll evidence](PAYROLL-EVIDENCE.md).

Schema-18 positive previous-cycle aggregates that lack historical membership are carried forward as explicitly opaque legacy items. Their minutes remain in totals, but work date, entry ID and rate fields stay `NULL`; the PDF shows only the compact previous-cycle information and never fabricates historical precision. The first schema-19 submission establishes exact membership for later detection.

### PDF and snapshot safety

The PDF displays reconciled weekly Hours Worked totals using configured font roles. Pay rates and allocations remain internal. Leave/public-holiday totals, structured sickness dates and stored mileage appear on the provider form. Signed corrections are already included in bold totals; the subordinate `(Info only +/-X.XX hours)` line is never added again. Configurable footer instructions are described in [Domain rules](DOMAIN.md#payroll-pdf).

Generation writes a temporary PDF, persists a candidate snapshot and SHA-256 digest, then publishes the final PDF. Required output parents are created only when writing. A generic PDF root remains flat; an already year-suffixed root such as `2026 to 2027` rolls over to a sibling year directory.

Only a successful production timesheet send freezes the candidate as the immutable submitted baseline. Preview and test email do not. Production verifies the exact approved document identity, path, digest, bytes, routing and classification. Schema36 records each attempt before SMTP. Explicit non-acceptance restores candidate eligibility; ambiguous SMTP errors, interrupted attempts and failed finalisation remain protected until an audited review. Accepted resends refer to the existing submission and do not reapply corrections. Production payslip delivery likewise writes durable per-PA indeterminate state before SMTP, restores unsent only after a reported SMTP failure, and records definitive sent state after successful transport. A crash or failed final status write leaves the payslip visibly uncertain and blocks automatic resend; payslip/supplement recovery remains an explicit future workflow. Timesheet review is available in Dashboard delivery-attempt history.

Generation and production timesheet email use the same selected-period employment eligibility as preparation. Payslip/document email additionally includes PAs with unsent imported P60/P45, independent of employment overlap.

### Email and Payroll Returns

Timesheet preview and test operations require the operational period; P60/P45-only emails can operate without one. Test messages use configured test recipients and test markers rather than production recipient routing.

Email Payslips first lists all unknown P60/P45 histories, identifying PA, type, filename and year. Each document offers Already delivered externally or Needs sending; decisions persist immediately without changing `sent_at`. Unknown documents remain excluded if the user continues, and Cancel sends nothing while retaining saved decisions. New imports also start unknown. It then asks about additional notes and shows individual PA checkboxes using the existing production selection controls. Available recipients are initially selected; Select all available and Clear selection are supported. Rows show unsent document types, including P45/P60 supplements. A separate final confirmation lists the selected PAs and count; an empty selection cannot send. Only selected recipients are dispatched and receive their chosen notes. Existing sent/indeterminate protections remain in force.

Production timesheets retain To payroll department, CC employer and BCC PA when available. Production payslips use a dedicated composer: To the selected PA, BCC the employer, no CC or payroll department recipient. Missing/invalid PA addresses are unavailable. One message contains that PA's eligible bundle. Ordinary payslip subjects are `Payslip for Week <PAYE week>` from the captured schedule, independent of the timesheet subject setting; configured payslip body, notes and signature are retained. Payslip test email uses only the configured PA test address.

A production email batch captures the optional selected schedule key and material facts when it starts; timesheets require a schedule, standalone documents do not. Confirmation displays the Payroll Week/date/pay-date label and warns for historical or future periods. Before dispatch, the schedule and global selection are revalidated; changed, missing or materially re-imported schedules are refused. Ordinary cycle attachments and status use captured schedule facts; P60/P45 use their own document identities. Standalone wording never borrows the Dashboard period. Cancel sends nothing and changes no status or snapshot.

Import Payroll Documents handles prep sheets, ordinary payslips, PA-specific P60/P45 supplements and shared P30/general information together. Ordinary payslips resolve exact stored periods, require an applicable choice when ambiguous, or become unassociated PA archives when no period applies. Safe case/separator-normalised token matching must resolve exactly one PA. ZIP members are checked for unsafe paths, special/encrypted members, count/size/ratio limits and conflicts; valid members can succeed while failures are reported. P30/general information is stored in the information area and is never attached to PA payroll email. See [Import/storage details](ARCHITECTURE.md#payroll-document-import-and-delivery) and [Email rules](DOMAIN.md#email).

### PAYE filenames and year folders

The PAYE week is calculated from `PayrollSchedule.pay_date`: the tax year starts on 6 April of the pay date's year, or the previous year when the pay date precedes 6 April; week is `floor(days / 7) + 1`, including week 53 when applicable. The period code remains `YYYYMMwWW`, where `YYYYMM` comes from the first week commencing date.

Year folders use `YYYY to YYYY`. If a configured root already ends with any year suffix in that form, its parent is treated as the reusable base before the selected schedule's year is appended. Associated payslips use the schedule year. Other documents use their own explicit filename year; yearless information uses the configured information root. Schedule-derived directories are created on actual import/write, not selection/viewing; saving configuration also ensures business roots exist.

### Backup and restore

Application Settings can create timestamped backups under `<data-root>/backups/YYYYMMDD-HHMMSS/` containing `database.sqlite`, optional `config.toml` and `README.txt`. SQLite's online backup API creates a consistent snapshot of the live connection.

Restore accepts only recognised backup directories beneath that backup root. It checks identity, paths/symlinks, required files, schema and read-only opening, and requires exactly one `PRAGMA integrity_check` result equal to `ok` case-insensitively. A mandatory safety backup is made first. SQLite's backup API restores into the live database safely; config is restored only when present. Success requires application restart so in-memory state is not used after restoration. There is no automatic scheduling, retention/deletion, compression or cloud backup.

## Tests and current limitations

The test suite uses temporary/in-memory databases and temporary filesystem roots. It covers migrations, repositories, schedule rollover/resolution, PAYE naming, year paths, preparation/snapshot safety, PDF data, email-period binding, returns, backups and effective-dated values.

Known limitations and deliberately deferred work include:

- no overtime calculation despite the persisted setting;
- persisted frequency, workweek and overtime choices are not applied as downstream configurable calculation rules;
- no implemented DOCX Payroll Prep Sheet import;
- no production consumer for the configured `email_archive` path;
- no arbitrary-file restore or automated backup retention/scheduling;
- public-holiday weekly aggregate hours and individual holiday rows remain distinct existing representations; and
- see [README limitations](../README.md#current-limitations) for packaging, access, annual-leave algorithms, CSV provenance, archival-payslip promotion and indeterminate-delivery recovery boundaries.


### Payroll filing and explicit replacements

Schema35 implements deliberate corrected-document replacement and complete safely identified PA document filing. Maintenance offers **Replace payroll document…** with identity/hash/history review and confirmation; old bytes/evidence remain retained. Corrected ordinary payslips are current and unsent; corrected supplements start unknown with NULL sent_at and link to the superseding registration. P45/P60 remain independent. MIME names for ordinary/P45/P60 attachments use canonical names built from structured identities; hash/revision basenames remain private. Active-to-Inactive transitions file registered/revised payroll documents and generated/historical on-disk timesheets directly into shared year-level Archived folders with no PA subfolder, updating durable readers/paths. Late documents for inactive PAs go straight to Archived. Repeated inactive saves only retry existing cleanup; reactivation never restores files. Shared information/CSV/P30 and external assets are excluded. The schema34 journal retains verified move pairs until post-commit source cleanup succeeds. [Implementation and platform limits](ARCHITECTURE.md#inactive-pa-filing-and-registered-supplement-repair).


## Stage 1 timesheet resend prevention (1.0.6/schema36)

Current unsent PDF versions default selected, including regenerated corrections
whose older versions were sent. Sent current versions remain visible unticked;
Select all unsent excludes them. Intentional manual resends require explicit
acknowledgement and final confirmation separating first sends/resends and actual
Payroll/employer/optional PA routing. Changing selection or approved document
invalidates acknowledgement. Each attempt has a durable intent, document/submission
association, recipients, Message-ID and transport evidence. Accepted replay skips
rather than sends; partial-batch successes stay unticked in a new selection.
Uncertainty requires documented review with a separate confirmation. Reviewed
original outcomes and submission evidence remain retained. Schema36 guards reject
ordinary old-connection SQL writes, but published older executables can still
replace upgraded data through restore; keep them away from production data.

Startup upgrades now create and verify a WAL-safe database/configuration recovery
snapshot, migrate the full chain in isolation and install transactionally. Restore
accepts supported legacy backups through isolated migration, blocks uncertainty,
invalidates stale approval and requires a documented destructive rollback decision.
A separately verified original snapshot retains newer delivery/payroll evidence.
External business files are inventoried only and need independent protection.
See DATABASE-SCHEMA.md for recovery limits and maintenance locking.


## Stage 2 sickness protection (1.0.6/schema37)

Implemented transactional cross-cycle sickness review, reasons/authorisation, exact duplicate rejection, stale-editor/candidate safeguards, immutable document/attempt/submission date evidence and scoped preservation of protected dates. Authorised sickness-only corrections can generate separately named historical PDFs for submitted or settled cycles, preserving settlement, worked totals, original submission/PDF and historical evidence. Corrected dates use the existing weekly cells; a dynamically numbered Sickness Information Correction footnote outside the table asks Payroll to assess financial implications without SSP calculation. Uncertain delivery blocks corrections until audited resolution. Schema37 upgrades use verified pre-migration database/config recovery and isolated installation; missing legacy date snapshots are not fabricated. Version remains 1.0.6; published releases remain unchanged.


Stage 2 final-review corrections close the retained-view transfer gap: source/destination ranges are fully reviewed, safe transfers persist a fresh active identity, and conflicts/uncertainty refuse without partial changes. Historical corrections bind original financial evidence and validated carry-forward; preparation indicators refresh after saves. Genuine populated schema35/schema36 upgrade/restore and full PA archive/reactivation regressions extend recovery coverage. No schema/version increase is required.


## Stage 4 shift change protection (1.0.6/schema38)

Current development remains version 1.0.6 and now uses additive schema38. Semantic CSV headers support the verified Hours Keeper format and existing synthetic aliases regardless of column order. Content identity detects changed same-path exports. Shared review protects imported/internal counterparts and cross-midnight overlaps; original evidence, archival bytes and omitted rows remain retained. Review supports a payable-version choice, explicit separate shifts and audited deferral. New protected edits require prospective authorisation and the existing contextual financial correction workflow; pre-existing obligations are preserved. The established verified database/config backup and isolated upgrade installation remain mandatory; external business files need separate protection.


## Stage 5 startup reliability (1.0.6/schema38)

Structured redacted startup errors, independent native dialogs with stderr fallback and nonzero failure exits are implemented. Configuration creation is atomic/concurrency-safe; explicit saves are atomic and do not require unrelated business folders. Windows Documents discovery no longer panics during loading. Background folder warnings preserve Settings access. Current-schema checks are bounded, fresh initialisation stages before transactional installation, and duplicate database initialisation is removed. Existing verified backup, WAL, locking and recovery safeguards remain intact. Native platform presentation is source-reviewed and mock-tested; no production application launch or platform GUI rehearsal is part of automated validation.
