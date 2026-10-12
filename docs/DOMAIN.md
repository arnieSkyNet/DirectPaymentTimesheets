# DirectPaymentTimesheets domain guide

This describes unreleased development 1.0.6 on `main`, schema41, including Stages
1–8. Historical schema references identify feature introductions, not the current
schema. See [published downloads](../README.md#download) for Linux/Windows v1.0.4
and macOS v1.0.5.

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

The established duration rule assigns an entire shift to its start calendar date. Duplicate detection compares complete timestamp intervals, including overlaps across midnight and different start dates, without changing payroll allocation. Imported clock values are not rewritten by preparation corrections.

## Directly recorded work

A DirectShift is application-created source evidence stored independently from imported `TimesheetEntry` rows. It records the maintained PA, exact local start and optional end date/time to the minute, actual break minutes, an optional note, a fixed `direct` source marker and creation/update timestamps. A `NULL` end represents a durable running shift and survives application restart.

Direct shift evidence represents what actually happened. Payroll allocation derives payable time and rates from it, but must never rewrite evidence as a side effect. Deliberate completed-shift corrections update the current record only while atomically appending immutable before/after audit evidence. Delete is a soft deletion: deleted rows remain stored and audited but are excluded from normal use. Cancelling an accidental running clock-in removes its current row only after recording a cancellation audit snapshot.

Every mutation records action type/time and actor. The desktop actor is currently the stable `local_employer` identity; the text actor field is intentionally suitable for future authenticated identities, but authentication, accounts and web/mobile access do not yet exist. Actual worked minutes are derived as end minus start minus break, without payroll rounding. Completed, non-deleted direct shifts feed the shared payroll evidence reconciliation and duplicate resolver, without being copied to imported rows.

Stage4 authorisation applies prospectively: new edits affecting protected cycles require a reason and explicit before/after review, delivery uncertainty blocks changes, and financial correction still requires the existing payroll review. Earlier audited edits, submitted applications and obligations remain intact. New open-cycle edits remain straightforward; protected deletions are refused. Possible duplicates can be approved as one payable version, explicitly retained as separate shifts, or deferred without deleting evidence.

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

Sick / SSP opens PA-owned `personal_assistant_sickness_periods` with inclusive start/end dates. Schema37 saves through one transactional mutation service, checks all affected cycles over both old and new ranges, requires a reason and reviewed confirmation, rejects stale edits and exact duplicates, and invalidates affected unsent PDFs. Legitimate overlapping periods and full-date cross-week repetition remain supported. Explicit editable-only scope freezes protected cycle projections while allowing editable portions to change; later corrections can edit that retained cycle view locally without changing other cycles. Moving or extending retained dates outside the selected cycle requires a reviewed transfer under a fresh active identity; all destination cycles are checked, conflicting versions are refused, and no unpersisted dates are reported as saved.

Submitted or settled changes require an explicitly authorised sickness-only correction associated with the existing reconciliation `resubmit` decision and original submission. Settlement, worked amounts and existing submissions/PDFs are preserved. A separately named replacement PDF displays corrected dates in the existing Sick / SSP cells; its explanation appears only in the notes area as a dynamically numbered `Sickness Information Correction` footnote. Payroll must assess financial implications; the application does not calculate SSP. Unresolved delivery uncertainty blocks overlapping changes, including authorised corrections. Historical documents and attempt/submission sickness snapshots remain retained. Neither employer nor former PA sickness enablement gates the workflow. Weekly `sick_leave_hours` remains legacy data; hours without date evidence prompt review rather than invented dates.

Travel Miles opens a weekly editor only when the PA's Enable mileage setting permits it. Finite, non-negative miles save immediately, invalidate stale candidates and respect protected payroll. Employer mileage flags are unused. Disabling PA mileage does not erase previously stored miles; PDFs continue to read stored values.

## Annual-leave guidance

PA Maintenance shows saved-data, read-only guidance; Payroll remains definitive. Effective-dated Contracted/Variable Hours Basis and inclusive employment dates apply within a fixed April–March leave year. Contracted entitlement uses applicable weekly hours and statutory weeks; Variable accrual uses qualifying retained work, gated by the PA/cycle's definitively Sent ordinary payslip, rounded once per cycle to whole hours. Safe historical weeks can fall back to retained worked totals when submitted item evidence is unavailable and attribution is unambiguous; unsafe evidence is excluded or flagged for review.

Taken leave uses actual dated entries, or week commencing for legacy undated totals, never both. Negative remaining is retained. Display figures round independently up to quarter-hours while Details retains precision; this is separate from payable-work rounding. Current singleton SQLite settings have no historical numeric rule values. Structured sickness dates and legacy weekly `sick_leave_hours` flag additional variable-hours review. Structured periods are checked against the selected leave year, employment and effective Hours Basis, up to today. Neither source adds sickness accrual figures; the sickness reference-period/accrual calculation remains unimplemented. See [guidance details](DEVELOPMENT.md#annual-leave-guidance-schema-27-unchanged).

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
- **Indeterminate**: protected state where acceptance is unknown after an ambiguous SMTP error or interrupted claim, or finalisation failed after possible/known acceptance.

Submitted and indeterminate records are read-only in preparation. Changing represented data for a candidate invalidates it before the change is persisted, requiring regeneration. Opening unchanged data does not invalidate it. Stale-bound screens cannot save after the operational selection or material schedule dates change.

## Public holidays

Public-holiday identification is always active. Payroll Settings selects England and Wales (the default for existing and new configurations), Scotland or Northern Ireland. New four-week preparation receives zero-hour date suggestions from the selected regional GOV.UK bank-holiday feed. Published dates are used directly, including exceptional, moved and substitute holidays; names, notes and bunting do not determine dates, entitlement or pay. Existing editable drafts retain every date and entered hour; preparation shows missing and unexpected dates for manual review instead of accumulating suggestions from another region. Submitted and indeterminate records retain their original values and PDF evidence.

Settings and preparation show the selected region, cache check time, published years, sources and warnings. The feed has no year-completeness guarantee. Missing or suspiciously sparse coverage requires review. Valid cached regional data remains usable offline. When a year is unavailable, England/Wales uses the established recurring calculation with an explicit incomplete-fallback warning; Scotland and Northern Ireland show unavailable/manual-review warnings and never receive England/Wales fallback dates. A covered published year is never merged with recurring rules.

Weekly payroll rows retain the public-holiday-hours aggregate, and individual rows retain dates and editable hours. CSV notes are not classified as holiday work. Deliberate subtraction from Hours Worked is a saved manual adjustment; entering holiday detail never automatically subtracts source hours. Existing adjustments survive source corrections and must not be deducted again.

Refresh is manual or periodic in the background (seven-day interval, one-hour retry backoff; manual retry remains available). Network failures do not block payroll or alter records. Preparation save, generation approval and final timesheet sending recheck the selected region and relevant normalized dates/source; changed calendar facts require fresh review. A timestamp-only refresh does not invalidate approval. Freshly approved historical resends continue to use the retained original PDF bytes.

There is no `public_holiday_enabled` business rule. An obsolete config key is ignored for compatibility.

## Payroll PDF

The provider PDF retains its established table and configured typography. Each Hours Worked cell shows the final reconciled weekly total as the primary bold value. It does not reveal pay rates, effective dates or allocation detail. Public-holiday dates remain in their intended field, and signed reconciliation information remains a compact subordinate `(Info only +/-X.XX hours)` line that is never totalled again.

The same integer-minute structure supplies displayed totals and persisted worked-item evidence, with residual rounding reconciliation so independently represented portions do not contradict the weekly total.

Payroll Settings also configures multiline footer/instructions below signature dates. Blank lines are preserved, empty text hides the footer, font sizes are 6–12 pt (default 7 pt), and overflow is rejected explicitly rather than silently shrinking or truncating text.

## Email

Timesheet production email remains from employer to payroll department, optional default-enabled CC employer and BCC PA when available. Payslip production uses To PA, BCC employer and no CC or payroll department recipient; a valid PA address is mandatory. Preview composes without sending. Test sends use only configured test addresses and are visibly marked `TEST`; payslip test email goes directly to the configured PA test address.

Production confirmation captures the optional selected period and selection revision; timesheets require a period, but standalone P60/P45 do not. Dispatch refuses if the global operational selection changed, the captured schedule disappeared/changed, the required attachment is missing, or a timesheet candidate digest no longer matches. Accepted first-send transport freezes the claimed submitted membership once. An explicit negative SMTP response releases retry eligibility; ambiguous exceptions and interrupted attempts remain protected pending audited review. Accepted resends record delivery evidence against the existing submission.

Production and Test Payslip Email share read-only attachment selection: an eligible unsent ordinary cycle payslip when present plus P60/P45 explicitly reconciled as Needs sending and not yet sent, validated against files and imported digests. An ordinary payslip is not required. Sent and externally delivered documents are excluded; unknown history requires explicit reconciliation and is excluded until Needs sending is chosen. Indeterminate delivery blocks automatic retry. P30, general information and unassociated ordinary archives are excluded. Empty test selections give a clear no-eligible-documents error. Test Timesheet Email retains its period/candidate eligibility and wording.

If an ordinary payslip is included, ordinary and combined bundles use `Payslip for Week <PAYE week>` from the captured schedule and retain the configured Payslip Email Body. The timesheet subject setting does not apply. P60/P45-only bundles use `Payroll documents - {Personal Assistant Name}` and `Please find attached your payroll document.` (one attachment) or `Please find attached your payroll documents.` (multiple), regardless of Dashboard selection. Production payslips are sent separately to each selected PA: To that PA, BCC employer, no CC or payroll-department recipient. Existing notes and employer signature are appended normally. MIME filenames for ordinary payslip, P45 and P60 use clean canonical names from structured document type/year and PA/schedule identity; internal revision/hash filenames are never exposed. Standalone test subjects are `TEST: Payroll documents - {Personal Assistant Name}`. Test sends retain TEST subject/body markers and go only to the configured PA test address, with no CC/BCC; they never mutate delivery, sent, settlement, schedule or evidence state.

Supplement `history_state` is a reconciliation declaration, separate from `sent_at`: `unknown` is excluded until explicitly reviewed; the user can record `external` (already delivered elsewhere) or `needs_sending`. Only `needs_sending` with NULL `sent_at` is eligible. `application` describes existing app delivery evidence; Sent and indeterminate `sent_at` evidence takes precedence. Supplement-history classification changes history_state only, not sent_at; this is separate from audited uncertain-attempt reconciliation, which can finalise protected sent_at markers. Production protects selected ordinary status and specific P45/P60 IDs atomically before transport and records SMTP acceptance afterwards, without claiming inbox receipt. Reliably established non-acceptance restores unsent state; generic SMTP errors, interrupted claims and uncertain finalisation remain protected. P60/P45 delivery never establishes ordinary-payslip settlement or schedule completion. See [schema31 introduction](DATABASE-SCHEMA.md#schema-31-cycle-independent-pa-payroll-documents) and [schema33 history](DATABASE-SCHEMA.md#schema-33-explicit-supplement-delivery-history).

### Reviewing uncertain delivery

1. Open **Dashboard → Timesheet Email Status** and expand the timesheet or
   payslip/P45/P60 history. Review the PA, document identities and SHA-256 hashes,
   known recipients, Message-ID, transport status, timestamps and prior decisions.
   Legacy unknown facts remain unknown; a registered file hash does not prove the
   bytes dispatched by a legacy attempt.
2. Gather external evidence and enter a reason explaining what it establishes.
   Choose **acceptance/external sending established**, **reliably confirmed not
   sent**, or **still uncertain**. SMTP acceptance and receipt of an employer copy
   do not prove delivery to another recipient's inbox.
3. Review the separate final confirmation. The application records the decision,
   reason, `local_employer` application identity and timestamp without deleting the
   original attempt. This identity is not an authenticated user account. An active
   dispatch holds the cross-process lock and cannot be reconciled concurrently.
4. Still uncertain retains the block. Confirmed not sent permits a new selection
   with fresh recipient and attachment approval, not replay of the old approval.
   Accepted/external-sent review finalises only the evidence-backed protected state;
   it sends no email. Retained SMTP acceptance cannot be contradicted by a not-sent
   bundle review. Repeated terminal resolution is refused.

Timesheet resends preserve immutable submitted bytes and require the existing
explicit acknowledgement. PA document bundles can continue for other safely
eligible PAs after an individual failure; timesheet batches stop on first failure
and retain earlier successes. Start a fresh selection for any remaining eligible
work. Resolution never silently reapplies corrections or reopens settled payroll.

### SMTP configuration and security

In **Settings → Email Settings**, **Local SMTP Server** uses unencrypted
`localhost:25` without credentials; its relay must secure onward delivery.
**SMTP Server** uses independently configurable host, port and security:

- **Required STARTTLS** (normally port 587) requires a successful TLS upgrade
  before credentials or email are transmitted.
- **Implicit TLS** (normally port 465) establishes TLS before SMTP begins.
- **Plaintext SMTP** permits intentional unencrypted relays and shows a warning:
  credentials, addresses and payroll contents can be intercepted.

New configurations select required STARTTLS for remote SMTP. Existing remote
configurations without `smtp_security` retain legacy plaintext behavior until
explicitly reviewed. A legacy local configuration defaults to STARTTLS on first
remote setup. Changing a port alone never enables encryption. Port suggestions
do not overwrite your entered port. Serialized security values are
`required_starttls`, `implicit_tls` and `plaintext`; unknown values are rejected.
TLS uses native platform certificate trust and hostname validation, with no
certificate bypass or fallback to plaintext. Private relays need certificates
trusted by the platform. Username/password authentication is attempted when a
remote username is supplied. OAuth account setup is not implemented.

Production and preview/test email share the same transport path. Failures before
message submission on that connection are definitely not sent; ambiguous
submission failures remain uncertain and block automatic retry. SMTP acceptance
never establishes inbox receipt. Server diagnostics retain only safe phase,
error class and numeric status, omitting server text. Security changes invalidate
existing send approvals; uncertain attempts retain their original evidence.

Passwords remain plaintext in `config.toml` and configuration backup copies;
masking the Settings field does not encrypt storage. Configuration Debug output
redacts credentials. Unix configuration publication and restore staging use
owner-only permissions; backup directories use mode 0700. Development 1.0.7
adds creation-time private Windows ACLs for new managed files/directories and a
read-only assessment for existing installations. Existing permissions and shared
business directories are not silently repaired; replacements preserve their
existing access policy. See [Phase 1 storage hardening](SECURITY-HARDENING.md). Database backups retain sensitive recipients,
email intent and immutable attachment bytes. Protect backups and external payroll
folders, and review permissions before sharing or exporting them.

Set dedicated test addresses for test email; test recipients do not replace
production routing. Review final production To/CC/BCC recipients before sending.
This functionality is unreleased development work, not a capability claim for
published packages. Platform trust-store and hardware acceptance remain required.

## Payroll document files

Import Payroll Documents accepts individual files or mixed ZIPs and classifies before requesting a period. ZIP paths, member types, encryption, entry count, individual and total expanded sizes, compression ratios and destination conflicts are validated; valid entries are staged/published independently so a failed entry does not roll back other successful items. Result summaries distinguish imported/already-present supplements, associated/archival payslips, information/prep sheets and failures, and report recovery paths where applicable. PA-name matching normalises case and separators, uses token boundaries and requires exactly one maintained match; ambiguous/incomplete matches are refused. Ordinary payslips use an exact stored association, an applicable user-selected period, or a PA archive fallback when no cycle applies. Associated filenames remain `Payslip for Week <week> for <PA>.pdf`; unassociated ordinary archives are not automatically emailed or used for settlement.

P60/P45 are PA-specific supplements and use their own explicit filename tax year (or the yearless payslip base), without borrowing a prep-sheet year or Dashboard period. P45 never changes employment data. P30, bank-transfer/quarter-end documents and other informational files are shared/non-PA documents. They go to the configured payroll-information root, grouped by their own explicit filename year when present, otherwise directly under that root. They never enter PA email or `email_archive`. See [classification, mixed imports and idempotency](ARCHITECTURE.md#payroll-document-import-and-delivery).

## Backup and restore

A backup is an application-owned timestamp directory containing a consistent SQLite snapshot, optional config and identity README. Restore is limited to recognised backups, validates integrity/schema, migrates in isolation and reviews changed/absent live rows. A reason, rollback acknowledgement and unchanged live/selected/staged evidence are required; unresolved delivery blocks restore. A verified original recovery copy precedes transactional installation under the OS lock and SQLite writer reservation. Config is installed separately only if present; close other instances and restart after restore or post-commit config failure. Retained SQLite attachment bytes are included, but external files require separate backups. See [recovery safeguards](DATABASE-SCHEMA.md#verified-startup-upgrades-and-legacy-restore). Backups are never automatically removed.

## Boundaries

Payroll Settings displays frequency, workweek and overtime preferences as read-only, unsupported values retained for compatibility. Imported schedules determine periods and week commencing dates. Rounding remains functional and editable.

The application prepares payroll evidence and provider documents; it is not a general payroll/pay calculation engine. Overtime calculation, configurable frequency/workweek engines, scheduled backups, retention policy, cloud storage and multi-user access are not implemented. See [current limitations](../README.md#current-limitations) for the remaining boundaries.


A deliberate saved Active-to-Inactive transition files safely identified PA-owned payslips, P45/P60s, retained revisions, generated timesheets and on-disk historical timesheet evidence into shared year-level Archived folders. Repeated inactive saves do not initiate another archival pass. Shared documents and external assets are excluded; reactivation does not restore files. New imports for an already inactive PA go directly to Archived without changing employment status.

Corrected documents require an explicit existing-identity selection and reviewed confirmation in Personal Assistant Maintenance. Old bytes and delivery evidence remain retained. Corrected payslips become current and unsent; corrected supplements become current with unknown history and no sent timestamp. P45 and P60 coexist independently. [Detailed safety and exclusions](ARCHITECTURE.md#explicit-payroll-document-corrections-schema35).

## Signature reliability and drawing (Stage 3)

Signature images are separate from the employer's email-text signature. The
employer and each PA retain their own nullable image path. Selection validates
PNG/JPEG content before changing the reference; saving a changed imported path
validates again. Supported JPEG regression examples include JFIF, Exif,
progressive and grayscale. Exif orientation is applied before PDF embedding;
existing signature positions, aspect-ratio fitting and unsigned spaces remain.

Drawing is optional in Maintenance and Dashboard generation selection. The
dialog names the database owner, requires explicit signer/delegate authorisation
and replacement acknowledgement, and saves only after **Use drawn signature**.
Black strokes and a transparent background are persisted immediately; other
unsaved maintenance fields are not saved by this action. Existing signatures
are never overwritten or deleted. A stale owner reference refuses replacement.
Generation reloads signature references, so a newly saved drawing is used
immediately and on subsequent generations, without repeated approval.

Unavailable configured signatures require a generation-scoped choice to replace,
continue unsigned or cancel. The unsigned choice leaves configuration intact and
never blocks subsequent printing, email or payroll completion. Intentionally
unconfigured signatures do not prompt. Previously generated/submitted PDFs and
approved resend bytes are unaffected by any later signature change. External
signature files remain external and require separate recovery protection.
