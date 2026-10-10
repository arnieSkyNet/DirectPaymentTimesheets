# Payroll evidence and reconciliation (schema 29)

Schema29 is the evidence-migration milestone described here; the development application is v1.0.6/schema38.

## Sources and possible duplicates

Hours Keeper CSV rows remain in `timesheets`, with original CSV bytes archived and the existing atomic import audit. Imported correction events project effective values without rewriting the original row. Completed Hours Shift rows remain in `direct_shifts`; running rows are excluded, and soft-deleted completed rows remain available for audit but not payable work. Direct duration is actual end minus start minus break. Imported worked duration remains independently authoritative. Payroll rounding settings do not change either source's duplicate identity.

`payroll_evidence` reads both sources into a source-labelled working model. It does not copy direct shifts into `timesheets`. Stage 4 detects exact positive clock overlap for the same PA across calendar-date boundaries, including overnight shifts. Existing legacy grouping/financial obligations are preserved until new evidence is introduced or explicitly reviewed. Touching intervals are separate. Groups are transitive and have no size limit. Old date-only imported rows retain their dates/durations; no clock interval is invented for them.

The consolidated resolver requires an explicit payable-version choice or an explicit retain-all-as-separate decision per displayed group. It never selects a candidate by source, length or recency. Original records remain intact. `payroll_duplicate_decisions` and `payroll_duplicate_members` retain the selected identity, complete candidate descriptions, material fingerprints, actor/time and invalidation. A changed PA, interval, break, duration, deletion or group membership invalidates the old decision during preflight. Notes and displayed names are excluded from payroll material fingerprints, but captured review authorisation also binds notes and imported monetary fields. Stage4 edits retain previously reviewed counterpart associations across date moves so a replacement cannot silently become payable alongside its original; newly separate versions require another explicit decision.

Import retains conflicting same-start and within-file duplicate candidates instead of overwriting rows. For every new content/path combination, exact existing raw source rows remain idempotent and captured CSV bytes are archived. Stage 4 identifies bytes by SHA-256, so an already imported pathname with changed contents is processed. Missing rows never imply deletion. An omitted retained row plus newly added rows for the same PA in a changed export becomes a possible correction review, including moved dates. New internal edits retain links to previously reviewed duplicate counterparts even after moving away from the old interval. Neither source is silently superseded. A choice can be deferred with durable history; affected payroll stays blocked while other PAs remain usable.

## Submission and settlement

Milestones are per PA/payroll period, not schedule-wide:

