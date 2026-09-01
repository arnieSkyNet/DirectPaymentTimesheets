# DirectPaymentTimesheets

DirectPaymentTimesheets is a local desktop application for administering UK Direct Payment Personal Assistant timesheets and the four-week payroll-provider workflow. It imports externally recorded work, prepares payroll timesheets, generates the provider PDF, sends timesheets and payslips, imports Payroll Returns, and preserves the evidence represented by submitted payroll.

The current pre-release is version `0.0.10` with SQLite schema version 19. It is a working application under active development, not an installer-packaged or general-purpose payroll product.

## Technology

- Rust 2021
- `eframe`/`egui` desktop UI
- SQLite through bundled `rusqlite`
- `printpdf` document generation
- SMTP through `lettre`
- PDF text extraction and ZIP processing for provider files

## Implemented functionality

### Records and settings

- Employer, payroll-provider and Personal Assistant maintenance.
- Effective-dated PA pay-rate history, including employer top-up.
- Effective-dated contracted weekly hours.
- Application themes, folder/font settings, payroll settings and email settings.
- Dark, light, system, soft light/dark, blue and accessible high-contrast themes.

### Work import and payroll preparation

- CSV import with duplicate detection, import audit and timestamped source archive.
- Provider Payroll Prep Sheet PDF import with validation and atomic per-year replacement.
- Coexistence of multiple imported payroll years, including early import of a future year.
- Schedule-driven payroll rollover rather than hard-coded calendar-year selection.
- A Dashboard operational payroll-period selector for deliberate current, historical or future work.
- Payroll Timesheet Preparation bound to the selected period, including manual Hours Worked corrections, leave, sick/SSP, public-holiday and mileage values.
- Read-only protection for submitted or indeterminate preparation records and invalidation of stale generated candidates when represented data changes.

Imported `worked_minutes` is parsed from the CSV Worked Hours field. It is not recalculated from Start Time/End Time and is not changed by the persisted rounding setting. CSV rate and amount fields are retained as imported data but are not the authoritative payroll rate; maintained PA rate history is authoritative.

### Payroll PDFs and historical evidence

- Four-week provider PDF generation using configured fonts and sizes.
- Per-shift effective-dated rate allocation retained internally without printing pay rates on the provider form.
- Per-week effective-dated contracted-hours presentation.
- Cross-payroll-year previous-cycle late-shift reconciliation.
- Persistent manual adjustments and exact worked-item snapshots.
- Candidate PDFs associated with SHA-256 digests before production sending.
- Immutable submitted baselines and protected indeterminate-delivery state.
- Historical inactive PA generation when that PA already has a selected-period payroll record.

### Email and Payroll Returns

- Timesheet and payslip preview, test and production workflows.
- Production batches bound to the exact operational payroll period captured at confirmation.
- Production timesheet and payslip routing from the employer to the payroll department, CC employer and BCC PA when available.
- Test routing through configured test addresses; payslip test email targets the configured PA test address.
- Explicit Payroll Return period selection before choosing a ZIP file.
- UK PAYE tax-week filename calculation and payroll-year-aware payslip/information folders.
- Collision-safe storage of non-payslip payroll information.

### Backup and restore

- Manual consistent SQLite backups with optional `config.toml` and a human-readable manifest.
- Discovery and validation of application-created backups.
- Integrity/schema checks and a mandatory pre-restore safety backup.
- SQLite-safe restore with optional configuration restore and required application restart.

## Build and run

Install a current Rust toolchain, then from the repository root run:

```bash
cargo check
cargo test
cargo run
```

No packaged installer is currently provided. Native desktop/TLS build prerequisites may depend on the operating system and Rust toolchain installation.

## Runtime data and configuration

The default application data root is:

```text
~/.directpaymenttimesheets/
```

Set `DIRECTPAYMENTTIMESHEETS_HOME` before launching to use a different data root, which is strongly recommended for disposable development/manual-test data.

The application root contains `database.sqlite`, `config.toml` and internal import, archive, backup, log, template and cache directories. Application Settings also configures external business folders for CSV import, generated PDFs, payslips and returned payroll information. These paths can be outside the application root and may contain sensitive payroll data. Inspect them before running a workflow that writes files.

The configured `email_archive` path is currently persisted but has no production consumer. Returned payroll information uses the separate payroll-information folder.

## CSV import testing

To test CSV import safely, prepare a parser-compatible CSV containing only synthetic data, then:

1. use a disposable `DIRECTPAYMENTTIMESHEETS_HOME`;
2. configure the CSV import folder in Application Settings;
3. copy the test CSV into that folder; and
4. use **Import CSV** on the Dashboard.

The test CSV's PA name must correspond to a maintained PA if downstream payroll work is required. Its rate and amount columns represent the external format only and are not authoritative for generated payroll.

The files `data/sig-Employer.jpg` and `data/sig-PA.jpg` are deliberately blank white, project-created test signature images. They contain no real signatures or personal data.

## Payroll-period selections

Three selections are intentionally independent:

- the Dashboard operational period drives preparation, generation and email operations;
- Payroll Return import selects the period to which a returned ZIP belongs; and
- View Payroll Schedule selects an imported payroll year for display only.

User-facing labels use Payroll Week, the four-week date range and pay date. Internal cycle numbers remain database identity rather than filename Payroll Week.

## Current limitations

- This is a pre-release, single-user local desktop application.
- There is no packaged installer, authentication or multi-user coordination.
- Persisted frequency, rounding, workweek and overtime choices are not downstream configurable calculation rules; no overtime engine is implemented.
- Payroll Prep Sheet PDF import is implemented; DOCX import is recognised but not implemented.
- Production email batches currently include active and legacy-`NULL`-status PAs only, even though preparation/generation can retain an inactive historical PA.
- There is no P60-specific Payroll Return processing.
- Backup/restore has no scheduling, retention cleanup, compression or cloud integration.
- Restore accepts only recognised DirectPaymentTimesheets backup directories, not arbitrary SQLite files.

## Documentation

- [Project state](docs/PROJECT_STATE.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Domain rules](docs/DOMAIN.md)
- [Development guide](docs/DEVELOPMENT.md)
- [Conceptual data model](docs/DATA-MODEL.md)
- [Database schema 19](docs/DATABASE-SCHEMA.md)

## Licence

DirectPaymentTimesheets is free software licensed under the [GNU General Public License version 3 or later](LICENSE), identified by the SPDX expression `GPL-3.0-or-later`. You may redistribute and modify the source under that licence. Charging for copies or genuine services is permitted, but recipients retain the GPL freedoms and distributed modified versions remain subject to the GPL's requirements.

Third-party dependencies and bundled font components remain subject to their respective terms; see [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).
