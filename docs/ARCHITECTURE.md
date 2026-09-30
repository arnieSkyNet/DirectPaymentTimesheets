# DirectPaymentTimesheets architecture

## Shape of the application

DirectPaymentTimesheets is a local Rust desktop application. `main.rs` wires modules and calls `application::run`. Startup orchestration lives in `application.rs`, `Application::initialise` and context/environment code, including database initialisation and `eframe`/`egui` launch. There is no server process or web API.

The main layers are:

```text
egui screens and dialogs
        |
GUI/application orchestration
        |
domain services and file/PDF/email adapters
        |
repositories
        |
SQLite + configured filesystem locations
```

The GUI owns navigation and stable selection state. `Application` exposes repositories and service operations without putting SQL, SMTP, PDF internals or backup implementation directly in widgets.

## Runtime environment

`AppEnvironment` uses `DIRECTPAYMENTTIMESHEETS_HOME` when set, otherwise `~/.directpaymenttimesheets`. It creates the internal data, import, archive, backup, log, template and cache directories and locates `database.sqlite`. `config.toml` is stored in the data root.

`AppConfig` is TOML/Serde data split into theme, folder, PDF, payroll and email sections. Serde's normal unknown-field handling permits obsolete keys in old files. Missing newer fields use defaults. Annual-leave rule values are an exception to TOML settings: they live in the singleton SQLite `annual_leave_settings` table. A compatibility reconciliation preserves the legacy timesheet email body when the newer payroll template is absent.

Configured business paths may point outside the data root. `email_archive` is currently persisted without a production consumer, and Payroll Return information deliberately uses the separate payroll-information path. Saving configuration ensures configured business roots exist. Schedule-derived path creation is deliberately late: for example, a future payroll-year directory is created only when a PDF or Payroll Return file is actually written.

## SQLite and repositories

`database.rs` creates the original schema and applies ordered migrations through `CURRENT_SCHEMA_VERSION` 35. Each repository opens/uses its own `rusqlite::Connection` to the same database path. Schema-changing work belongs in a migration; tests exercise fresh initialisation and upgrades, including preservation of existing history.

Principal persisted areas are:

- imported `timesheets` with stable row identity and import audit data;
- employer, PA and payroll-provider maintenance;
- effective-dated PA pay rates and contracted hours;
- multi-year payroll schedules;
- payroll timesheets, four weekly preparation rows and individual public-holiday rows;
- per-timesheet email status;
- schema-19 manual adjustments, worked-item snapshots and snapshot state/digest metadata;
- dated annual leave, SQLite annual-leave settings and structured sickness periods;
- duplicate decisions, immutable submissions, corrections and verified legacy settlement; and
- schema31 cycle-independent P60/P45 documents, schema33 delivery-history reconciliation, schema34 file-cleanup journalling, and schema35 superseded documents/current payslip revisions.

Repositories isolate queries and identity rules. Important compound identities use payroll year plus internal cycle number, not row ID alone. Dates are currently stored as text in existing formats, so repository methods parse and compare calendar dates explicitly where ordering must be chronological.

## Schedule architecture

`PayrollScheduleRepository` owns schedule import/replacement and resolution:

- validated 13-cycle imports replace only one payroll year in a transaction;
- all imported years coexist;
- `resolve_for_date` searches inclusive 28-day windows across every year and rejects gaps/ambiguity;
- lookup by `(payroll_year, cycle_number)` re-fetches a complete schedule before operations; and
- predecessor resolution uses dates and an exact 28-day boundary, allowing cross-year cycle 13 to cycle 1 rollover.

Payroll Prep Sheet PDF extraction is implemented; DOCX is recognised but explicitly not implemented. A schedule's `payslips_sent` state is retained when the same cycle is re-imported with unchanged material dates. A changed cycle receives reset sent state when replacement is safe, while changed dates are refused if prepared payroll history already exists for that year.

The Dashboard operational selector stores the stable `(payroll_year, cycle_number)` key. A complete schedule is re-fetched before generation, preview/test email, status and other operational actions. Production batches additionally capture the first-week and pay dates to detect a materially changed re-import.

