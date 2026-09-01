# DirectPaymentTimesheets architecture

## Shape of the application

DirectPaymentTimesheets is a local Rust desktop application. `main.rs` initialises the environment, configuration, SQLite database and application object, then starts the `eframe`/`egui` GUI. There is no server process or web API.

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

`AppConfig` is TOML/Serde data split into theme, folder, PDF, payroll and email sections. Serde's normal unknown-field handling permits obsolete keys in old files. Missing newer fields use defaults. A compatibility reconciliation preserves the legacy timesheet email body when the newer payroll template is absent.

Configured business paths may point outside the data root. `email_archive` is currently persisted without a production consumer, and Payroll Return information deliberately uses the separate payroll-information path. Path creation is deliberately late: for example, a future payroll-year directory is created only when a PDF or Payroll Return file is actually written.

## SQLite and repositories

`database.rs` creates the original schema and applies ordered migrations through `CURRENT_SCHEMA_VERSION` 19. Each repository opens/uses its own `rusqlite::Connection` to the same database path. Schema-changing work belongs in a migration; tests should exercise a newly initialised database and upgrade behaviour where relevant.

Principal persisted areas are:

- imported `timesheets` with stable row identity and import audit data;
- employer, PA and payroll-provider maintenance;
- effective-dated PA pay rates and contracted hours;
- multi-year payroll schedules;
- payroll timesheets, four weekly preparation rows and individual public-holiday rows;
- per-timesheet email status; and
- schema-19 manual adjustments, worked-item snapshots and snapshot state/digest metadata.

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

`PayrollTimesheetScreen` receives a complete operational schedule and stores a private bound identity with its key and material dates. It does not independently choose today's schedule. Its display set combines active/legacy-NULL-active PAs with PAs already represented by a payroll-timesheet row in the selected period.

Existing submitted or indeterminate records follow a persisted-only, read-only load path. Editable records follow reconciliation against imported rows, previous submitted membership and manual adjustments. Preparation baselines are compared before writes so that:

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

`email_service` composes previews and performs SMTP transport. Both production timesheets and payslips are sent from the employer to the payroll department, CC the employer and BCC the PA when available. Test functions use only configured test addresses and add test markers; payslip test email goes to the configured PA test address.

The GUI's `PendingEmailBatch` captures its kind/stage, selected PA IDs, selected schedule key, material schedule dates and operational-selection revision; it does not capture already resolved email addresses. The existing confirmation window is the production safety boundary. Final dispatch re-fetches and validates the captured schedule and rejects a changed global selection. It never substitutes today's schedule. Timesheet dispatch also preserves candidate digest/state verification; payslip dispatch requires the exact shared path-derived file. Before production payslip SMTP, the existing per-PA status row is durably marked indeterminate; SMTP failure restores unsent state, successful SMTP is followed by definitive sent state, and any crash or failed final write leaves restart-safe uncertainty that refuses automatic resend. Preview and test sends do not touch this state. Production batches currently select active and legacy-`NULL`-status PAs only, even though preparation and generation can include an inactive PA with an existing selected-period record.

## Backup and restore architecture

`BackupService` owns discovery, creation, validation and restore. The GUI supplies configured paths and displays confirmation/status only.

Creation uses `rusqlite::backup::Backup`, copies config when present and writes a human-readable identity manifest. Validation confines selection to recognised timestamp directories under the backup root, rejects unsafe links, opens the database read-only, requires one `integrity_check` result equal to `ok` case-insensitively, and checks the DirectPaymentTimesheets schema/version. Restore first creates a safety backup, then uses SQLite's backup API into the live database. Restored config is optional and restart is required.

## UI structure

The Dashboard coordinates imports, operational period selection, generation, email and navigation. Dedicated screens cover employer, PA, payroll provider/settings, Payroll Timesheet Preparation, Application Settings and Email Settings. Long preparation/settings content uses vertical scroll areas. View Payroll Schedule selects an imported year without changing application state beyond the view.

## Testing boundaries

Unit and integration-style module tests use temporary directories and SQLite databases. Safety-critical tests cover migration 19, schedule transactions/resolution, period-bound operations, candidate state transitions/digests, preparation read-only/stale-save behaviour, effective dates, return paths, backup/restore and historical PA eligibility. Tests must not use the real data root.

## Deliberate boundaries

The current architecture does not provide authentication/multi-user coordination, an overtime engine, P60-specific import, scheduled/cloud backups, retention cleanup or arbitrary SQLite restore. These are limitations, not partially implemented promises.
