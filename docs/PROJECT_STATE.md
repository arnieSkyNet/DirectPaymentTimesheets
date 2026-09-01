# DirectPaymentTimesheets project state

This document describes the implementation on `main`. Source code, migrations and tests are the authority if this summary becomes stale.

## Current release and platform

- Application version: `0.0.10`.
- Database schema: version 19, upgraded in place by ordered SQLite migrations.
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

The application maintains employer, payroll-provider and Personal Assistant (PA) records. PA maintenance retains pay-rate and contracted-hours histories, including future-dated rows. Themes include system, light/dark variants, blue and high contrast.

Payroll Settings persists payroll frequency, rounding choice, workweek start and overtime enabled. These values are not currently applied as downstream configurable calculation rules: schedule dates define the four payroll weeks, imported worked minutes are not altered by the rounding setting, and no overtime calculation is implemented. Public-holiday handling is permanently active and has no enable/disable setting. Email subject/body templates are configured separately through Email Settings.

### Importing work

CSV import parses provider rows, prevents duplicates and stores stable `TimesheetEntry` identities. Start/end values are retained unchanged, and `worked_minutes` is parsed directly from the CSV worked-duration field rather than calculated from those values or altered using the persisted rounding settings. The imported CSV rate and amount are not authoritative payroll rates.

After a successful import, the source CSV is copied to the internal `archive/YYYY/MM/` hierarchy with a timestamped filename and is not subsequently modified by the application. Successes and failures are recorded in `import_audit`, including row counts, source/archive paths and errors where applicable.

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

- The Dashboard operational payroll period controls preparation/generation and non-production email operations. It defaults to the schedule containing today, or the newest imported schedule when none contains today. Historical or future choices are explicit and can be reset with **Use current payroll period**.
- Payroll Return import has its own schedule-selection dialog, then opens the ZIP chooser.
- View Payroll Schedule has a payroll-year selector and is display-only.

Normal UI labels use Payroll Week, date range and pay date. The database still uses `cycle_number` 1–13 as part of schedule identity; it is not the user-facing payroll week.

### Payroll Timesheet Preparation

Preparation is bound to the complete selected schedule and remembers payroll year, internal cycle, first-week date and pay date. Its PA list is the union of currently active PAs (including legacy `NULL` active state) and PAs that already have a payroll-timesheet record for that selected period. An inactive PA with no such record is not added.

Unsent records and generated candidates are editable. Submitted and indeterminate records load their persisted timesheet, week and public-holiday values read-only, without reconciliation or load-time writes. Saving rechecks the operational selection and the schedule facts; a changed selection, missing schedule or materially re-imported schedule requires the screen to be reloaded.

Worked Hours remains editable. Imported shifts are not changed: the persistent manual adjustment is the difference between imported/reconciled minutes and the employer's final value, with an optional reason. Annual leave, sick/SSP, public-holiday rows and mileage remain editable preparation values.

If editable reconciliation or a save changes data represented by an existing generated candidate, the candidate is invalidated before preparation writes. Merely viewing an unchanged candidate does not invalidate it. A failed invalidation aborts the associated mutation.

### Effective-dated values

For each positive imported shift, the authoritative pay rate is the newest PA rate effective on or before the shift's start calendar date. Stored `DD/MM/YYYY` dates are parsed as dates; equal effective dates are ordered deterministically by newest row ID. Base rate plus employer top-up forms the applicable total. Future rates never apply early, and generation fails clearly if worked time has no effective rate.

Rate allocation is retained internally by integer minutes. A cycle or week may contain several rates. Shifts are assigned wholly to their start date; the real workflow does not contain midnight-crossing shifts. A positive manual adjustment uses the higher/newer rate genuinely applicable within that week; a negative adjustment uses the lower/older rate; a one-rate week uses that rate. These allocations are snapshot/accounting data and are not printed on the provider PDF.

Contracted weekly hours are informational. Each PDF week resolves the newest contracted-hours record effective on or before that week's commencing date. Future records are excluded and a week before the first record is shown as unavailable. A single value retains the original header; differing values use a compact week-labelled header summary. Contracted hours do not change worked hours or pay.

### Previous-cycle adjustments and snapshots

Late imported shifts from previous-cycle weeks three and four are identified against the frozen submitted snapshot's stable TimesheetEntry membership. Their actual historical work dates and effective rates are retained, including when the previous cycle belongs to another payroll year. The compatible aggregate `previous_cycle_hours` display is reconciled from those items.

Schema-18 positive previous-cycle aggregates that lack historical membership are carried forward as explicitly opaque legacy items. Their minutes remain in totals, but work date, entry ID and rate fields stay `NULL`; the PDF shows only the compact previous-cycle information and never fabricates historical precision. The first schema-19 submission establishes exact membership for later detection.