Payroll Return and View Payroll Schedule intentionally have independent selector state. Viewing/importing a future year cannot change the operational period.

## Payroll preparation architecture

`PayrollTimesheetScreen` receives a complete operational schedule and stores a private bound identity with its key and material dates. It does not independently choose today's schedule. Its display set combines inclusive Start/Leaving date overlap independent of current status with PAs already represented by a payroll-timesheet row in the selected period, including records outside those dates.

Existing submitted or indeterminate records follow a persisted-only, read-only load path. Editable records follow the shared source-labelled reconciliation service against effective imported evidence, completed direct shifts, duplicate decisions, submission membership, outstanding corrections and manual adjustments. Preparation baselines are compared before writes so that:

- opening unchanged data does not invalidate a candidate or update timestamps;
- a material reconciliation change invalidates the candidate before persistence;
- save changes to weeks, manual hours, leave, sick/SSP, holiday rows or mileage invalidate a stale PDF candidate first; and
- selection/schedule/state validation happens before any save mutation.

The screen retains `Local::now()` only for persisted timestamps, not payroll-period selection.

## Worked-item and effective-date model

`pay_rate_allocation` builds a reconciled integer-minute representation used by PDF preparation and schema-19 snapshots. Imported shifts retain TimesheetEntry ID, work/start date and minutes, and resolve the newest pay-rate row effective at that date. Manual adjustments are separate persisted items; imported source rows are never rewritten.

CSV `worked_minutes` comes directly from the imported worked-duration field; import does not recalculate it from start/end values or apply the persisted rounding setting. Successful source files are copied to `archive/YYYY/MM/` with timestamped names and recorded in `import_audit`.

Snapshot items distinguish imported work, late previous-cycle work, manual adjustments and opaque legacy aggregates. Ordinary allocated items require rate evidence. Legacy pre-schema-19 aggregates allow nullable work/rate fields precisely because the original details cannot be reconstructed.

Late-shift detection uses only a successfully submitted snapshot as membership evidence. Generated candidates never establish that baseline. Contracted hours use a separate effective-dated lookup per week commencing date and do not participate in pay or worked-minute reconciliation.

## PDF candidate lifecycle

`PdfGenerator` renders the provider form from reconciled preparation data and configured fonts. Weekly Hours Worked contains only the final hours value; rate portions stay internal.

`payroll_snapshot_service::publish_candidate` coordinates output safely:

1. create the resolved parent directory if necessary;
2. generate to a temporary path;
3. persist the candidate snapshot/state and SHA-256 digest association;
4. reconcile required database aggregates; and
5. publish/rename the final file.

Failures clean up or invalidate candidate state so a new PDF is not silently paired with old evidence. Production send verifies the file path/digest, sends it, then freezes the submitted baseline. Transport failure keeps the candidate mutable. Failure to persist state after possible successful transport creates protected `indeterminate` state.

## File naming and storage

`payroll_file_naming` is shared by producers and consumers. PAYE week derives from `pay_date` and the 6 April tax-year boundary. Period code uses the first-week month plus PAYE week. Timesheet and payslip generation, attachment lookup and subject substitution therefore agree.

Payroll-year directory normalisation recognises a final `YYYY to YYYY` component. Such a component is replaced for another schedule year rather than nested. Generic PDF output roots intentionally remain flat; payslip and payroll-information storage is year-aware. Unknown Payroll Return information files go to the payroll-information folder with collision-safe names, not the email archive.

## Email architecture

`email_service` composes previews and performs SMTP transport. Production timesheets retain To payroll department, CC employer and BCC PA when available. The dedicated payslip composer uses To PA, BCC employer, no CC and no payroll department recipient; previews and production share it. Ordinary and combined payslip subjects use `Payslip for Week <PAYE week>` from the captured schedule; standalone supplement subjects use `Payroll documents - {Personal Assistant Name}` with neutral body wording. MIME attachment names are canonical names derived from structured type/year/PA/schedule data, never stored revision/hash basenames. Test functions use only configured test addresses and add test markers; payslip test email uses the same canonical filename logic and goes to the configured PA test address.

