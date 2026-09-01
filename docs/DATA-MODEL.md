# DirectPaymentTimesheets conceptual data model

## Purpose

This document describes the implemented business entities and their relationships. It is intentionally conceptual; [DATABASE-SCHEMA.md](DATABASE-SCHEMA.md) is the exact schema-19 table/column reference and the Rust source remains authoritative.

The model preserves imported facts, effective-dated employment terms, prepared payroll values and the exact worked-item evidence represented by a generated or submitted timesheet.

## Employer

The employer is the Direct Payment holder. The record contains identity/contact information, payroll reference details, employer and email signatures, PDF-template path and sick-pay/mileage capability defaults.

Employer information supplies the generated form declaration and the sender/copy route for payroll email. Older provider-related columns remain in the table for compatibility; current provider maintenance uses the separate payroll-provider entity.

## Payroll provider and payroll configuration

The payroll-provider record contains provider identity/contact information and the payroll-department email address.

Payroll/application settings are TOML configuration rather than payroll database entities. They include folder paths, PDF fonts/sizes, payroll frequency/rounding/workweek/overtime choices, email templates, SMTP/test addresses and theme. Persisted frequency, rounding, workweek and overtime choices are not currently applied as downstream configurable calculation rules.

## Personal Assistant

A Personal Assistant (PA) record contains identity/contact data, employment status, start date, signature path and sick-pay/mileage capabilities.

`NULL` employment status is treated as active for compatibility. Inactive PAs remain valid historical identities. Preparation and generation include an inactive PA only when a payroll-timesheet record already exists for the selected period; production email batches remain active/legacy-`NULL` only.

Deletion is refused when dependent imported work, rate/contracted-hours history, payroll records, holiday details or email statuses exist.

## Effective-dated pay rate

Each PA can have many pay-rate records containing:

- effective date (`DD/MM/YYYY`);
- base hourly rate;
- employer top-up rate; and
- creation timestamp.

For an imported shift, the authoritative rate is the newest record effective on or before the shift start calendar date. Equal effective dates resolve by highest row ID. Future rates remain stored and visible but never apply early. The allocated total is base plus top-up. Positive work with no effective rate prevents PDF generation.

## Effective-dated contracted hours

Each PA can have many contracted-hours records containing an effective date, a textual contracted-hours value and creation timestamp.

Each payroll week independently selects the newest record effective on or before that week's commencing date, with highest ID as the equal-date tie-break. Contracted hours are informational PDF data; they do not alter worked hours, allocation or pay.

## Imported TimesheetEntry

An imported timesheet row preserves:

- stable ID;
- source PA name and optional resolved PA ID;
- source start/end values;
- break and worked minutes;
- imported hourly rate and amount; and
- optional notes.

`worked_minutes` is parsed directly from the CSV Worked Hours field, not recalculated from start/end or altered by the configured rounding setting. Imported rate and amount are retained but are not authoritative for generated payroll.

New imports are preflighted as a complete file. PA names are matched case-insensitively after collapsing whitespace and must resolve to exactly one maintained PA. An incoming row that is materially identical to an existing immutable shift is counted and skipped; a same-PA/start row with any material difference refuses the complete file for explicit review. The same rule applies within one incoming file. Existing imported rows are never replaced or merged. The stable row ID is later used as submitted-snapshot membership evidence.

## Import audit and archive

An import-audit record captures import time, original/archive filenames, row counts, status and optional error. Validated source bytes are written first to a unique, non-overwriting file in `archive/YYYY/MM/`; all imported rows and the SUCCESS audit are then committed in one SQLite transaction. A database failure leaves the archive as reported recoverable evidence and rolls back all rows. Failed/refused audits are best-effort.

Schema 19 has no content hash or row-to-import relationship. Historical successful-import detection therefore remains based on the original pathname: changed content at an already-successful pathname is conservatively skipped, while renamed identical content is preflighted again and may be refused as competing evidence. Durable content identity and row provenance require an explicitly approved future migration.

## Payroll schedule

A Payroll Prep Sheet creates 13 four-week schedule records for one payroll year. Each record contains:

- payroll year and internal cycle number;
- first week commencing;
- latest posting date;
- pay date;
- creation timestamp; and
- schedule-level `payslips_sent` state.

