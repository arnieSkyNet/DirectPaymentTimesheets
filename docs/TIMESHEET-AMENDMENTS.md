# Timesheet amendments — v1.0.7

Open the original payroll period in Payroll Preparation, select the PA, edit the
hours and click Save. The enabled Save button is blue while the draft differs
from its saved values; successful saving restores normal styling and displays a
confirmation. Then generate a replacement PDF through Generate Payroll Timesheets.
Email it separately through Email Payroll Timesheets when ready. Neither saving
nor regeneration sends email, marks a replacement as submitted, or changes the
original send history. No Correct/Resubmit approval is needed to unlock editing.
Submitted and settled records remain available even for departed PAs.

The latest **saved and successfully regenerated** timesheet is authoritative for
later payroll reconciliation and estimated-hours carry-forward. Saving without
regeneration leaves the preceding generated/submitted baseline authoritative and
invalidates the current PDF binding, preventing an old PDF being sent as the edit.
An unchanged historical CSV cannot overwrite manually corrected hours. Material
source changes recorded subsequently are compared with the corrected version;
the existing rounding, carry-forward, consumption and nonnegative-payable rules
apply to the difference. Repeated calculations do not add that difference again.
A newer corrected generated version supersedes older baseline differences rather
than summing historical estimates. No separate financial submission state or
amendment-specific financial correction ledger has been introduced.

Schema 42 adds an append-only timesheet amendment/version audit, including before
and after preparation, actor/time, generated document identity, worked evidence,
weekly totals and carry applications. Original submission rows, sent attachment
bytes, earlier document identities/hashes, notes and delivery attempts remain
retained. Existing PDF publication, native private-file handling and signature
validation are reused; valid maintained signatures are rendered normally, while
original signed PDF bytes remain in document/submission history. A failed version
publication cannot promote a new authoritative baseline or permit a stale send.
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

## Florence validation — 2 November 2026

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

An amended/regenerated timesheet does not close later payroll cycles to new
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
the stale PDF must be regenerated, but next-cycle preparation and generation stay
available. The corrected baseline replaces older values; applied carry and source
hours must not be paid twice. Original submissions/PDF bytes and version history
remain retained. No acquisition or regeneration sends email automatically.

Cross-cycle validation on Florence (2 November 2026): full application regression
**1,051 passed, zero failed, three intentionally ignored native tests**, zero
filtered out (348.03 seconds). The related Feature 2/clipboard/import/logger
selection passed **54 tests** (45.40 seconds), including a strengthened negative
carry-capacity case: zero-payable preparation/generation remains available, only
the remaining deduction reaches the following cycle, and new actual hours are
still accepted. Production code was unchanged between the full suite and that
strengthened test rerun. Cargo check and diff checks passed; test counts overlap.
Evidence: `/tmp/dpt-feature2-cycles-gckbl__m/`. All fixtures are synthetic; no live
payroll, credentials or real email submission was used. Schema remains 42.
