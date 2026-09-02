# DirectPaymentTimesheets domain guide

## Purpose and terminology

DirectPaymentTimesheets supports an employer who uses a UK direct payment to employ Personal Assistants (PAs) and submit information to an external payroll provider.

Use **timesheet** as one word. In user-facing payroll text, identify a period by payroll year, PAYE Payroll Week, four-week date range and scheduled pay date. `cycle_number` is an internal 1–13 database identity and is not the PAYE week printed in filenames.

## People and maintenance records

The employer record supplies identity/contact details, email routing and the declaration/signature information used on generated documents. The payroll-provider record supplies provider contact details. A PA has identity/contact details, an active state and effective-dated pay-rate and contracted-hours histories.

An active state of `NULL` is treated as active for compatibility. Inactive PAs remain historically meaningful:

- Payroll Timesheet Preparation includes them when they already have a record for the selected period.
- PDF generation includes them under the same rule.
- An inactive PA with no selected-period record is not added merely because a historical/future period was selected.

This historical inclusion applies to preparation and PDF generation. Production timesheet and payslip batches currently remain limited to active and legacy-`NULL`-status PAs.

## Imported work

A TimesheetEntry is an imported work interval associated with a PA. Its stable database ID is evidence used by submitted payroll snapshots and late-entry detection. Import preserves the source start/end values and parses worked minutes directly from the CSV worked-duration field; it does not recalculate them from start/end values or apply the persisted rounding setting. Duplicate detection prevents re-importing the same work as another ordinary row.

After a successful import, the source CSV is copied unchanged to the internal `archive/YYYY/MM/` hierarchy with a timestamped filename. `import_audit` records successful and failed imports and their source/archive details.

The external file's hourly-rate and amount values are not authoritative for payroll. The application resolves employer-maintained rate history instead.

Schema 21 introduced an append-only correction-event layer for the effective start, end, break, worked minutes and notes of an imported row. Every event retains complete before/after values, actor, action time and optional reason; reverting is another event. The archived CSV and original `timesheets` row remain immutable, including PA identity, rate and amount. Worked minutes remain independent source evidence and are not recalculated from clocks or break.

This foundation is intentionally not connected to the current UI or payroll pipeline. Import collision checks, Payroll Timesheet Preparation, snapshots and PDFs still consume raw imported evidence.

The established duration rule assigns an entire shift to its start calendar date; real project shifts do not cross midnight. Imported clock values are not rewritten by preparation corrections.

## Directly recorded work

A DirectShift is application-created source evidence stored independently from imported `TimesheetEntry` rows. It records the maintained PA, exact local start and optional end date/time to the minute, actual break minutes, an optional note, a fixed `direct` source marker and creation/update timestamps. A `NULL` end represents a durable running shift and survives application restart.

Direct shift evidence represents what actually happened. Payroll calculations may later derive payable time and rates from it, but must never rewrite evidence as a side effect. Deliberate completed-shift corrections update the current record only while atomically appending immutable before/after audit evidence. Delete is a soft deletion: deleted rows remain stored and audited but are excluded from normal use. Cancelling an accidental running clock-in removes its current row only after recording a cancellation audit snapshot.

Every mutation records action type/time and actor. The desktop actor is currently the stable `local_employer` identity; the text actor field is intentionally suitable for future authenticated identities, but authentication, accounts and web/mobile access do not yet exist. Actual worked minutes are derived as end minus start minus break, without payroll rounding. Direct shifts are not yet included in Payroll Timesheet Preparation and are not deduplicated or reconciled against imported work.

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

Payroll Return import has an independent period selection because a returned ZIP may arrive late. View Payroll Schedule independently selects a payroll year because viewing a future schedule must not alter operational work. Production email batches freeze the operational period that was selected when confirmation began.

## Worked hours and manual corrections

Ordinary worked hours come from imported shifts, grouped into the four schedule weeks. Payroll Timesheet Preparation permits the employer to set the final Hours Worked value when an external record is missing or wrong.

The application persists the correction as integer minutes separate from imported rows:

```text
manual adjustment = final prepared worked minutes - reconciled imported baseline minutes
```

