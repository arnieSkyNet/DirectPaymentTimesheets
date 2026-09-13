# DirectPaymentTimesheets domain guide

## Purpose and terminology

DirectPaymentTimesheets supports an employer who uses a UK direct payment to employ Personal Assistants (PAs) and submit information to an external payroll provider.

Use **timesheet** as one word. In user-facing payroll text, identify a period by payroll year, PAYE Payroll Week, four-week date range and scheduled pay date. `cycle_number` is an internal 1–13 database identity and is not the PAYE week printed in filenames.

## People and maintenance records

The employer record supplies identity/contact details, email routing and the declaration/signature information used on generated documents. The payroll-provider record supplies provider contact details. A PA has identity/contact details, an active state and effective-dated pay-rate and contracted-hours histories.

Payroll-period PA eligibility uses inclusive Start/Leaving date overlap, independent of current Active/Inactive status. Missing boundaries remain unbounded. Existing selected-period payroll records remain included outside those dates. Preparation, generation and production timesheet email share this scope; payslip/document email additionally includes PAs with unsent imported P60/P45.

## Imported work

A TimesheetEntry is an imported work interval associated with a PA. Its stable database ID is evidence used by submitted payroll snapshots and late-entry detection. Import preserves the source start/end values and parses worked minutes directly from the CSV worked-duration field; it does not recalculate them from start/end values or apply the persisted rounding setting. Duplicate detection prevents re-importing the same work as another ordinary row.

After a successful import, the source CSV is copied unchanged to the internal `archive/YYYY/MM/` hierarchy with a timestamped filename. `import_audit` records successful and failed imports and their source/archive details.

The external file's hourly-rate and amount values are not authoritative for payroll. The application resolves employer-maintained rate history instead.

Schema 21 introduced an append-only correction-event layer for the effective start, end, break, worked minutes and notes of an imported row. Every event retains complete before/after values, actor, action time and optional reason; reverting is another event. The archived CSV and original `timesheets` row remain immutable, including PA identity, rate and amount. Worked minutes remain independent source evidence and are not recalculated from clocks or break.

Payroll evidence now projects these audited corrections for calculation, while import repeat detection and the imported-hours view retain the original raw source.

The established duration rule assigns an entire shift to its start calendar date; real project shifts do not cross midnight. Imported clock values are not rewritten by preparation corrections.

## Directly recorded work

A DirectShift is application-created source evidence stored independently from imported `TimesheetEntry` rows. It records the maintained PA, exact local start and optional end date/time to the minute, actual break minutes, an optional note, a fixed `direct` source marker and creation/update timestamps. A `NULL` end represents a durable running shift and survives application restart.

Direct shift evidence represents what actually happened. Payroll allocation derives payable time and rates from it, but must never rewrite evidence as a side effect. Deliberate completed-shift corrections update the current record only while atomically appending immutable before/after audit evidence. Delete is a soft deletion: deleted rows remain stored and audited but are excluded from normal use. Cancelling an accidental running clock-in removes its current row only after recording a cancellation audit snapshot.

Every mutation records action type/time and actor. The desktop actor is currently the stable `local_employer` identity; the text actor field is intentionally suitable for future authenticated identities, but authentication, accounts and web/mobile access do not yet exist. Actual worked minutes are derived as end minus start minus break, without payroll rounding. Completed, non-deleted direct shifts feed the shared payroll evidence reconciliation and duplicate resolver, without being copied to imported rows.

## Payroll years, periods and Payroll Week

A provider Payroll Prep Sheet defines a payroll year and its 13 consecutive four-week cycles. Importing this sheet—not a manual “new year” command—creates or refreshes the year. PDF import is implemented; DOCX is recognised but not implemented. Several years may coexist, including a future year imported months early.

Each cycle also retains a schedule-level `payslips_sent` flag. Re-import with unchanged material dates preserves it; a materially changed cycle receives reset sent state when replacement is permitted, and changed dates are refused once prepared payroll history makes replacement unsafe.

A cycle is operationally active for dates from its first week commencing through 27 days later, inclusive. The current schedule is resolved from these imported date windows, not from a hard-coded year or 1 April assumption. A future sheet remains storage-only until its dates apply or the user deliberately selects one of its periods.

The PAYE Payroll Week used for files is derived from the schedule pay date and the tax year beginning 6 April:

```text
PAYE week = floor((pay date - applicable 6 April) / 7) + 1
```

This may differ from internal cycle 1–13. For example, the 2026/27 schedule beginning 10/08/2026 with pay date 04/09/2026 uses Payroll Week 22 and period code `202608w22`.

## Operational versus viewing/import selections

The operational payroll-period selection drives preparation, generation, preview/test email and displayed statuses. Today is the convenient default, but a historical or future selection is deliberate and visible.

Import Payroll Documents selects/classifies an individual file or ZIP first and has an independent period chooser only for unresolved ordinary payslips with applicable candidates. View Payroll Schedule independently selects a payroll year because viewing a future schedule must not alter operational work. Production email batches freeze the operational period that was selected when confirmation began.