The GUI's `PendingEmailBatch` captures its kind/stage, selected PA IDs, an optional selected schedule key, material schedule dates and operational-selection revision; it does not capture already resolved email addresses. The existing confirmation window is the production safety boundary. Final dispatch re-fetches and validates the captured schedule and rejects a changed global selection. It never substitutes today's schedule. Timesheet dispatch also preserves candidate digest/state verification; payslip/document dispatch uses the shared selector for an optional ordinary payslip plus reconciled Needs sending P60/P45; eligible standalone documents do not require an ordinary payslip. Before production payslip/document SMTP, selected ordinary-payslip status and document-ID states are durably marked indeterminate; SMTP failure restores unsent state, successful SMTP is followed by definitive sent state, and any crash or failed final write leaves restart-safe uncertainty that refuses automatic resend. Preview and test sends do not touch this state. PA payroll email also includes PAs with unsent imported P60/P45, regardless of employment eligibility. Their document IDs and delivery states are independent of the selected cycle. A batch without a selected schedule can send these documents alone.

## Backup and restore architecture

`BackupService` owns discovery, creation, validation and restore. The GUI supplies configured paths and displays confirmation/status only.

Creation uses `rusqlite::backup::Backup`, copies config when present and writes a human-readable identity manifest. Validation confines selection to recognised timestamp directories under the backup root, rejects unsafe links, opens the database read-only, requires one `integrity_check` result equal to `ok` case-insensitively, and checks the DirectPaymentTimesheets schema/version. Restore first creates a safety backup, then uses SQLite's backup API into the live database. Restored config is optional and restart is required.

## UI structure

The Dashboard coordinates imports, operational period selection, generation, email and navigation. Dedicated screens cover employer, PA, payroll provider/settings, Payroll Timesheet Preparation, Application Settings and Email Settings. Long preparation/settings content uses vertical scroll areas. View Payroll Schedule selects an imported year without changing application state beyond the view.

## Testing boundaries

Unit and integration-style module tests use temporary directories and SQLite databases. Safety-critical tests cover migration 19, schedule transactions/resolution, period-bound operations, candidate state transitions/digests, preparation read-only/stale-save behaviour, effective dates, return paths, backup/restore and historical PA eligibility. Tests must not use the real data root.

## Deliberate boundaries

The current architecture does not provide authentication/multi-user coordination, an overtime engine, scheduled/cloud backups, retention cleanup or arbitrary SQLite restore. These are limitations, not partially implemented promises.


## Schema-28 reconciliation extension

`payroll_evidence` supplies the source model, transitive overlap grouping, versioned duplicate decisions, immutable submission history and signed correction ledger. `duplicate_ui` and `review_ui` provide consolidated selection and contextual review over that logic. The existing snapshot candidate/submitted/indeterminate model remains the current payroll-timesheet preparation/attachment slot; append-only submission tables preserve earlier successful sends across explicit resubmission. This is separate from schema35 ordinary payslip document revisions. Final settlement uses the existing per-PA payslip delivery rule. See [PAYROLL-EVIDENCE.md](PAYROLL-EVIDENCE.md) for the complete implemented workflow and audit model.


## Payroll document import and delivery

Import Payroll Documents opens and classifies the source before requesting a period. Each ordinary payslip is resolved independently: an explicit filename tax year and exact provider “for Week N for” form can identify a single stored schedule through the existing PAYE-week calculation. Otherwise, choices are restricted by the known year and/or week. For yearless documents, only periods whose first week has started are plausible; a future recurring Week 50 is not evidence of the year of a returned payslip. A chooser appears only for unresolved payslips with plausible stored candidates. The displayed choices must be applicable to all payslips needing that choice; incompatible groups require separate imports. Exact matches and archival documents are independent of this choice.