### PDF and snapshot safety

The PDF displays reconciled weekly Hours Worked totals using configured font roles. Pay rates and allocations remain internal. Existing leave, sick/SSP, public-holiday and mileage presentation is retained; positive previous-cycle hours use the compact `[Info.only +… prev]` line.

Generation writes a temporary PDF, persists a candidate snapshot and SHA-256 digest, then publishes the final PDF. Required output parents are created only when writing. A generic PDF root remains flat; an already year-suffixed root such as `2026 to 2027` rolls over to a sibling year directory.

Only a successful production timesheet send freezes the candidate as the immutable submitted baseline. Preview and test email do not. Production verifies the exact candidate path and digest. SMTP failure leaves the candidate replaceable. If transport may have succeeded but persisting the submitted state fails, the record enters protected indeterminate state to prevent an unsafe automatic resend or regeneration. Production payslip delivery likewise writes durable per-PA indeterminate state before SMTP, restores unsent only after a reported SMTP failure, and records definitive sent state after successful transport. A crash or failed final status write leaves the payslip visibly uncertain and blocks automatic resend; recovery remains an explicit future workflow.

Generation uses the same PA eligibility principle as preparation: active PAs plus inactive PAs with an existing selected-period payroll record. It does not create unrelated historical records for inactive PAs.

That historical inactive-PA eligibility applies to preparation and PDF generation only. Production timesheet and payslip batches currently include active PAs and legacy `NULL`-status PAs, not inactive historical PAs.

### Email and Payroll Returns

Preview and test operations use the operational selection. Test messages use configured test recipients and test markers rather than production recipient routing.

Both production timesheet and payslip messages are sent from the employer address to the payroll department, with the employer copied and the PA blind-copied when an address is available. Payslip test email is different: it is sent to the configured PA test address.

A production email batch captures the selected schedule key and material facts when it starts. Confirmation displays the Payroll Week/date/pay-date label and warns for historical or future periods. Before dispatch, the schedule and global selection are revalidated; changed, missing or materially re-imported schedules are refused. Attachment paths, subject period code and status reads/writes use only the captured schedule. Cancel sends nothing and changes no status or snapshot.

Payroll Return import uses the explicitly selected complete schedule, not today's cycle and not the internal cycle number as a filename week. Payslips are named `Payslip for Week <PAYE-week> for <PA>.pdf`. Other returned payroll information keeps a safe original name where possible and goes under the configured payroll-information folder, with deterministic collision avoidance.

### PAYE filenames and year folders

The PAYE week is calculated from `PayrollSchedule.pay_date`: the tax year starts on 6 April of the pay date's year, or the previous year when the pay date precedes 6 April; week is `floor(days / 7) + 1`, including week 53 when applicable. The period code remains `YYYYMMwWW`, where `YYYYMM` comes from the first week commencing date.

Year folders use `YYYY to YYYY`. If a configured root already ends with any year suffix in that form, its parent is treated as the reusable base before the selected schedule's year is appended. Payslips and payroll information are year-separated. Directories are created on actual import/write, not by schedule import, selection or viewing.

### Backup and restore

Application Settings can create timestamped backups under `<data-root>/backups/YYYYMMDD-HHMMSS/` containing `database.sqlite`, optional `config.toml` and `README.txt`. SQLite's online backup API creates a consistent snapshot of the live connection.

Restore accepts only recognised backup directories beneath that backup root. It checks identity, paths/symlinks, required files, schema and read-only opening, and requires exactly one `PRAGMA integrity_check` result equal to `ok` case-insensitively. A mandatory safety backup is made first. SQLite's backup API restores into the live database safely; config is restored only when present. Success requires application restart so in-memory state is not used after restoration. There is no automatic scheduling, retention/deletion, compression or cloud backup.

## Tests and current limitations

The test suite uses temporary/in-memory databases and temporary filesystem roots. It covers migrations, repositories, schedule rollover/resolution, PAYE naming, year paths, preparation/snapshot safety, PDF data, email-period binding, returns, backups and effective-dated values.

Known limitations and deliberately deferred work include:

- no overtime calculation despite the persisted setting;
- persisted frequency, rounding, workweek and overtime choices are not applied as downstream configurable calculation rules;
- no P60-specific Payroll Return handling;
- no implemented DOCX Payroll Prep Sheet import;
- no production consumer for the configured `email_archive` path;
- no arbitrary-file restore or automated backup retention/scheduling;
- public-holiday weekly aggregate hours and individual holiday rows remain distinct existing representations; and
- generated historical payroll is limited to active PAs or PAs with an existing record for the selected period, while production email batches remain active/legacy-`NULL` only.