## Worked hours and manual corrections

Ordinary worked hours come from selected effective imported corrections and completed internal shifts, reconciled into the four schedule weeks with outstanding work and corrections. Payroll Timesheet Preparation permits the employer to set the final Hours Worked value when an external record is missing or wrong.

The application persists the correction as integer minutes separate from imported rows:

```text
manual adjustment = final prepared worked minutes - reconciled evidence baseline minutes
```

An optional reason can accompany it. Positive and negative adjustments are preserved when the preparation is reloaded and in the submitted snapshot.

Dated annual leave, public-holiday rows and weekly mileage are preparation values; sickness uses separate structured dates. Contracted weekly hours supply PDF information and annual-leave guidance without altering worked hours or pay.

Payable source allocation applies the configured increment/direction, default 15 minutes Up, after source break handling. Raw evidence remains exact; manual adjustments and corrections are not rounded again. Submitted/settled history is not recalculated when settings change. See [Payable rounding](PAYROLL-EVIDENCE.md#payable-rounding).

## Sickness and weekly mileage

Sick / SSP opens PA-owned `personal_assistant_sickness_periods` records with inclusive start/end dates. The same period is visible across overlapping weeks; add/edit/delete and date validation preserve shared identity. Submitted/settled payroll is protected, and changes invalidate affected editable PDF candidates. PDF projection prints full sickness dates and refuses overflow rather than omitting dates. DirectPaymentTimesheets records sickness dates; Payroll calculates SSP. Neither employer nor former PA sickness enablement gates this workflow. Weekly `sick_leave_hours` remains legacy data, not current sickness entry.

Travel Miles opens a weekly editor only when the PA's Enable mileage setting permits it. Finite, non-negative miles save immediately, invalidate stale candidates and respect protected payroll. Employer mileage flags are unused. Disabling PA mileage does not erase previously stored miles; PDFs continue to read stored values.

## Annual-leave guidance

PA Maintenance shows saved-data, read-only guidance; Payroll remains definitive. Effective-dated Contracted/Variable Hours Basis and inclusive employment dates apply within a fixed April–March leave year. Contracted entitlement uses applicable weekly hours and statutory weeks; Variable accrual uses qualifying retained work, gated by the PA/cycle's definitively Sent ordinary payslip, rounded once per cycle to whole hours. Safe historical weeks can fall back to retained worked totals when submitted item evidence is unavailable and attribution is unambiguous; unsafe evidence is excluded or flagged for review.

Taken leave uses actual dated entries, or week commencing for legacy undated totals, never both. Negative remaining is retained. Display figures round independently up to quarter-hours while Details retains precision; this is separate from payable-work rounding. Current singleton SQLite settings have no historical numeric rule values. Structured sickness is not part of the accrual algorithm: the current sickness warning checks legacy weekly `sick_leave_hours`, and the sickness reference-period/accrual calculation remains unimplemented. See [guidance details](DEVELOPMENT.md#annual-leave-guidance-schema-27-unchanged).

## Effective-dated pay rates

The rate for an imported shift is the newest PA pay-rate record whose effective date is on or before the shift start date. Base rate and employer top-up are added. Future rows remain visible in maintenance but never apply early. Equal effective dates resolve deterministically by the newest database ID.

A payroll week may cross a rate boundary, including part-way through the week. Imported minutes retain their actual shift date and are allocated to the appropriate rates internally. Positive manual hours use the higher/newer rate applicable somewhere within that week, favouring the PA; negative manual hours use the lower/older applicable rate. Rates outside that week, including future rates, are not used. A positive worked item with no effective rate prevents generation rather than silently using zero or a future rate.

Rate portions are internal accounting/snapshot evidence. The payroll-provider PDF prints only the reconciled Hours Worked total.

## Effective-dated contracted hours

Each of the four payroll weeks independently uses the newest contracted-hours record effective on or before that week's commencing date. Equal dates use the newest ID, future rows are excluded, and a week before the first record is unavailable.

When all weeks share a value, the PDF retains `Contracted Weekly Hours: VALUE`. When values differ, a compact header summary associates values (or unavailable state) with week-commencing dates. No midweek contracted-hours rule is applied.

## Previous-cycle work

Dated unpaid work remains outstanding across periods, with actual work dates and historical rates. Submitted membership reserves evidence and definitive per-PA payslip Sent state establishes settlement. Aggregate-only historical payment uncertainty requires an explicit paid/unpaid review. Separate signed corrections preserve estimate and actual-evidence differences; negative aggregate estimates require completeness confirmation, and negative applications cannot reduce worked hours below zero. Detailed rules and review gates are in [PAYROLL-EVIDENCE.md](PAYROLL-EVIDENCE.md).

## Preparation and submission states

A selected-period payroll record can be:

- **Unsent/no snapshot**: editable preparation with no generated candidate.
- **Candidate**: editable preparation paired with a generated PDF, worked-item snapshot and SHA-256 digest.
- **Submitted**: immutable baseline representing the production PDF successfully sent.
- **Indeterminate**: protected state where SMTP may have succeeded but recording submission failed.

Submitted and indeterminate records are read-only in preparation. Changing represented data for a candidate invalidates it before the change is persisted, requiring regeneration. Opening unchanged data does not invalidate it. Stale-bound screens cannot save after the operational selection or material schedule dates change.

## Public holidays

Public-holiday handling is always active. Payroll preparation calculates the implemented England/Wales bank-holiday dates for the four-week period and stores individual holiday rows that can be edited. Weekly payroll rows also contain the established public-holiday-hours aggregate used in the PDF, while individual rows supply holiday-date information. These are existing distinct representations; they have not been redesigned into a single model.

There is no `public_holiday_enabled` business rule. An obsolete config key is ignored for compatibility.

## Payroll PDF

The provider PDF retains its established table and configured typography. Each Hours Worked cell shows the final reconciled weekly total as the primary bold value. It does not reveal pay rates, effective dates or allocation detail. Public-holiday dates remain in their intended field, and signed reconciliation information remains a compact subordinate `(Info only +/-X.XX hours)` line that is never totalled again.

The same integer-minute structure supplies displayed totals and persisted worked-item evidence, with residual rounding reconciliation so independently represented portions do not contradict the weekly total.

Payroll Settings also configures multiline footer/instructions below signature dates. Blank lines are preserved, empty text hides the footer, font sizes are 6–12 pt (default 7 pt), and overflow is rejected explicitly rather than silently shrinking or truncating text.

## Email

Both timesheet and payslip production email are sent from the employer address to the payroll department, CC the employer and BCC the PA when available. Preview composes without sending. Test sends use only configured test addresses and are visibly marked `TEST`; payslip test email goes directly to the configured PA test address.

Production confirmation captures the optional selected period and selection revision; timesheets require a period, but standalone P60/P45 do not. Dispatch refuses if the global operational selection changed, the captured schedule disappeared/changed, the required attachment is missing, or a timesheet candidate digest no longer matches. Successful timesheet transport freezes submitted membership; an SMTP failure does not.

Production and Test Payslip Email share read-only attachment selection: an eligible unsent ordinary cycle payslip when present plus unsent imported P60/P45, validated against files and imported digests. An ordinary payslip is not required. Sent documents are excluded; indeterminate delivery blocks automatic retry. P30, general information and unassociated ordinary archives are excluded. Empty test selections give a clear no-eligible-documents error. Test Timesheet Email retains its period/candidate eligibility and wording.

If an ordinary payslip is included, ordinary and combined bundles retain the configured cycle-based subject and Payslip Email Body. P60/P45-only bundles use `Payroll documents - {Personal Assistant Name}` and `Please find attached your payroll document.` (one attachment) or `Please find attached your payroll documents.` (multiple), regardless of Dashboard selection. Existing notes and employer signature are appended normally. Standalone test subjects are `TEST: Payroll documents - {Personal Assistant Name}`. Test sends retain TEST subject/body markers and go only to the configured PA test address, with no CC/BCC; they never mutate delivery, sent, settlement, schedule or evidence state.

Production protects the selected ordinary status and specific P60/P45 document IDs atomically before transport and marks successful delivery afterwards. SMTP failure restores unsent state; uncertain finalisation remains protected. P60/P45 delivery never establishes ordinary-payslip settlement or schedule completion. See [schema31 delivery](DATABASE-SCHEMA.md#schema-31-cycle-independent-pa-payroll-documents).

## Payroll document files

Import Payroll Documents accepts individual files or mixed ZIPs and classifies before requesting a period. Ordinary payslips use an exact stored association, an applicable user-selected period, or a PA archive fallback when no cycle applies. Associated filenames remain `Payslip for Week <week> for <PA>.pdf`; unassociated ordinary archives are not automatically emailed or used for settlement.

P60/P45 use their own explicit filename tax year (or an unknown-year PA area), without borrowing a prep-sheet year or Dashboard period. P45 never changes employment data. P30/general information goes to the configured payroll-information root, grouped by its own explicit filename year when present, otherwise directly under that root. It never enters PA email or `email_archive`. See [classification, mixed imports and idempotency](ARCHITECTURE.md#schema31-payroll-documents).

## Backup and restore

A backup is an application-owned timestamp directory containing a consistent SQLite snapshot, optional config and identity README. Restore is limited to recognised backups, validates integrity/schema, creates a mandatory safety backup, restores config only if present and requires restart. Backups are never automatically removed.

## Boundaries

The application prepares payroll evidence and provider documents; it is not a general payroll/pay calculation engine. Overtime calculation, configurable frequency/workweek engines, scheduled backups, retention policy, cloud storage and multi-user access are not implemented. See [current limitations](../README.md#current-limitations) for the remaining boundaries.