When there is no plausible stored cycle, an ordinary payslip is filed as archival evidence, preserving its useful provider filename after prefix stripping. It creates no schedule, cycle status, document-delivery record or settlement. With an explicit tax year it goes to `<Payslip base>/<YYYY to YYYY>/PA <id>/<cleaned filename>`; without one it goes to `<Payslip base>/PA <id>/<cleaned filename>`. The base removes any trailing configured `YYYY to YYYY` component. These PA areas contain no cycle directory and are outside the flat canonical email lookup `<Payslip base>/<schedule year>/Payslip for Week N for <name>.pdf`. No automatic emailing or promotion of archival files is implemented. The import result reports separate archival counts and this email limitation.

ZIPs are checked for unsafe paths/names, symlinks or non-regular entries, encryption, entry-count/size/total-expansion/compression-ratio limits and conflicting destinations. An invalid archive container is refused before import. For a valid container, entries are classified and validated independently, staged, and published without clobbering; a failure for one item is reported while other valid items can succeed. The UI reports per-category counts, per-entry failures, and recovery paths where publication succeeded but a later registration or prep-sheet parse failed. This mixed workflow is deliberately not one cross-resource transaction.

PA matching is token/boundary based after case and separator normalisation, and requires exactly one maintained PA. Ambiguous or incomplete ownership is rejected rather than guessed. P45/P60 must be PDFs matching exactly one PA and one type. A non-PDF containing a PA name is rejected rather than stored as shared information. P30, bank-transfer, quarter-end and other non-PA information is shared information, not a PA supplement.

P60/P45 continue using `imported_payroll_documents` on the email repository's connection. Active supplements are stored directly in their own payroll-year directory, or the yearless base when no year is known; they never create PA-ID directories. Inactive supplements go directly in that directory's shared `Archived` folder. Filenames retain metadata from the P45/P60 heading, remove its unmatched closing bracket and supplied PA-name spans, and append the maintained full name once; numeric revision suffixes remain metadata. Their document IDs and delivery states remain cycle-independent. A P60's year, a prep sheet's year, the current date or Dashboard selection is never borrowed to date another document. Information files use their own explicit filename tax year when available, otherwise the configured information root. Prep sheets use the existing parser and update their own schedule year. P30, bank-transfer slips, memos and general files never enter PA email. Collisions never overwrite different bytes, and `[1]` remains filename data.


Mixed bundles protect the ordinary payslip status and specific P60/P45 IDs in one SQLite transaction before SMTP. SMTP failure restores unsent states together; success marks them sent together. Failed final persistence leaves indeterminate states that block automatic resend. Already-sent ordinary payslips are excluded from later document emails. Schedule completion and payroll settlement continue to consult only ordinary payslip status. Standalone P60/P45 bundles bypass the configured subject template and use neutral subject/body wording independent of Dashboard dates; ordinary and combined bundles use `Payslip for Week <week>` from the captured schedule and the configured payslip body. Email Payroll Timesheets retains its configurable timesheet subject. For payroll attachments, MIME filenames are generated from structured document type/year and PA/schedule identity; internal revision/hash storage basenames are never used as recipient-visible names. P45 imports never update employment data.

Information imports reuse identical content at the original or any existing numbered destination, including beyond suffix gaps; different bytes get a collision-safe filename. Reimporting that variant reuses it rather than creating another copy. `[1]` is ordinary filename content, not a collision suffix to strip.

Individual files and mixed ZIPs use the same staging and no-clobber publication. Results distinguish imported, already-present, archival and failed items. Prep sheets are stored, then parsed through the existing per-year transactional schedule importer; parsing failure is reported separately from successful file storage. The overall mixed workflow is not one filesystem/database transaction. Published files may remain after registration/parser failure, with paths and errors reported for recovery.