Several payroll years coexist. The operational resolver finds a unique schedule whose inclusive active window is its first-week date through day 27. The chronological predecessor must end exactly where the current period starts, allowing cycle 1 to follow cycle 13 in another payroll year.

The internal cycle number is not the PAYE Payroll Week. PAYE week and file period code are derived from schedule dates.

## Payroll timesheet and weekly preparation

One payroll-timesheet record represents one PA in one payroll year/internal cycle. It holds creation/update timestamps and the compatible aggregate positive previous-cycle hours.

Each payroll timesheet has up to four weekly rows. A weekly row contains its week number/date and prepared values for:

- worked hours;
- annual leave;
- sick leave/SSP;
- public-holiday hours; and
- travel miles.

The final Hours Worked value can be edited. Imported work remains unchanged; the difference between the imported/reconciled baseline and the employer's final value is persisted as a manual adjustment.

## Public-holiday detail

Public-holiday detail rows belong to a payroll timesheet and identify a week, holiday date and editable hours. Preparation creates the implemented England/Wales bank-holiday dates for editable records.

The individual holiday rows and the weekly aggregate `public_holiday_hours` remain distinct existing representations. The aggregate supplies the PDF hours value; positive detail rows supply displayed holiday-date information.

## Manual worked-hours adjustment

A payroll timesheet can have at most one manual adjustment per week. It stores signed integer minutes, an optional human-readable reason and update timestamp.

Positive manual minutes use the higher/newer rate genuinely applicable during the week; negative minutes use the lower/older applicable rate. A one-rate week uses that rate for either sign. Imported TimesheetEntry rows are never modified.

## Email status

Email status identifies one PA, payroll year, internal cycle and email type (`timesheet` or `payslip`) with an optional sent timestamp. Timesheet and payslip status are independent.

The schedule-level `payslips_sent` flag is separate compatibility/summary state and is marked after all currently active/legacy-active PA payslips are recorded as sent.

## Worked-item snapshot

A worked-item snapshot row belongs to one payroll timesheet/week and records the evidence represented by a generated PDF. Depending on source type it contains:

- source TimesheetEntry ID;
- copied work date and integer worked minutes;
- selected pay-rate ID, effective date and total rate;
- optional reason; and
- capture timestamp.

Source types distinguish current imported shifts, late previous-cycle shifts, manual adjustments and opaque legacy previous-cycle adjustments. Ordinary items require rate evidence. The explicit legacy source type retains only aggregate minutes because its historical date/rate cannot be reconstructed.

Submitted raw-shift IDs form the membership baseline for detecting genuinely late shifts in the preceding cycle. An unsubmitted candidate is not membership evidence.

## Snapshot state

Each snapshotted payroll timesheet has one state row containing:

- `candidate`, `submitted` or `indeterminate` state;
- final PDF path and SHA-256 digest;
- generation timestamp; and
- optional submitted/indeterminate timestamps.

A candidate is replaceable until production send. Successful timesheet transport freezes it as submitted. If transport may have succeeded but final database persistence fails, indeterminate state protects against unsafe resend/regeneration.

## Main relationships

```text
Personal Assistant
  |-- Pay Rate history
  |-- Contracted Hours history
  |-- imported TimesheetEntry rows
  |-- Payroll Timesheet (payroll year + internal cycle)
        |-- four Payroll Timesheet Week rows
        |-- Public Holiday detail rows
        |-- Manual Adjustment rows
        |-- Worked Item Snapshot rows
        |-- one optional Snapshot State
  |-- Email Status (period + type)

Payroll Schedule (payroll year + internal cycle)
  |-- supplies the dates/identity used by Payroll Timesheet and Email Status
```

These relationships are implemented through stored IDs/business keys and repository logic. Schema 19 does not declare SQLite foreign-key constraints; see the schema reference for the actual constraints.

## Historical-data principles

- Imported rows are retained rather than rewritten by preparation corrections.
- Effective-dated rate and contracted-hours histories are preserved, including future entries.
- Schedule replacement is per payroll year and transactional.
- Submitted snapshot membership is immutable.
- Opaque pre-schema-19 aggregates remain explicitly unallocated rather than receiving invented dates or rates.
