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