The test path uses this same read-only selection and validation followed only by test transport, never production transitions. See [Email rules](DOMAIN.md#email) for recipient, composition and state guarantees. User-triggered GitHub update checking in `update_check` reports versions and installation guidance; it does not download or install updates.


## Inactive PA filing and registered supplement repair

A saved Active-to-Inactive transition initiates a complete safely identified PA document filing plan. It covers registered P45/P60s (including superseded versions), ordinary and unassociated payslips, durable payslip revisions, current timesheet PDFs, and historical on-disk timesheet evidence linked by snapshot/submission records. Exact application-generated timesheet filenames with unique maintained ownership are also recognised. Existing payroll-year metadata/directories determine the year; unregistered generated timesheets without reliable year evidence use the PDF base's `Archived` directory. Documents are placed directly in each document root's year-level `Archived`, never inside a PA-name or PA-number subdirectory.

Shared P30/information/prep sheets, generic CSV archives, arbitrary unregistered files, external signature assets and database-embedded evidence are not moved. A shared, altered or out-of-root registered timesheet refuses filing. Historical records whose bytes were replaced by later generated bytes retain their original digest and embedded evidence; path relocation changes location metadata only. Missing historical files are not recreated. Ambiguous ownership is not guessed.

The schema34 copy/transaction/cleanup journal now covers both payslip and PDF roots. Verified no-clobber publication precedes the transaction commit; PA status, supplement paths, payslip revision paths, timesheet snapshot/submission paths and cleanup rows commit together. Hashes, submission facts, history declarations, sent timestamps and embedded bytes do not change. Originals are removed only after commit and re-verification. Failed cleanup remains journalled and retryable; failed rollback cleanup reports extra copies. Unix directories are synced. Filesystems must support hard links within the destination directory; Windows directory fsync is not performed. Power loss cannot provide perfect cross-resource atomicity.

Saving unrelated edits to an already inactive PA does not initiate another archival pass, but may finish committed cleanup. Repair normalises verified registered supplement names/legacy paths and finishes journalled cleanup; it does not initiate new archival. Name edits retain current locations unless accompanied by Active-to-Inactive transition. Reactivation never restores files. Incoming new or corrected documents for an already inactive PA are filed directly in Archived without changing employment status.

Timesheet preview/send reads the durable snapshot path, including after archival or later regeneration. Revised payslips use their durable current path/hash; legacy ordinary payslips retain active/Archived canonical lookup. Unassociated evidence stays excluded from automatic payslip email.

## Explicit payroll document corrections (schema35)

Personal Assistant Maintenance → **Replace payroll document…** lists the saved PA's current supplements and available ordinary payslip periods. The user selects the intended existing document, chooses an individual PDF, reviews the PA/type/year/cycle, old delivery evidence, both paths and SHA-256 hashes, then deliberately confirms. ZIP corrections must first be extracted for individual review. Filename PA/type and any supplied year/week must agree; missing metadata requires the user's explicit identity confirmation. The application does not infer identity from PDF contents.

Import never overwrites changed bytes. A supplement variant with the same PA/type/year (or uncertain year) is refused and directed to replacement instead of creating an accidental second current document. P45 and P60 remain independent. Import refuses revised ordinary identities and directs them to the same workflow. Legacy active/Archived duplicate detection prevents correction imports from bypassing conflicts after reactivation.

`payroll_replacement` takes a write transaction, rechecks the reviewed source, current identity, old digest and delivery state, publishes a synced immutable hash-named revision without clobbering, then commits the current identity switch. It retains old bytes in place, so no deletion journal is necessary for replacement itself. A failed transaction removes only this attempt's new publication; a pre-commit crash may leave an extra verified copy that retry can reuse. Conflicts are refused.

Ordinary legacy evidence is adopted lazily on first explicit replacement: the old PDF path/hash and cycle delivery marker are captured in `payslip_revisions`; the new revision is current and unsent. The cycle's sent marker and schedule completion flag are cleared for the new bytes, while the old revision retains its original marker, including indeterminate. Subsequent delivery transitions update the current revision and cycle together. A stale selected attachment cannot send after replacement, and current revision hashes are checked before delivery.

Supplements retain their original registration and history; `superseded_by` links to the new current registration. Every corrected supplement starts with `history_state=unknown` and NULL `sent_at`. Nothing converts an unknown P45 to sent, external or needs_sending. The normal history-review UI must be used before the corrected supplement can be emailed. Superseded documents are excluded from automatic selection and reconciliation but remain available to filing and historical inspection. All prior rows are preserved by migration without filesystem access.
