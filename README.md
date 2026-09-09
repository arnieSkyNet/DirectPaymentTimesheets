# DirectPaymentTimesheets

DirectPaymentTimesheets is a local desktop application for administering UK Direct Payment Personal Assistant timesheets and the four-week payroll-provider workflow. It imports externally recorded work, prepares payroll timesheets, generates the provider PDF, sends timesheets and payslips, imports Payroll Returns, and preserves the evidence represented by submitted payroll.

The current pre-release is version `0.0.13` with SQLite schema version 29. It is a working application under active development, not an installer-packaged or general-purpose payroll product.

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
- Built-in exact-minute clock-in/out and audited completed-shift corrections, kept separate from imported work.
- Provider Payroll Prep Sheet PDF import with validation and atomic per-year replacement.
- Coexistence of multiple imported payroll years, including early import of a future year.
- Schedule-driven payroll rollover rather than hard-coded calendar-year selection.
- A Dashboard operational payroll-period selector for deliberate current, historical or future work.
- Payroll Timesheet Preparation bound to the selected period, including manual Hours Worked corrections, leave, sick/SSP, public-holiday and mileage values.
- Read-only protection for submitted or indeterminate preparation records and invalidation of stale generated candidates when represented data changes.

Imported `worked_minutes` is parsed from the CSV Worked Hours field. It is not recalculated from Start Time/End Time and is not changed by the persisted rounding setting. For open payroll, selected evidence from both sources is converted to payable minutes using Payroll Settings rounding; original source minutes remain unchanged. CSV rate and amount fields are retained as imported data but are not the authoritative payroll rate; maintained PA rate history is authoritative.

### Payroll PDFs and historical evidence

- Four-week provider PDF generation using configured fonts and sizes.
- Per-shift effective-dated rate allocation retained internally without printing pay rates on the provider form.
- Per-week effective-dated contracted-hours presentation.
- Dated outstanding work across payroll periods, with audited signed carry-forward reconciliation.
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

## Install, build and run

DirectPaymentTimesheets is currently a pre-release application. Building from source is presently the supported way to run it.

### Linux

#### Build and run from source

You will need:

- Git
- A current Rust toolchain, including Cargo
- The native development libraries required by the application's desktop and TLS dependencies

If Rust and Cargo are not already installed, see the official Rust installation instructions:

https://www.rust-lang.org/tools/install

Clone the DirectPaymentTimesheets source from GitHub:

```bash
git clone https://github.com/ArnieSkyNet/DirectPaymentTimesheets.git
cd DirectPaymentTimesheets
```

Check that the project builds and passes its tests:

```bash
cargo check
cargo test
```

Run the application:

```bash
cargo run
```

#### Debian/Ubuntu `.deb` package

Not yet available. A packaged `.deb` release is planned for a future version.

#### AppImage

Not yet available. An AppImage release is planned for a future version.

### Windows

Windows installation instructions and packaged releases are planned for a future version.

### macOS

macOS installation instructions and packaged releases are planned for a future version.

## Runtime data and configuration

The default application data root is:

```text
~/.directpaymenttimesheets/
```

Set `DIRECTPAYMENTTIMESHEETS_HOME` before launching to use a different data root, which is strongly recommended for disposable development/manual-test data.

The application root contains `database.sqlite`, `config.toml` and internal import, archive, backup, log, template and cache directories. Application Settings also configures external business folders for CSV import, generated PDFs, payslips and returned payroll information. These paths can be outside the application root and may contain sensitive payroll data. Inspect them before running a workflow that writes files.

New configurations use provider-neutral paths beneath `~/Documents/DirectPaymentTimesheets/` for those business folders. The `~` prefix is expanded to the current user's home directory at runtime. Existing configured paths are preserved and remain editable in Application Settings.

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
- Persisted frequency, workweek and overtime choices are not downstream configurable calculation rules; no overtime engine is implemented.
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
- [Database schema](docs/DATABASE-SCHEMA.md)

## Licence

DirectPaymentTimesheets is free software licensed under the [GNU General Public License version 3 or later](LICENSE), identified by the SPDX expression `GPL-3.0-or-later`. You may redistribute and modify the source under that licence. Charging for copies or genuine services is permitted, but recipients retain the GPL freedoms and distributed modified versions remain subject to the GPL's requirements.

Third-party dependencies and bundled font components remain subject to their respective terms; see [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).

Payroll Settings includes two annual-leave settings groups: contracted-hours annual leave (recurring DD/MM boundary and statutory weeks) and variable-hours annual leave (recurring DD/MM boundary and accrual percentage). Save Payroll Settings saves all four together. Personal Assistant Maintenance now provides read-only Annual Leave guidance with a compact April-to-March leave-year selector, entitlement/accrued, taken, remaining, Calculated to (Variable/mixed years), and an auditable Details view. Payroll remains definitive; negative remaining hours are retained.

Personal Assistants have an optional Leaving date, stored as canonical `DD/MM/YYYY`. Schema 27 adds nullable `personal_assistants.leaving_date` without backfilling dates or altering historical data. A supplied date must be real and not precede Start date. Stored Active/Inactive status is never changed automatically. Ordinary selected-period payroll inclusion requires Active (or legacy unset status) and inclusive employment-date overlap with the four-week period. Existing selected-period preparation records remain included regardless of status or employment dates. Annual-leave guidance applies Start and Leaving dates inclusively without changing employment status.


### Payroll evidence and reconciliation

Completed Hours Shift records now feed payroll alongside Hours Keeper imports, while both original sources remain separate. Possible overlapping shifts use one consolidated, audited duplicate resolver. Preparation preserves sent submissions, offers contextual correction/resubmission or carry-forward, and treats each PA's definitively Sent payslip as settlement. Historical payment uncertainty and aggregate negative estimate corrections require explicit contextual reviews. Signed corrections persist, never make payable worked hours negative, and appear on PDFs only through final bold totals plus a subordinate `(Info only +/-X.XX hours)` line.

See [Payroll evidence and reconciliation](docs/PAYROLL-EVIDENCE.md) for storage, safeguards, limitations of historical evidence and the manual verification sequence.
