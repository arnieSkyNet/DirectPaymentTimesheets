# DirectPaymentTimesheets conceptual data model

## Purpose

This document describes the implemented business entities and their relationships. It is intentionally conceptual; [DATABASE-SCHEMA.md](DATABASE-SCHEMA.md) is the exact schema36 table/column reference and the Rust source remains authoritative.

The model preserves imported facts, effective-dated employment terms, prepared payroll values and the exact worked-item evidence represented by a generated or submitted timesheet.

## Employer

The employer is the Direct Payment holder. The record contains identity/contact information, payroll reference details, employer and email signatures, PDF-template path and legacy stored sick-pay/mileage booleans. Those employer flags have no current runtime feature-gating UI or consumer. The legacy database postcode column is not mapped by the current Employer model/editor/repository; the editor uses one multiline address string.

Employer information supplies the generated form declaration and the sender/copy route for payroll email. Older provider-related columns remain in the table for compatibility; current provider maintenance uses the separate payroll-provider entity.

## Payroll provider and payroll configuration

The payroll-provider record contains provider identity/contact information and the payroll-department email address.

Most payroll/application settings are TOML configuration. TOML covers folder paths, PDF fonts/sizes, payroll frequency/rounding/workweek/overtime choices, email templates, SMTP/test addresses and theme. The singleton SQLite `annual_leave_settings` table is an exception. Persisted frequency, workweek and overtime choices are not downstream configurable calculation rules. Payable allocation does apply configured rounding (default 15 minutes Up) to selected source durations while preserving raw evidence; see [Payable rounding](PAYROLL-EVIDENCE.md#payable-rounding).

## Personal Assistant

A Personal Assistant (PA) record contains identity/contact data, employment status, Start/Leaving dates, signature path and PA mileage enablement. PA sickness enablement has been removed from the model/UI; its legacy database column remains unused.

Payroll-period eligibility uses inclusive Start/Leaving overlap independent of current status, with missing boundaries unbounded. Existing selected-period records remain accessible outside those dates. Preparation, generation and timesheet email share this scope; payslip/document email additionally includes unsent P60/P45 recipients.

Deletion is refused when dependent imported or directly recorded work, rate/contracted-hours history, payroll records, holiday details or email statuses exist.

## Effective-dated pay rate

Each PA can have many pay-rate records containing:

- effective date (`DD/MM/YYYY`);
- base hourly rate;
- employer top-up rate; and
- creation timestamp.

For an imported shift, the authoritative rate is the newest record effective on or before the shift start calendar date. Equal effective dates resolve by highest row ID. Future rates remain stored and visible but never apply early. The allocated total is base plus top-up. Positive work with no effective rate prevents PDF generation.

## Effective-dated contracted hours

Each PA can have many contracted-hours records containing an effective date, `hours_basis` (Contracted or Variable), a textual contracted-hours value and creation timestamp. Variable rows store no weekly-hours value.

Each payroll week independently selects the newest record effective on or before that week's commencing date, with highest ID as the equal-date tie-break. Contracted hours supply PDF information and read-only annual-leave guidance; they do not alter worked hours, allocation or pay. Guidance applies basis changes on actual dates; PDF headers resolve each week commencing date.

## Imported TimesheetEntry

An imported timesheet row preserves:

- stable ID;
- source PA name and optional resolved PA ID;
- source start/end values;
- break and worked minutes;
- imported hourly rate and amount; and
- optional notes.

`worked_minutes` is parsed directly from the CSV Worked Hours field, not recalculated from start/end or altered by the configured rounding setting. Imported rate and amount are retained but are not authoritative for generated payroll.

New imports are preflighted as a complete file. PA names are matched case-insensitively after collapsing whitespace and must resolve to exactly one maintained PA. An incoming row that is materially identical to an existing immutable shift is counted and skipped; conflicting same-start and within-file candidates remain stored for audited duplicate resolution. Existing imported rows are never replaced or merged. The stable row ID is later used as submitted-snapshot membership evidence.

An imported row can now have an append-only correction history for start, end, break minutes, worked minutes and notes. Each event records actor/action/time, optional reason and complete before/after effective values. The raw `TimesheetEntry`, PA identity, imported rate and imported amount remain immutable. Reversion is another event, and an identical effective proposal creates no event. Explicit repository APIs distinguish raw evidence from a latest-event effective projection.

Payroll preparation and generation consume effective imported corrections alongside completed direct shifts through shared evidence reconciliation. Raw import identity and View Imported Hours remain unchanged.

## Import audit and archive

An import-audit record captures import time, original/archive filenames, row counts, status and optional error. Validated source bytes are written first to a unique, non-overwriting file in `archive/YYYY/MM/`; all imported rows and the SUCCESS audit are then committed in one SQLite transaction. A database failure leaves the archive as reported recoverable evidence and rolls back all rows. Failed/refused audits are best-effort.

The current schema36 still has no CSV imported-file content hash or row-to-import relationship. Historical successful-import detection therefore remains based on the original pathname: changed content at an already-successful pathname is conservatively skipped, while renamed identical content is preflighted again and exact already-stored raw rows are skipped. Correction events do not change raw collision identity. Durable content identity and row provenance require a future migration.

## DirectShift

A direct shift is separate application-created source evidence linked to one maintained PA. It stores an exact local start minute, nullable exact end minute, actual break minutes, optional notes, a fixed direct-source marker, creation/update metadata and nullable soft-deletion actor/time. A nullable end is a durable running clock-in. At most one non-deleted direct shift may run per PA.

Completed actual worked minutes are derived from end minus start minus break and are never payroll-rounded. Completed rows can be corrected, but every mutation and soft deletion atomically appends an immutable audit row containing actor, action, time and the relevant before/after snapshots. The current actor is `local_employer`; the text identity can later hold authenticated actors without claiming that authentication exists today. Cancelling an accidental running clock-in retains creation/cancellation history even though its current row is removed.

Completed, non-deleted direct shifts feed shared payroll reconciliation and audited duplicate resolution alongside effective imported evidence. Running shifts are excluded; direct rows are never copied into `timesheets`.

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
- legacy `sick_leave_hours` (no longer the sickness entry/PDF source);
- public-holiday hours; and
- travel miles.

The final Hours Worked value can be edited. Imported work remains unchanged; the difference between the imported/reconciled baseline and the employer's final value is persisted as a manual adjustment.

## Sickness, leave and document entities

`personal_assistant_sickness_periods` stores PA-owned inclusive ISO start/end dates independently of weeks. Overlapping weeks display the same record; editing is protected for submitted/settled payroll and PDF projection shows dates. DirectPaymentTimesheets records sickness dates; Payroll calculates SSP.

`payroll_timesheet_annual_leave` stores dated weekly leave details; weekly aggregates remain for compatibility. `annual_leave_settings` stores the two recurring rule boundaries and current statutory-weeks/accrual-percentage values. Derived leave guidance is not persisted; see [Annual-leave guidance](DOMAIN.md#annual-leave-guidance).

`imported_payroll_documents` holds current and superseded P60/P45 document IDs, PA identity, path, SHA-256 digest, optional own tax year, `history_state`, `sent_at` and an optional replacement link. Its history declaration (`unknown`, `external`, `needs_sending`, `application`) records how delivery history is understood; actual application delivery evidence remains in nullable `sent_at`, including the protected `indeterminate:` marker. Only `needs_sending` with NULL `sent_at` is selectable for production. Corrected supplements start with unknown history and NULL sent_at; older registrations and evidence remain. Ordinary unassociated archival payslips and P30/general information remain outside this table. P60/P45 sent state does not settle payroll or complete a schedule.

Schema28 duplicate decisions, immutable submissions and signed correction components extend the payroll-timesheet snapshot slot. Schema35 ordinary payslip revisions are a separate corrected-document lifecycle and do not change those submission snapshots. See [Payroll evidence](PAYROLL-EVIDENCE.md) for relationships and lifecycle.

Schema35 also adds `payslip_revisions`, keyed by PA/year/cycle with immutable stored path, SHA-256, `sent_at` and `is_current`. A replacement creates a new current unsent revision and retains the old bytes and old delivery marker; current delivery updates the current revision alongside ordinary cycle status. In MIME email, ordinary payslip, P45 and P60 attachments use canonical recipient-visible names derived from PA and structured schedule/document identity; revision/hash storage basenames remain internal.

## Public-holiday detail

Public-holiday detail rows belong to a payroll timesheet and identify a week, holiday date and editable hours. Preparation creates the implemented England/Wales bank-holiday dates for editable records.

The individual holiday rows and the weekly aggregate `public_holiday_hours` remain distinct existing representations. The aggregate supplies the PDF hours value; positive detail rows supply displayed holiday-date information.

## Manual worked-hours adjustment

A payroll timesheet can have at most one manual adjustment per week. It stores signed integer minutes, an optional human-readable reason and update timestamp.

Positive manual minutes use the higher/newer rate genuinely applicable during the week; negative minutes use the lower/older applicable rate. A one-rate week uses that rate for either sign. Imported TimesheetEntry rows are never modified.

## Email status

Email status identifies one PA, payroll year, internal cycle and email type (`timesheet` or `payslip`) with an optional sent timestamp. Timesheet and payslip status are independent.

The schedule-level `payslips_sent` flag is separate compatibility/summary state and is marked only after all required selected-period PA ordinary payslips are definitively recorded as sent. Payslip production delivery uses the existing nullable `sent_at` value as a small state machine: null is unsent, a reserved `indeterminate:` attempt marker protects the SMTP uncertainty window, and a normal RFC 3339 timestamp is definitively sent. Indeterminate rows survive restart and block automatic resend.

## Worked-item snapshot

A worked-item snapshot row belongs to one payroll timesheet/week and records the evidence represented by a generated PDF. Depending on source type it contains:

- source TimesheetEntry ID or mutually exclusive direct-shift ID;
- raw `source_evidence`, separate from payable minutes;
- copied work date and integer worked minutes;
- selected pay-rate ID, effective date and total rate;
- optional reason; and
- capture timestamp.

Source types distinguish imported/direct work, outstanding dated work, manual adjustments, signed carry corrections and opaque legacy previous-cycle adjustments. Ordinary items require rate evidence. The explicit legacy source type retains only aggregate minutes because its historical date/rate cannot be reconstructed.

Submitted source membership reserves evidence across periods; definitive ordinary-payslip settlement prevents repeat payment. Historical uncertainty and signed discrepancies follow the [evidence review rules](PAYROLL-EVIDENCE.md). An unsubmitted candidate is not membership evidence.

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
  |-- imported TimesheetEntry rows and DirectShift evidence
  |-- Sickness Periods
  |-- imported Payroll Documents (P60/P45, independent delivery IDs)
  |-- Payroll Timesheet (payroll year + internal cycle)
        |-- four Payroll Timesheet Week rows
        |-- Public Holiday and dated Annual Leave detail rows
        |-- Manual Adjustment rows
        |-- Worked Item Snapshot rows
        |-- one optional Snapshot State
        |-- immutable Submissions and correction applications
  |-- Email Status (period + type)

Payroll Schedule (payroll year + internal cycle)
  |-- supplies the dates/identity used by Payroll Timesheet and Email Status
```

These relationships are implemented through stored IDs/business keys and repository logic. Schemas 30/31 declare PA references for sickness and imported payroll documents; enforcement depends on the connection. See the schema reference for current constraints.

## Historical-data principles

- Imported rows are retained rather than rewritten by preparation corrections.
- Effective-dated rate and contracted-hours histories are preserved, including future entries.
- Schedule replacement is per payroll year and transactional.
- Submitted snapshot membership is immutable.
- Opaque pre-schema-19 aggregates remain explicitly unallocated rather than receiving invented dates or rates.


## Outgoing payroll notes and returned results (schema 32)

Each PA/payroll-period preparation can retain a free-form **Notes for Payroll
Department** string of at most 256 Unicode characters. It is not a permanent PA
attribute or a worked-shift note, and has no calculation meaning. It participates
in preparation Save/Discard/Cancel and submission protections. Each new retained
submission captures its own structured note; older historical submissions retain
NULL rather than an invented note.

The same PA/period record separately holds optional **actual in-lieu hours awarded
by Payroll**, and the timestamp of its last independent save/clear. NULL means the
result is unknown; 0.00 is a confirmed zero award. Only the user records the actual
returned figure. The application never derives entitlement, adds the award to
worked hours, or includes it in outgoing timesheet PDFs. A later manual correction
to this result leaves all outgoing/submitted payroll facts intact. Result storage
is available to a future importer without coupling it to preparation saving.


## Payroll filing lifecycle

Employment status, document filing and delivery history remain separate facts. Importing P45/P60 does not change employment status. An explicit Active-to-Inactive save files safely identified PA-owned documents directly into shared year-level `Archived` directories, without a PA subfolder; late documents for inactive PAs are filed there immediately. Reactivation does not restore files. Registered supplement IDs, hashes and reconciliation/delivery states survive path changes unchanged. `payroll_file_moves` tracks verified original/destination pairs until post-commit cleanup completes. P30 and other shared information, generic CSV archives and external assets are outside PA filing. Unassociated ordinary evidence remains ineligible for automatic delivery. See [filing and repair](ARCHITECTURE.md#inactive-pa-filing-and-registered-supplement-repair).


Explicit corrections preserve superseded supplement registrations through `superseded_by`. Ordinary corrections retain immutable `payslip_revisions`, with one current revision per PA/payroll-year/cycle. Delivery evidence belongs to the bytes/revision it described; corrected supplement history starts unknown. File location changes on deactivation preserve identity, hashes, submission facts and embedded evidence. See the schema35 and architecture sections for transaction and lookup behaviour.


## Timesheet document and delivery identity (schema36)

A generated timesheet has a stable immutable document identity distinct from its
PA/payroll period and file location. Its digest and retained bytes identify the
approved artifact. The current snapshot points to that document; successful first
delivery links it to one payroll submission. Each resend references that existing
submission. Delivery attempts retain intent, recipients, Message-ID, timestamps,
classification and transport outcome. Separate append-only reviews resolve
uncertainty while preserving original evidence. Claimed payroll payloads remain
immutable for post-crash finalisation. These entities do not settle payroll;
ordinary payslip status retains that responsibility.

Byte-identical regeneration with identical represented evidence retains the same
document identity and sent status. Material corrections allocate a new identity.


Current snapshot location may change after successful byte-identical PDF
regeneration; immutable document location and original submission paths remain
provenance. Database/config recovery is distinct from external business-file
recovery. Explicit rollback decisions and exact pre-restore evidence are retained
in a separately verified recovery snapshot; uncertainty cannot be cleared by restore.
