# DirectPaymentTimesheets

DirectPaymentTimesheets is a local desktop application for administering UK Direct Payment Personal Assistant timesheets and the four-week payroll-provider workflow. It imports externally recorded work, prepares payroll timesheets, generates the provider PDF, sends timesheets and payslips, imports payroll documents, and preserves the evidence represented by submitted payroll.

The forthcoming **1.0.2** release uses SQLite schema **32**. It is a single-user local desktop application for the documented Direct Payment payroll workflow, not a general-purpose payroll product.

## Download

**v1.0.2 is not yet published. Final package rehearsal and acceptance checks are still in progress.** The links below are the intended public release URLs and may show “Not Found” until publication; they are not evidence that an asset is available or approved.

When published, use the [official v1.0.2 release page](https://github.com/ArnieSkyNet/DirectPaymentTimesheets/releases/tag/v1.0.2). [All published releases](https://github.com/ArnieSkyNet/DirectPaymentTimesheets/releases) remain available separately.

Choose the row for your computer **and installed operating system**. You only need one package: on Linux, choose either the installer (`.deb`) or the portable AppImage, not both.

| Computer / operating system | Intended v1.0.2 download |
|---|---|
| Linux on a 64-bit Intel or AMD PC — Debian installer (`amd64`) | [direct-payment-timesheets_1.0.2_amd64.deb](https://github.com/ArnieSkyNet/DirectPaymentTimesheets/releases/download/v1.0.2/direct-payment-timesheets_1.0.2_amd64.deb) |
| Linux on a 64-bit Intel or AMD PC — portable (`x86_64`) | [DirectPaymentTimesheets-1.0.2-linux-x86_64.AppImage](https://github.com/ArnieSkyNet/DirectPaymentTimesheets/releases/download/v1.0.2/DirectPaymentTimesheets-1.0.2-linux-x86_64.AppImage) |
| Linux / Raspberry Pi OS, ARM32 — Debian installer (`armhf`, ARMv7) | [direct-payment-timesheets_1.0.2_armhf.deb](https://github.com/ArnieSkyNet/DirectPaymentTimesheets/releases/download/v1.0.2/direct-payment-timesheets_1.0.2_armhf.deb) |
| Linux / Raspberry Pi OS, ARM32 — portable (`armhf`, ARMv7) | [DirectPaymentTimesheets-1.0.2-linux-armhf.AppImage](https://github.com/ArnieSkyNet/DirectPaymentTimesheets/releases/download/v1.0.2/DirectPaymentTimesheets-1.0.2-linux-armhf.AppImage) |
| Linux / Raspberry Pi OS, ARM64 — Debian installer (`arm64`) | [direct-payment-timesheets_1.0.2_arm64.deb](https://github.com/ArnieSkyNet/DirectPaymentTimesheets/releases/download/v1.0.2/direct-payment-timesheets_1.0.2_arm64.deb) |
| Linux / Raspberry Pi OS, ARM64 — portable (`aarch64`) | [DirectPaymentTimesheets-1.0.2-linux-aarch64.AppImage](https://github.com/ArnieSkyNet/DirectPaymentTimesheets/releases/download/v1.0.2/DirectPaymentTimesheets-1.0.2-linux-aarch64.AppImage) |
| Windows 10 or later, 64-bit Intel/AMD PC (`x86_64`) | [DirectPaymentTimesheets-1.0.2-windows-x86_64-setup.exe](https://github.com/ArnieSkyNet/DirectPaymentTimesheets/releases/download/v1.0.2/DirectPaymentTimesheets-1.0.2-windows-x86_64-setup.exe) |
| macOS, Apple Silicon (`arm64`, Apple M-series chip) | [DirectPaymentTimesheets-1.0.2-macos-arm64.dmg](https://github.com/ArnieSkyNet/DirectPaymentTimesheets/releases/download/v1.0.2/DirectPaymentTimesheets-1.0.2-macos-arm64.dmg) |
| macOS, Intel (`x86_64`) | [DirectPaymentTimesheets-1.0.2-macos-x86_64.dmg](https://github.com/ArnieSkyNet/DirectPaymentTimesheets/releases/download/v1.0.2/DirectPaymentTimesheets-1.0.2-macos-x86_64.dmg) |

### Choosing a Linux or Raspberry Pi architecture

`amd64` and `x86_64` mean the same 64-bit PC architecture, including Intel processors. `arm64` and `aarch64` mean 64-bit ARM. `armhf` here means **32-bit ARMv7 hard-float**; original ARMv6 Raspberry Pi / Pi Zero models are not supported.

On Raspberry Pi OS or another Debian-based system, open Terminal and run:

```bash
dpkg --print-architecture
```

Choose `armhf` for an `armhf` result, `arm64`/`aarch64` for `arm64`, or `amd64`/`x86_64` for `amd64`. Choose by the installed OS, not just the processor: a Pi with a 64-bit-capable CPU can run a 32-bit OS. `uname -m` reports the kernel architecture and can be misleading when a 64-bit kernel runs a 32-bit userspace. If the result is something else, do not assume these packages are compatible.

## Install, build and run

Before upgrading an existing installation, make an application backup and retain your payroll documents. Download packages only from this project's official GitHub release.

### Linux

The packages use a **Debian 12 / Bookworm baseline** (glibc 2.36), for compatible desktop Linux systems of the matching architecture. Older distributions are not supported merely by choosing AppImage. Compatible Ubuntu/Mint and Raspberry Pi OS installations need the corresponding system libraries and a working graphical desktop; this is not a headless application. See the release notes for completed platform testing.

#### Debian / Ubuntu / Raspberry Pi OS `.deb`

Download the matching `.deb`. Open Terminal in the folder containing the download and run the applicable command:

```bash
# Intel/AMD 64-bit PC:
sudo apt-get install ./direct-payment-timesheets_1.0.2_amd64.deb
# ARM32 / 32-bit Raspberry Pi OS:
sudo apt-get install ./direct-payment-timesheets_1.0.2_armhf.deb
# ARM64 / 64-bit Raspberry Pi OS:
sudo apt-get install ./direct-payment-timesheets_1.0.2_arm64.deb
```

Run **only the command matching your download**. `apt-get` installs the declared dependencies. Then open **Direct Payments Timesheets** from the desktop application menu, or run `direct-payment-timesheets` in Terminal.

#### AppImage

Download the matching AppImage and keep it somewhere convenient in your home folder. In its file properties, enable permission to execute/run it as a program, then double-click it. Alternatively, open Terminal in that folder and use the matching pair of commands:

```bash
# Intel/AMD 64-bit PC:
chmod +x DirectPaymentTimesheets-1.0.2-linux-x86_64.AppImage
./DirectPaymentTimesheets-1.0.2-linux-x86_64.AppImage
# ARM32:
chmod +x DirectPaymentTimesheets-1.0.2-linux-armhf.AppImage
./DirectPaymentTimesheets-1.0.2-linux-armhf.AppImage
# ARM64:
chmod +x DirectPaymentTimesheets-1.0.2-linux-aarch64.AppImage
./DirectPaymentTimesheets-1.0.2-linux-aarch64.AppImage
```

No administrator installation is required. AppImage still relies on compatible host graphics/desktop services. If it reports a missing FUSE facility, follow your distribution's AppImage/FUSE guidance; do not run the application with `sudo`. Packaged ARM Linux builds default to software Mesa rendering automatically; no manual launcher edit is needed.

### Windows

Requires **Windows 10 or later, 64-bit Intel/AMD**. This is not a 32-bit Windows or native Windows ARM installer.

1. Download `DirectPaymentTimesheets-1.0.2-windows-x86_64-setup.exe` from the official release above.
2. Open the downloaded file. The installer is unsigned. If SmartScreen displays **Windows protected your PC**, use **More info**, check that the filename matches your official download, then choose **Run anyway** only if you trust that download.
3. Follow the installer prompts. Installation is per-user and does not require administrator access.
4. Launch **Direct Payments Timesheets** from the Start menu.

Upgrades and uninstall preserve application data and payroll documents. The application checks for updates but does not install them automatically.

### macOS

The two separate DMGs target **macOS 12 Monterey or later**. That is the configured minimum; final hardware and minimum-OS acceptance testing remains pending. These are independently distributed builds, not Mac App Store applications.

#### Choose the correct DMG

1. Click the **Apple menu → About This Mac**.
2. If it lists **Chip: Apple M1, M2, M3, M4** or another Apple M-series chip, choose `DirectPaymentTimesheets-1.0.2-macos-arm64.dmg`.
3. If it lists an **Intel processor**, choose `DirectPaymentTimesheets-1.0.2-macos-x86_64.dmg`.

#### Copy the application into Applications

1. Download the matching DMG from the project's official v1.0.2 release page once it is published.
2. Open **Finder → Downloads**, then double-click the downloaded `.dmg`. This opens a temporary disk containing the application and an **Applications** shortcut.
3. Drag **Direct Payments Timesheets.app** onto the **Applications** shortcut. Finder may hide the `.app` extension. The spaces in the application name are intentional.
4. Wait for copying to finish. If replacing an older copy, quit the old application first and confirm replacement only for the intended application.
5. Close the disk window. In Finder's sidebar, click the eject symbol beside the mounted **Direct Payments Timesheets** disk.
6. Open **Finder → Applications** and double-click **Direct Payments Timesheets**. Launch this installed copy, not the copy inside the DMG. You may delete the downloaded DMG after successful installation.

#### If macOS blocks the first launch

The bundle has an **ad-hoc signature**, but is **not Developer ID-signed or notarised by Apple**. macOS may say the developer cannot be verified or that Apple cannot check the application for malicious software.

Only make an exception for the package you deliberately downloaded from **this project's official GitHub release**. Do not override a malware or damaged-app warning; stop and check the download instead.

1. Attempt to open the installed app from **Finder → Applications**, then dismiss the blocked-launch message without moving the app to the Bin.
2. Open **Apple menu → System Settings → Privacy & Security**.
3. Scroll to **Security** and find the message naming **Direct Payments Timesheets**. Click **Open Anyway**.
4. Authenticate with your password or Touch ID if requested, then confirm **Open** in the next dialog.
5. Later launches should work normally from Applications.

On macOS 12, the equivalent controls are **System Preferences → Security & Privacy → General**. On older macOS versions that offer it, Control-clicking the app in Finder and choosing **Open**, then confirming **Open**, may also grant the exception. On newer versions, use Privacy & Security if Finder's Open command still blocks it. If no exception is offered, consult [Apple's current safe-opening instructions](https://support.apple.com/en-gb/102445); a managed Mac may require help from its administrator.

Do not disable Gatekeeper globally or change security settings to allow all applications.

### Build and run from source

You will need Git, a current Rust toolchain including Cargo, and the native development libraries required by the desktop and TLS dependencies. See the [Rust installation instructions](https://www.rust-lang.org/tools/install) and [development guide](docs/DEVELOPMENT.md). The pinned release toolchain and platform dependencies are documented in [Release builds](docs/RELEASE-BUILDS.md).

```bash
git clone https://github.com/ArnieSkyNet/DirectPaymentTimesheets.git
cd DirectPaymentTimesheets
cargo check --locked
cargo test --locked
cargo run
```

Use disposable data for development as described under [Runtime data and configuration](#runtime-data-and-configuration).

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

Employer Maintenance uses a roughly 75/25 upper layout: Name | DOB | NI, then Email | DP Account | Telephone, with Edit Email Signature beneath; the right panel keeps the single multiline Address string. A full-width physical Signature section provides Select, Clear, open-containing-folder and the full path underneath. Redundant employer Sickness/SSP and Mileage Claims controls are removed; their legacy stored fields remain unchanged and do not gate features.

PA Maintenance includes effective-dated Contracted/Variable Hours Basis, Start/Leaving dates, read-only dated annual-leave guidance, PA Enable mileage and signature-folder access. PA sickness enablement is removed. Sickness dates can be reported for every PA, subject to payroll-state protections; Payroll calculates SSP. See [sickness and mileage rules](docs/DOMAIN.md#sickness-and-weekly-mileage).

### Work import and payroll preparation

- CSV import with duplicate detection, import audit and timestamped source archive.
- Built-in exact-minute clock-in/out and audited completed-shift corrections, kept separate from imported work.
- Provider Payroll Prep Sheet PDF import with validation and atomic per-year replacement.
- Coexistence of multiple imported payroll years, including early import of a future year.
- Schedule-driven payroll rollover rather than hard-coded calendar-year selection.
- A Dashboard operational payroll-period selector for deliberate current, historical or future work.
- Payroll Timesheet Preparation bound to the selected period, including manual Hours Worked corrections, dated leave, structured sickness dates, public holidays and weekly mileage.
- Read-only protection for submitted or indeterminate preparation records and invalidation of stale generated candidates when represented data changes.

Imported `worked_minutes` is parsed from the CSV Worked Hours field. It is not recalculated from Start Time/End Time and is not changed by the persisted rounding setting. For open payroll, selected evidence from both sources is converted to payable minutes using Payroll Settings rounding; original source minutes remain unchanged. The current default is 15 minutes Up. CSV rate and amount fields are retained as imported data but are not the authoritative payroll rate; maintained PA rate history is authoritative.

The Dashboard groups the workflow into three rows:

| Left | Middle | Right |
|---|---|---|
| Enter Hours/Shifts | Import Hours CSV | View Imported Hours |
| Payroll Timesheet Preparation | Generate Payroll Timesheets | Email Payroll Timesheets |
| Import Payroll Documents | Email Payslips | View Payroll Schedule |

### Payroll PDFs and historical evidence

- Four-week provider PDF generation using configured fonts and sizes. Payroll Settings also controls multiline footer instructions: blank lines preserved, empty to hide, 6–12 pt (default 7 pt), with explicit overflow rejection.
- Per-shift effective-dated rate allocation retained internally without printing pay rates on the provider form.
- Per-week effective-dated contracted-hours presentation.
- Dated outstanding work across payroll periods, with audited signed carry-forward reconciliation.
- Persistent manual adjustments and exact worked-item snapshots.
- Candidate PDFs associated with SHA-256 digests before production sending.
- Immutable submitted baselines and protected indeterminate-delivery state.
- Inclusive employment-date payroll eligibility independent of current status, preserving existing selected-period records outside those dates.

### Email and payroll documents

- Timesheet and payslip preview, test and production workflows.
- Period-associated production batches bound to the exact operational payroll period captured at confirmation; standalone P60/P45 need no period.
- Production timesheet and payslip routing from the employer to the payroll department, CC employer and BCC PA when available.
- Test routing through configured test addresses; payslip test email targets the configured PA test address.
- Import Payroll Documents classifies the source first. Ordinary payslips use an exact stored cycle, a plausible period chosen by the employer, or a PA archive fallback when no applicable cycle is stored. Unassociated archival payslips are not automatically emailed or used for settlement.
- UK PAYE tax-week payslip filenames; P60/P45 use their own tax year and PA directory without a payroll cycle.
- Collision-safe, idempotent payroll-information storage; P30/general documents never enter PA email. Each document uses its own explicit filename year, otherwise the configured information root.
- Mixed files/ZIPs can include prep sheets, payslips and P60/P45. Results distinguish imported, already-present, archival and failures; the overall workflow is not one filesystem/database transaction. See [import architecture](docs/ARCHITECTURE.md#schema31-payroll-documents).
- P60/P45 have schema31 document IDs and delivery tracking, including former PAs and operation without a schedule. P45 never changes employment data. Their delivery does not settle ordinary payroll or complete a schedule.
- Production and Test Payslip Email select the same eligible unsent ordinary payslip plus P60/P45 bundle; an ordinary payslip is optional. Sent documents, P30/general information and unassociated ordinary archives are excluded; uncertain delivery blocks retry.
- Standalone P60/P45 use `Payroll documents - {Personal Assistant Name}` and singular/plural neutral body text, independent of Dashboard dates. Ordinary/combined bundles retain configured cycle wording. Test sends retain TEST markers, target only the PA test address with no CC/BCC, and never mutate delivery/settlement/evidence state. See [exact email rules](docs/DOMAIN.md#email).

### Backup and restore

- Manual consistent SQLite backups with optional `config.toml` and a human-readable manifest.
- Discovery and validation of application-created backups.
- Integrity/schema checks and a mandatory pre-restore safety backup.
- SQLite-safe restore with optional configuration restore and required application restart.

## Runtime data and configuration

The default application data root is:

```text
~/.directpaymenttimesheets/
```

Set `DIRECTPAYMENTTIMESHEETS_HOME` before launching to use a different data root, which is strongly recommended for disposable development/manual-test data.

The application root contains `database.sqlite`, `config.toml` and internal import, archive, backup, log, template and cache directories. Application Settings also configures external business folders for CSV import, generated PDFs, payslips and returned payroll information. These paths can be outside the application root and may contain sensitive payroll data. Inspect them before running a workflow that writes files.

New configurations use provider-neutral paths beneath `~/Documents/DirectPaymentTimesheets/` for those business folders. The `~` prefix is expanded to the current user's home directory at runtime. Existing configured paths are preserved and remain editable in Application Settings. Saving configuration ensures business roots exist; schedule-derived folders remain lazy until an actual write/import.

The configured `email_archive` path is currently persisted but has no production consumer. Returned payroll information uses the separate payroll-information folder.

## CSV import testing

To test CSV import safely, prepare a parser-compatible CSV containing only synthetic data, then:

1. use a disposable `DIRECTPAYMENTTIMESHEETS_HOME`;
2. configure the CSV import folder in Application Settings;
3. copy the test CSV into that folder; and
4. use **Import Hours CSV** on the Dashboard.

The test CSV's PA name must correspond to a maintained PA if downstream payroll work is required. Its rate and amount columns represent the external format only and are not authoritative for generated payroll.

The files `data/sig-Employer.jpg` and `data/sig-PA.jpg` are deliberately blank white, project-created test signature images. They contain no real signatures or personal data.

## Payroll-period selections

Three selections are intentionally independent:

- the Dashboard operational period drives preparation, generation and email operations;
- Import Payroll Documents selects a period only for ordinary payslips after classification; and
- View Payroll Schedule selects an imported payroll year for display only.

User-facing labels use Payroll Week, the four-week date range and pay date. Internal cycle numbers remain database identity rather than filename Payroll Week.

## Current limitations

- This is a single-user local desktop application.
- Authentication and multi-user coordination are not implemented.
- Persisted frequency, workweek and overtime choices are not downstream configurable calculation rules; no overtime engine is implemented.
- Payroll Prep Sheet PDF import is implemented; DOCX import is recognised but not implemented.
- Backup/restore has no scheduling, retention cleanup, compression or cloud integration.
- Restore accepts only recognised DirectPaymentTimesheets backup directories, not arbitrary SQLite files.
- Historical annual-leave numeric rule history and sickness reference-period/accrual calculations are not implemented. Guidance's current sickness warning reads legacy weekly sickness hours, not structured sickness dates.
- Durable CSV content identity/row-to-import provenance, automatic promotion/emailing of archival ordinary payslips, and user-facing indeterminate-email recovery remain future work.
- Authentication, roles and web/mobile access are not implemented. `email_archive` has no actual email-archiving consumer.

The existing user-triggered GitHub **Check for updates** reports newer versions and installation guidance; it does not download or install updates. Users must manually download and install the appropriate GitHub release asset.

## Annual leave and employment

Payroll Settings includes two annual-leave settings groups: contracted-hours annual leave (recurring DD/MM boundary and statutory weeks) and variable-hours annual leave (recurring DD/MM boundary and accrual percentage). Save Payroll Settings saves all four together. Personal Assistant Maintenance provides read-only Annual Leave guidance with a compact April-to-March leave-year selector, entitlement/accrued, taken, remaining, Calculated to (Variable/mixed years), and an auditable Details view. Payroll remains definitive; negative remaining hours are retained. Guidance uses submitted work or safe retained historical weekly totals, not fresh raw-shift reconciliation. See [guidance rules and limitations](docs/DOMAIN.md#annual-leave-guidance).

Personal Assistants have an optional Leaving date, stored as canonical `DD/MM/YYYY`. Schema 27 adds nullable `personal_assistants.leaving_date` without backfilling dates or altering historical data. A supplied date must be real and not precede Start date. Stored Active/Inactive status is never changed automatically. Ordinary selected-period payroll inclusion uses inclusive Start/Leaving date overlap with the four-week period, independent of current Active/Inactive status. Missing employment boundaries remain unbounded. Existing selected-period preparation records remain included regardless of status or employment dates. Annual-leave guidance applies Start and Leaving dates inclusively without changing employment status.


### Payroll evidence and reconciliation

Completed Hours Shift records now feed payroll alongside Hours Keeper imports, while both original sources remain separate. Possible overlapping shifts use one consolidated, audited duplicate resolver. Preparation preserves sent submissions, offers contextual correction/resubmission or carry-forward, and treats each PA's definitively Sent payslip as settlement. Historical payment uncertainty and aggregate negative estimate corrections require explicit contextual reviews. Signed corrections persist, never make payable worked hours negative, and appear on PDFs only through final bold totals plus a subordinate `(Info only +/-X.XX hours)` line.

See [Payroll evidence and reconciliation](docs/PAYROLL-EVIDENCE.md) for storage, safeguards, limitations of historical evidence and the manual verification sequence.

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