An optional reason can accompany it. Positive and negative adjustments are preserved when the preparation is reloaded and in the submitted snapshot.

Annual leave, sick/SSP, public-holiday hours and mileage are preparation values. Contracted weekly hours are informational and never alter worked hours or pay.

## Effective-dated pay rates

The rate for an imported shift is the newest PA pay-rate record whose effective date is on or before the shift start date. Base rate and employer top-up are added. Future rows remain visible in maintenance but never apply early. Equal effective dates resolve deterministically by the newest database ID.

A payroll week may cross a rate boundary, including part-way through the week. Imported minutes retain their actual shift date and are allocated to the appropriate rates internally. Positive manual hours use the higher/newer rate applicable somewhere within that week, favouring the PA; negative manual hours use the lower/older applicable rate. Rates outside that week, including future rates, are not used. A positive worked item with no effective rate prevents generation rather than silently using zero or a future rate.

Rate portions are internal accounting/snapshot evidence. The payroll-provider PDF prints only the reconciled Hours Worked total.

## Effective-dated contracted hours

Each of the four payroll weeks independently uses the newest contracted-hours record effective on or before that week's commencing date. Equal dates use the newest ID, future rows are excluded, and a week before the first record is unavailable.

When all weeks share a value, the PDF retains `Contracted Weekly Hours: VALUE`. When values differ, a compact header summary associates values (or unavailable state) with week-commencing dates. No midweek contracted-hours rule is applied.

## Previous-cycle work

The existing positive previous-cycle adjustment detects late imported shifts from weeks three and four of the immediately preceding schedule. “Previous” is chronological, so cycle 1 of a new payroll year can follow cycle 13 of the prior year.

Exact detection compares current TimesheetEntry IDs with the immutable submitted snapshot. Late rows keep their historical start dates and rates, and rows on opposite sides of a rate boundary remain distinct. A generated-but-unsent candidate is not evidence that work was submitted.

Old schema-18 `previous_cycle_hours` can contain only an aggregate. Such an amount remains in totals as an opaque legacy adjustment with no invented shift ID, date or rate. The PDF presents the existing compact informational form such as `[Info.only +1 prev]`.

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

The provider PDF retains its established table and configured typography. Each Hours Worked cell shows the final reconciled weekly total as the primary bold value. It does not reveal pay rates, effective dates or allocation detail. Public-holiday dates remain in their intended field, and positive previous-cycle information remains a compact subordinate line.

The same integer-minute structure supplies displayed totals and persisted worked-item evidence, with residual rounding reconciliation so independently represented portions do not contradict the weekly total.

## Email

Both timesheet and payslip production email are sent from the employer address to the payroll department, CC the employer and BCC the PA when available. Preview composes without sending. Test sends use only configured test addresses and are visibly marked `TEST`; payslip test email goes directly to the configured PA test address.

Production confirmation is period-bound. Dispatch refuses if the global operational selection changed, the captured schedule disappeared/changed, the required attachment is missing, or a timesheet candidate digest no longer matches. Successful timesheet transport freezes submitted membership; an SMTP failure does not.

## Payroll Return files

The user explicitly selects the schedule to which a returned ZIP belongs. The selected schedule supplies year and PAYE week; today's cycle is not substituted.

Payslips use `Payslip for Week <week> for <PA>.pdf` inside the configured payslip base and a `YYYY to YYYY` folder. Unmatched payroll information uses the configured payroll-information base and the same year grouping, preserving safe original names and adding deterministic collision suffixes rather than overwriting. It deliberately does not use the configured `email_archive` path, which currently has no production consumer. P60-specific interpretation is not implemented.

## Backup and restore

A backup is an application-owned timestamp directory containing a consistent SQLite snapshot, optional config and identity README. Restore is limited to recognised backups, validates integrity/schema, creates a mandatory safety backup, restores config only if present and requires restart. Backups are never automatically removed.

## Boundaries

The application prepares payroll evidence and provider documents; it is not a general payroll/pay calculation engine. Overtime calculation, configurable workweek semantics, scheduled backups, retention policy, cloud storage, P60-specific processing and multi-user access are not currently implemented.
