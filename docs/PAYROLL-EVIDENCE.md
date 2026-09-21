# Payroll evidence and reconciliation (schema 29)

Schema29 is the evidence-migration milestone described here; the current application is v1.0.3/schema32.

## Sources and possible duplicates

Hours Keeper CSV rows remain in `timesheets`, with original CSV bytes archived and the existing atomic import audit. Imported correction events project effective values without rewriting the original row. Completed Hours Shift rows remain in `direct_shifts`; running rows are excluded, and soft-deleted completed rows remain available for audit but not payable work. Direct duration is actual end minus start minus break. Imported worked duration remains independently authoritative. Payroll rounding settings do not change either source's duplicate identity.

`payroll_evidence` reads both sources into a source-labelled working model. It does not copy direct shifts into `timesheets`. Exact positive clock overlap for the same PA and start calendar date forms possible duplicate groups. Touching intervals are separate. Groups are transitive and have no size limit. Old date-only imported rows retain their dates/durations; no clock interval is invented for them.

The consolidated resolver requires one radio-button selection per displayed group. It never selects a candidate by source, length or recency. Original records remain intact. `payroll_duplicate_decisions` and `payroll_duplicate_members` retain the selected identity, complete candidate descriptions, material fingerprints, actor/time and invalidation. A changed PA, interval, break, duration, deletion or group membership invalidates the old decision during preflight. Notes and displayed names are excluded from material fingerprints. An excluded shift corrected to a non-overlapping interval becomes eligible again.

Import retains conflicting same-start and within-file duplicate candidates instead of refusing the file. For newly processed paths, exact existing source rows in repeated exports remain idempotent and their CSV evidence is archived. A pathname already recorded as successfully imported is skipped before reading/archiving again, even if its contents changed; durable CSV content identity remains future work. Import results identify the affected PA/dates for the consolidated resolver. Returning to preparation checks currently stored evidence, without importing files. Duplicate preflight does not regenerate PDFs. Unchanged preparation and correction reservations remain unchanged.

## Submission and settlement

Milestones are per PA/payroll period, not schedule-wide:

