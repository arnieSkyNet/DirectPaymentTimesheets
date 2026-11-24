# Timesheet amendments — v1.0.7

Open the original payroll period in Payroll Preparation, select the PA, edit the
hours and click Save. The enabled Save button is blue while the draft differs
from its saved values; successful saving restores normal styling and displays a
confirmation. Then generate a replacement PDF through Generate Payroll Timesheets.
Email it separately through Email Payroll Timesheets when ready. Neither saving
nor regeneration sends email, marks a replacement as submitted, or changes the
original send history. No Correct/Resubmit approval is needed to unlock editing.
Submitted and settled records remain available even for departed PAs.

The latest **definitively accepted submission** is authoritative for later
payroll reconciliation and estimated-hours carry-forward. Its weekly totals,
worked-source membership and submitted correction applications advance together
when Payroll delivery is accepted or an audited review establishes acceptance.
Saving invalidates the current PDF binding. A generated replacement is available
for review and separate dispatch. It leaves the existing financial baseline in
place, as do renewed previews, rejected delivery, same-document resends and
ordinary payslip settlement.

An unresolved first send reserves its claimed sources and correction balances
until the outcome is established. The prior accepted submission remains in force
during that hold. A confirmed rejection releases the reservation. An accepted
replacement supersedes the prior submission and retains both documents and their
evidence. Actual-hour differences use the existing rounding, completeness and
carry rules against the accepted accounting facts.

Schema 42 adds an append-only timesheet amendment/version audit, including before
and after preparation, actor/time, generated document identity, worked evidence,
weekly totals and carry applications. Original submission rows, sent attachment
bytes, earlier document identities/hashes, notes and delivery attempts remain
retained. Existing PDF publication, native private-file handling and signature
validation are reused; valid maintained signatures are rendered normally, while
original signed PDF bytes remain in document/submission history. A failed version
publication cannot create a sendable replacement.
The existing verified schema-upgrade/recovery process applies; older application
writers are refused by schema guards. No live database is migrated by tests.

Ordinary submission/settlement no longer locks preparation or generation. Actual
in-flight/indeterminate delivery, concurrent dispatch/restore, invalid values,
missing/invalid signatures and filesystem/integrity failures remain distinct
safety conditions. No automatic retry, delivery, or bypass of transport approval
has been added. Existing source-duplicate safety remains for ordinary source-based
payroll; a deliberate manual amendment uses the employer's saved totals.

Automated acceptance uses synthetic temporary databases, generated PDFs and test
transports only. Coverage includes submitted and settled amendment/regeneration,
repeated versions, original-document/submission preservation, hash consistency,
explicit email separation, blue Save/reset/confirmation and reload, future
positive/negative carry, no repeated payment, publication failure and atomic
schema-42 upgrade/old-writer refusal. Native interactive acceptance is separate.

## Historical Florence validation — 2 November 2026

- Focused amendment/eligibility/migration regressions: **10 passed**.
- Full application suite: **1,047 passed, zero failed, three intentionally ignored
  native credential tests; zero filtered out** (318.61 seconds).
- `cargo check --offline --bin direct_payment_timesheets` and `git diff --check`
  passed. Dead-code warnings remain for retained compatibility/test adapters.
- Native interactive GUI and other-platform acceptance were not claimed. No live
  payroll database, mailbox, OAuth account or native credential was accessed.

The full suite uses `cargo test --offline --bin direct_payment_timesheets --
--test-threads=2`. Test counts overlap. Initial old-protection/schema-version
expectations were revised for this behaviour/schema change; the final full run
was isolated from concurrent test-binary rebuilds so subprocess fixtures could
execute their own binary reliably. Evidence is retained locally under
`/tmp/dpt-feature2-6_gs8ofj/`.

## Subsequent cycles and new actual hours

An amended timesheet leaves later payroll cycles open to new
Hours Keeper CSVs, clipboard CSVs or completed internal shifts. Source audit events
are retained as provenance; their presence is not a veto on reconciling new actual
hours against the latest corrected authoritative timesheet. Unchanged original
facts retain the employer's saved downward correction when additional separate
shifts arrive, rather than resurrecting the old export's larger figure.

Import Hours CSV still processes the configured folder first. With a working
payroll period selected, clipboard outstanding-PA coverage is scoped to that
period. Previously imported files left in the folder do not suppress clipboard
entry in subsequent cycles. This affects the outstanding list, not acceptance of
CSV rows: valid files containing earlier actuals and current-cycle hours still
use the ordinary importer, audit, duplicate and reconciliation paths.

Regressions exercise real CSV import, the clipboard paste handler and the internal
clock-in/complete API across subsequent cycles, including positive and negative
carry, settled history, saved downward corrections and repeat preparation. They
also cover correcting the prior timesheet after a next-cycle PDF already exists:
acceptance of the replacement invalidates the stale next-cycle PDF. Next-cycle
preparation remains available, and regeneration uses the accepted replacement
and previously submitted carry. Original submissions/PDF bytes and version history
remain retained. No acquisition or regeneration sends email automatically.

Historical cross-cycle validation on Florence (2 November 2026): full application regression
**1,051 passed, zero failed, three intentionally ignored native tests**, zero
filtered out (348.03 seconds). The related Feature 2/clipboard/import/logger
selection passed **54 tests** (45.40 seconds), including a case for the former rule that deferred part of a deduction. Current Stage 2A acceptance requires applying the full signed balance in Week 1. Production code was unchanged between the full suite and that
strengthened test rerun. Cargo check and diff checks passed; test counts overlap.
Evidence: `/tmp/dpt-feature2-cycles-gckbl__m/`. All fixtures are synthetic; no live
payroll, credentials or real email submission was used. Schema remains 42.