1. Editable preparation uses current eligible evidence and explicit manual adjustments.
2. A successfully emailed timesheet preserves submitted evidence. Differences are reviewed inside that period's preparation. **Correct / Resubmit Timesheet** explicitly opens a replacement preparation; **Carry correction forward** explicitly records the discrepancy instead. The replacement is generated and emailed through the existing normal workflow. Other submitted/settled PAs are skipped during batch PDF generation.
3. A definitively Sent per-PA `email_type='payslip'` status establishes settlement. Indeterminate delivery remains protected. P60/P45 per-document delivery does not settle payroll or complete a schedule; see [schema31 documents](DATABASE-SCHEMA.md#schema-31-cycle-independent-pa-payroll-documents). Settled preparation cannot be silently recalculated or saved.

## Payroll document history and corrections (schemas 31, 33 and 35)

Ordinary payslips are cycle-associated and settle only through their per-PA cycle status. `payslip_revisions` retains each explicitly corrected file's path, SHA-256, current flag and delivery marker; replacement makes the new bytes current and unsent while preserving the old bytes and their prior sent/indeterminate evidence. The old marker continues to describe the old bytes, not the corrected ones.

P45/P60 are separate PA-specific, cycle-independent documents. Their `history_state` is `unknown`, `external`, `needs_sending` or `application`; this declaration is distinct from `sent_at`, which records application transport evidence or an indeterminate attempt. New/corrected documents begin unknown with NULL `sent_at`. Reconciliation changes only `history_state`; only needs_sending plus NULL `sent_at` is eligible for production. Corrected documents link to the superseding registration, while older rows and hashes remain for history. P30 and other shared payroll-information documents have no PA delivery identity and are never part of a PA email bundle.

Email Payslips selects recipients individually. For each selected PA, production sends one bundle To that PA, BCCs the employer, and has no CC or payroll-department recipient. A normal ordinary/combined subject is `Payslip for Week <PAYE week>` from the captured schedule and uses the configured payslip body; a standalone P45/P60 bundle uses neutral `Payroll documents - {Personal Assistant Name}` wording. MIME filenames use clean canonical names from structured document identity, not internal replacement revision/hash storage basenames. Delivery evidence is updated only by production transport; test-email sends do not update it.

`payroll_submissions` and its item/week/leave/holiday child tables preserve submission history. Successful resubmission links to the prior submission with `supersedes_id`; the latest successful submission is the expected submitted baseline. Production submissions retain the attachment bytes and digest. Migration copies only retained submitted data; an unavailable legacy attachment or submission timestamp remains unavailable. There are no user-facing PDF revision numbers and normal filenames are unchanged.

The existing `submitted_at` history is displayed in discrepancy details. It identifies the submission milestone, not proof that all later shifts were estimated or that actual evidence is complete. Explicit manual preparation adjustments remain distinct from source shifts.

`payroll_candidate_checks` records a material evidence signature. Production send checks source membership, duplicate decisions, rates, competing submissions and correction availability as well as the existing PDF path/digest checks. A stale candidate must be regenerated. A notes-only source edit does not require regeneration.

## Selected payroll production

Dashboard generation and production timesheet email capture the operational period.
Generation retains intentional regeneration of submitted payroll; settled and
indeterminate payroll cannot be regenerated. Email uses stable document identities:
unsent current versions (including newly generated corrected PDFs) default selected;
sent current versions remain visible and unticked. **Select all unsent** excludes
resends. A manual resend requires explicit acknowledgement and separate final
confirmation listing first sends/resends, actual To/CC/BCC recipients and the period.
A changed selection/document/routing/classification invalidates confirmation.

`load_for_pa` filters source rows in SQL before parsing. `preflight_for_pa` scopes
both current evidence and the retained duplicate-decision inventory by source
ownership, so an omitted PA's decisions are not invalidated. Reconciliation and
candidate signatures/verification use this scope. Global import/preparation
preflight retains its original whole-inventory semantics. Selected processing
stops at the first failure and reports completed, failed and unattempted PAs;
previous successes remain committed. Replaying an accepted dispatch intent skips it; a fresh retry selection defaults only to unsent candidates. A failed resend stays unticked because that document was previously sent.

Saved Payroll Department notes follow the existing schema-32 PDF and retained
submission path. Source shift notes still have no material evidence effect;
returned actual in-lieu hours remain outside all production calculations/state.

## Historical work and review

Outstanding dated work is no longer limited to the preceding period's final two weeks. Work keeps its actual historical date and maintained rate effective on that date. Submitted source membership reserves work; settled membership prevents repeat payment. Unconsumed outstanding work remains eligible in later open preparation. A missing historical rate stops allocation rather than inventing a rate.

Saving a historical internal shift displays an informational notice when its recorded period is settled. Legacy imported membership can use the immutable original row’s exact interval when its submitted source ID, PA, work date and duration match; this is a read-time lookup and does not manufacture or rewrite snapshot data. If settled history lacks exact paid membership/interval evidence, preparation holds the source for **Already paid / included** versus **Genuinely unpaid — carry forward** review. The decision is fingerprinted and audited. Settled totals are preserved. An already-paid decision excludes that evidence from payment; an unpaid decision allows the normal dated outstanding path.

Post-submission evidence differences are not silently written into the original payroll. Pre-Stage-4 concrete audited changes retain their original automatic correction rules and existing obligations. New changes require reviewed source authorisation for protected cycles and explicit contextual financial correction/carry-forward; delivery uncertainty blocks source authorisation until audited resolution. A reviewed duplicate replacement against retained submitted evidence reconciles the difference rather than paying the replacement in full again.

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


## Production timesheet attempts and review (schema36)

`timesheet_delivery` retains immutable generated-document identities/bytes and
records every transport attempt before SMTP. Claims are atomic and unique per
unresolved timesheet, with a unique dispatch intent and Message-ID. The claimed
items, weeks, leave, holidays, corrections and payroll note are frozen before
transport. Accepted first sends append one submission from that payload; accepted
resends attach delivery evidence to the existing submission. Original submission
and settlement records remain unchanged.

Explicit SMTP negative responses establish non-acceptance. Generic transport,
timeout/TLS/I/O errors and interrupted claims remain uncertain. Acceptance means
SMTP acceptance, not proof of recipient delivery. Dashboard history exposes all
attempts and requires a reason/evidence plus a separate confirmation for review.
The original uncertainty is retained alongside actor/time/decision evidence. A
review establishing acceptance finalises the frozen payload; established
non-acceptance releases eligibility; inconclusive evidence leaves it blocked.
An OS sidecar lock prevents review during an active sender and is released by the
kernel on process exit. Legacy uncertainty is reviewed without inventing attempts;
acceptance cannot be finalised without the original PDF evidence.

Migration36 preserves existing submissions and their bytes, corrections and
settlements. Legacy missing bytes remain absent until a matching retained file is
verified; no historical attempts or resend counts are inferred. No numbered PDF
revision architecture is reintroduced. Preview/test email and payslip/P45/P60
routing and delivery rules remain unchanged.

Byte-identical regeneration with identical represented evidence retains the same
document identity and sent status. Material corrections allocate a new identity.


Restore cannot release unresolved attempts or legacy indeterminate email states.
An approved destructive rollback may remove newer evidence from the active
working database only after a separate verified original recovery copy and
reason/decision manifest are durable. Review explicitly lists changed/absent
current rows and warns of duplicate payroll processing. Reconcile any rollback
with Payroll before further sending; preserved historical acceptance is not proof
of recipient delivery. Published older executables lack these recovery safeguards.


## Sickness-only historical corrections (schema37)

An authorised sickness correction extends the existing reviewed resubmission/document-delivery lifecycle with date-only evidence. It never becomes a worked-minute or SSP calculation. Submitted and settled original evidence remains retained, and settled status/totals remain unchanged. Corrected dates appear directly in the original weekly table layout; the next populated-note asterisk labels an explanation outside the table as `Sickness Information Correction`. Payroll assesses financial implications.

Sickness changes check the union of original/proposed ranges transactionally under the dispatch OS lock. Explicit editable-only scope retains protected cycle projections. Uncertain cycles remain blocked until audited resolution. Deterministic date evidence invalidates unsent candidates and blocks stale first delivery; immutable resends preserve the exact original approved bytes. Each generated document, attempt and successful submission retains its structured sickness evidence. A later authorised general worked-evidence correction retires the limited sickness-only generation authority without removing its historical audit.


Stage 4 review authorisation captures complete displayed shift evidence (including notes and imported rate/amount), affected cycle state and retained submission membership. Logger edits additionally bind the original record and complete proposed payload and require a reason for protected-cycle changes. Unauthorised protected edits and deletions are refused. Source approval preserves settled amounts and original submissions; it does not itself authorise financial carry-forward. Production first-send freshness includes possible-move groups as well as overlap groups. Explicit resends retain their original approved PDF bytes.

Hours Keeper does not supply an entry ID in the supported eight-column export. Matching establishes exact repeats and possible corrections, never proves replacement from similarity. Same-path export revisions supply context for moved-date review. Independently recorded rows differing by exactly one day with matching start/finish clock times, breaks, duration and identical nonempty notes are also possible-date-change suggestions; imported/imported comparisons additionally require equal retained rate/amount. Co-present rows in one CSV stay separate, and separate internal clock-ins are not treated as revisions merely because they repeat a daily pattern. Other non-overlapping rows without an existing counterpart association remain separate because evidence cannot reliably establish a correction. Similar legitimate recurring shifts can require an explicit retain-all decision; no suggestion automatically overwrites or pays a replacement. No SSP, public-holiday or imported worked-duration interpretation changes are introduced.