1. Editable preparation uses current eligible evidence and explicit manual adjustments.
2. A successfully emailed timesheet preserves submitted evidence. Differences are reviewed inside that period's preparation. **Correct / Resubmit Timesheet** explicitly opens a replacement preparation; **Carry correction forward** explicitly records the discrepancy instead. The replacement is generated and emailed through the existing normal workflow. Other submitted/settled PAs are skipped during batch PDF generation.
3. A definitively Sent per-PA `email_type='payslip'` status establishes settlement. Indeterminate delivery remains protected. P60/P45 per-document delivery does not settle payroll or complete a schedule; see [schema31 documents](DATABASE-SCHEMA.md#schema-31-cycle-independent-pa-payroll-documents). Settled preparation cannot be silently recalculated or saved.

`payroll_submissions` and its item/week/leave/holiday child tables preserve submission history. Successful resubmission links to the prior submission with `supersedes_id`; the latest successful submission is the expected submitted baseline. Production submissions retain the attachment bytes and digest. Migration copies only retained submitted data; an unavailable legacy attachment or submission timestamp remains unavailable. There are no user-facing PDF revision numbers and normal filenames are unchanged.

The existing `submitted_at` history is displayed in discrepancy details. It identifies the submission milestone, not proof that all later shifts were estimated or that actual evidence is complete. Explicit manual preparation adjustments remain distinct from source shifts.

`payroll_candidate_checks` records a material evidence signature. Production send checks source membership, duplicate decisions, rates, competing submissions and correction availability as well as the existing PDF path/digest checks. A stale candidate must be regenerated. A notes-only source edit does not require regeneration.

## Selected payroll production

Dashboard generation and production timesheet email capture an explicit set of
PA IDs for the operational period. All available PAs are selected by default;
checkboxes permit one or several. Submitted/settled and indeterminate PAs are
unavailable. Email selection requires a current candidate, with path/digest and
current evidence checked again at send time. Successful early submissions are
excluded from later ordinary batches; replacements require the existing audited
resubmission decision.

`load_for_pa` filters source rows in SQL before parsing. `preflight_for_pa` scopes
both current evidence and the retained duplicate-decision inventory by source
ownership, so an omitted PA's decisions are not invalidated. Reconciliation and
candidate signatures/verification use this scope. Global import/preparation
preflight retains its original whole-inventory semantics. Selected processing
stops at the first failure and reports completed, failed and unattempted PAs;
previous successes remain committed and retry cannot resend submitted PAs.

Saved Payroll Department notes follow the existing schema-32 PDF and retained
submission path. Source shift notes still have no material evidence effect;
returned actual in-lieu hours remain outside all production calculations/state.

## Historical work and review

Outstanding dated work is no longer limited to the preceding period's final two weeks. Work keeps its actual historical date and maintained rate effective on that date. Submitted source membership reserves work; settled membership prevents repeat payment. Unconsumed outstanding work remains eligible in later open preparation. A missing historical rate stops allocation rather than inventing a rate.

Saving a historical internal shift displays an informational notice when its recorded period is settled. Legacy imported membership can use the immutable original row’s exact interval when its submitted source ID, PA, work date and duration match; this is a read-time lookup and does not manufacture or rewrite snapshot data. If settled history lacks exact paid membership/interval evidence, preparation holds the source for **Already paid / included** versus **Genuinely unpaid — carry forward** review. The decision is fingerprinted and audited. Settled totals are preserved. An already-paid decision excludes that evidence from payment; an unpaid decision allows the normal dated outstanding path.

Post-submission evidence differences are not silently written into the original payroll. Concrete changes to known settled source evidence can establish an individual correction. A reviewed duplicate replacement against retained submitted evidence reconciles the difference rather than paying the replacement in full again.

An aggregate negative estimate difference is only a proposal until the user explicitly confirms that actual evidence for the displayed aggregate portions is complete and chooses carry-forward. Missing imports/entries alone never create an aggregate negative correction. Confirmation is recorded with the discrepancy and submission relationship. Future weekly portions are not proposed as actual shortfalls. Historical aggregate reviews do not fabricate source membership or rates.

## Corrections, totals and audit

`payroll_corrections` retains separate signed components with origin, submission, decision, source/date/rate evidence when known, reason and timestamp. `payroll_correction_evidence` links actual source evidence covered by an aggregate reconciliation so it cannot also flow through as a full unpaid shift. `payroll_correction_applications` records current preparation reservations; immutable `payroll_submission_corrections` records their submitted applications. A correction is not consumed by merely opening a screen or generating a PDF.

Positive components are applied before negative components, in creation/ID order within each sign. Negative applications consume available worked hours across the four weeks without taking any week below zero. An unapplied negative remainder stays attached to its original correction. Reservations in another submitted/uncertain payroll prevent duplicate application; settlement confirms consumption. Detailed component records remain separate even though their visible effect is consolidated.

The PDF's bold worked-hours values already include applied corrections. The subordinate line is exactly the simple signed form `(Info only -3.00 hours)` or `(Info only +2.00 hours)`. This line is never added to totals. Reasons and history belong in preparation's audit/details view. Contracted-hours entitlement does not participate. Existing explicit historical backfill adjustments retain their information-only treatment and unknown historical rate evidence. The one-off startup repair and its repeated public-holiday synchronisation have been retired. Startup no longer requires that repair’s source dataset or writes its historical values. Existing adjustment rows remain supported through the exact legacy reason matcher in `src/historical_adjustment_compatibility.rs`; this compatibility code does not write or reinterpret stored records.

## Manual verification with disposable data

1. Import an ordinary Hours Keeper CSV; verify archive/audit and normal preparation hours. Repeat the export and check idempotency. Import conflicting same-start rows and confirm all candidates survive.
2. Complete an Hours Shift; check inclusion without a CSV. Confirm a running or soft-deleted shift contributes nothing. Leave preparation, add another shift, return and check reassessment.
3. Create imported/internal and same-source overlapping groups, including four candidates and a transitive chain. Verify no default winner, independent group selections and disabled Apply until all groups are selected. Touching intervals must remain separate.
4. Reopen after resolving; confirm no repeated resolver. Change only notes, then materially edit an excluded shift to a separate interval. Both separate shifts should become eligible, with old decisions/audits retained.
5. Enter genuinely forgotten work several periods back. With aggregate-only settled history, test each paid/unpaid decision; only unpaid work should carry. Confirm the historical date/rate and no rewriting of old totals.
6. Email a timesheet through the existing confirmation workflow. Change evidence before sending the payslip. Check that original preparation is protected; test resubmission and carry-forward separately. Verify original/replacement submissions and attachments remain retained, with ordinary filenames.
7. Test positive and negative carry-forward. For -3 hours with 2 current hours, expect zero payable and -1 still outstanding. Open a later period without consuming it, then verify it remains available. Inspect separate component history.
8. For aggregate shortfalls, verify no negative correction exists until evidence completeness is confirmed and carry-forward chosen. An audited individual shift correction should not require the blanket confirmation.
9. Verify the PDF's bold hours include corrections once, and the signed Info only line is subordinate. Send one PA's payslip only; that PA must be settled while another PA in the same period remains open. Try editing/generating settled payroll and verify it is preserved.
10. Generate a PDF, then add or materially edit a relevant shift before sending. Sending must refuse the stale candidate. A notes-only change should not produce that refusal.

## Legacy settlement cutover (schema 29)

Migration 29 persists a verified pre-schema-28 inventory separately from individual
payment decisions. `payroll_legacy_evidence` records source/ID/material fingerprint;
`payroll_legacy_settlements` records the definitive per-PA settled preparation and
retained payslip timestamp. `payroll_legacy_cutover` retains the complete inventory,
its provenance and the time it was **recorded**, not an invented migration-28 time.
No submitted membership, individual-paid decision, rate or correction is backfilled.

A matching unchanged source in its already-settled original period is held out of
new unpaid claims and individual historical-payment reviews. New dated-work
corrections do not arise merely from that baseline source lacking submitted
membership. Opaque cutover weekly aggregates are not reinterpreted as estimate
discrepancies from an inventory that never established submitted membership;
concrete corrections to known submitted items retain the existing rules.
Duplicate preflight still runs first and preserves existing decisions.
Material edits lose the baseline match; notes-only changes do not. Pre-cutover
membership without cutover settlement does not establish payment, and later
settlement does not expand the baseline. Original payroll aggregates are unchanged.

For upgrades from before schema 28, migration 29 captures existing evidence and
settlement as part of startup. An already-schema-28 database has no implicit
cutover: absent a verified inventory it receives empty baseline tables, preserving
all review protections. Before its first schema-29 startup, an operator may supply
`<database path>.schema28-cutover.toml`. The migration validates every supplied
source and settlement against current data and commits inventory and version
atomically; mismatches abort without a partial migration. Subsequent startups do
not reread or expand the inventory.

`scripts/prepare_legacy_cutover_inventory.py` builds this recovery file read-only
from a pre-28 backup and the live schema-28 database. It compares complete imported
rows, not counts alone. Additional direct IDs require explicit, independently
established pre-cutover provenance via `--confirmed-direct-id`; the operator must
also confirm the captured definitive payslip statuses existed at cutover. The
script never infers this from historical work dates. Keep this sensitive inventory
with application data, not in source control. Back up the database and stage the
file before starting the new build; do not use an unverified current inventory.

## Payable rounding

Open payroll converts each selected imported or completed direct worked duration
using the same configured Payroll Settings increment and Up/Down direction. For
example, 34 raw minutes with a 15-minute increment becomes 45 payable minutes Up
or 30 Down. Current and outstanding late source work use this same allocation
boundary. Source start/end/break/minutes and duplicate fingerprints remain exact;
`source_evidence` retains those raw values while snapshot `worked_minutes` records
payable duration. Existing corrections and manual adjustments are already payable
values and are not rounded again. Submitted/settled history is not recalculated
when settings change, and unchanged raw evidence does not create a discrepancy
merely because its submitted payable duration was rounded.
