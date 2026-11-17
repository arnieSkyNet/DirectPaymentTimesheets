# Direct Payment Timesheets

Direct Payment Timesheets is a local desktop application for managing UK Direct Payment Personal Assistant timesheets and payroll paperwork.

## Downloads and installation instructions

**Release: v1.0.7 · Linux, Windows and macOS · SQLite schema 42**

The links below download directly from the **private repository's v1.0.7 release**. Sign in to GitHub with an account that has access to `arnieSkyNet/DirectPaymentTimesheets`.

**Already installed? Start with the [First-Time Setup Guide for v1.0.7](docs/FIRST-TIME-SETUP.md)** — employer and PA details, email, the provider's payroll schedule, hours and your first PDF timesheet.

Choose your computer and installed operating system below. On Linux, choose either the `.deb` installer or the portable AppImage.

**Windows 10 and later — Intel/AMD 64-bit (x86-64)**

[![Windows installer](https://img.shields.io/badge/Windows%20installer-v1.0.7-blue?logo=windows11&logoColor=white)](https://github.com/arnieSkyNet/DirectPaymentTimesheets/releases/download/v1.0.7/DirectPaymentTimesheets-1.0.7-windows-x86_64-setup.exe)<br>

**Windows 10 and 11 — ARM64 (native application)**

[![Windows ARM64 installer](https://img.shields.io/badge/Windows%20ARM64%20installer-v1.0.7-blue?logo=windows11&logoColor=white)](https://github.com/arnieSkyNet/DirectPaymentTimesheets/releases/download/v1.0.7/DirectPaymentTimesheets-1.0.7-windows-arm64-setup.exe)<br>

**Linux — Intel/AMD 64-bit (x86-64 / amd64)**

[![Linux .deb](https://img.shields.io/badge/Linux%20.deb-v1.0.7-blue?logo=debian&logoColor=white)](https://github.com/arnieSkyNet/DirectPaymentTimesheets/releases/download/v1.0.7/direct-payment-timesheets_1.0.7_amd64.deb)<br>
[![Linux AppImage](https://img.shields.io/badge/Linux%20AppImage-v1.0.7-green?logo=linux&logoColor=white)](https://github.com/arnieSkyNet/DirectPaymentTimesheets/releases/download/v1.0.7/DirectPaymentTimesheets-1.0.7-linux-x86_64.AppImage)<br>

**Raspberry Pi / Linux — ARM32 (armhf, ARMv7)**

[![ARM32 .deb](https://img.shields.io/badge/ARM32%20.deb-v1.0.7-blue?logo=debian&logoColor=white)](https://github.com/arnieSkyNet/DirectPaymentTimesheets/releases/download/v1.0.7/direct-payment-timesheets_1.0.7_armhf.deb)<br>
[![ARM32 AppImage](https://img.shields.io/badge/ARM32%20AppImage-v1.0.7-green?logo=linux&logoColor=white)](https://github.com/arnieSkyNet/DirectPaymentTimesheets/releases/download/v1.0.7/DirectPaymentTimesheets-1.0.7-linux-armhf.AppImage)<br>

**Raspberry Pi / Linux — ARM64 (arm64 / aarch64)**

[![ARM64 .deb](https://img.shields.io/badge/ARM64%20.deb-v1.0.7-blue?logo=debian&logoColor=white)](https://github.com/arnieSkyNet/DirectPaymentTimesheets/releases/download/v1.0.7/direct-payment-timesheets_1.0.7_arm64.deb)<br>
[![ARM64 AppImage](https://img.shields.io/badge/ARM64%20AppImage-v1.0.7-green?logo=linux&logoColor=white)](https://github.com/arnieSkyNet/DirectPaymentTimesheets/releases/download/v1.0.7/DirectPaymentTimesheets-1.0.7-linux-aarch64.AppImage)<br>

**macOS — Apple Silicon (ARM64)**

[![Apple Silicon DMG](https://img.shields.io/badge/Apple%20Silicon%20DMG-v1.0.7-blue?logo=apple&logoColor=white)](https://github.com/arnieSkyNet/DirectPaymentTimesheets/releases/download/v1.0.7/DirectPaymentTimesheets-1.0.7-macos-arm64.dmg)<br>

**macOS — Intel (x86-64)**

[![Intel DMG](https://img.shields.io/badge/Intel%20DMG-v1.0.7-blue?logo=apple&logoColor=white)](https://github.com/arnieSkyNet/DirectPaymentTimesheets/releases/download/v1.0.7/DirectPaymentTimesheets-1.0.7-macos-x86_64.dmg)<br>

[Installation instructions](#install-build-and-run) · [v1.0.7 release notes](https://github.com/arnieSkyNet/DirectPaymentTimesheets/releases/tag/v1.0.7) · [SHA256SUMS](https://github.com/arnieSkyNet/DirectPaymentTimesheets/releases/download/v1.0.7/SHA256SUMS) · [All releases](https://github.com/arnieSkyNet/DirectPaymentTimesheets/releases/)

### Choosing a Linux or Raspberry Pi architecture

`amd64` and `x86_64` mean the same 64-bit PC architecture, including Intel processors. `arm64` and `aarch64` mean 64-bit ARM. `armhf` here means **32-bit ARMv7 hard-float**; original ARMv6 Raspberry Pi / Pi Zero models are not supported.

On Raspberry Pi OS or another Debian-based system, open Terminal and run:

```bash
dpkg --print-architecture
```

Choose `armhf` for an `armhf` result, `arm64`/`aarch64` for `arm64`, or `amd64`/`x86_64` for `amd64`. Choose by the installed OS, not just the processor: a Pi with a 64-bit-capable CPU can run a 32-bit OS. `uname -m` reports the kernel architecture and can be misleading when a 64-bit kernel runs a 32-bit userspace. If the result is something else, do not assume these packages are compatible.

## Install, build and run

Before upgrading an existing installation, close every running copy, keep a database/configuration recovery copy and separately protect external payroll documents, signatures and other business files. Download packages only from this project's official GitHub release.

When upgrading from v1.0.6, first launch automatically creates and verifies a pre-upgrade database backup, including committed WAL data, before migrating schema **41 to 42**. Configuration is included when present; OS credential-store secrets and external business files are not copied. Backup failure blocks the upgrade. Migration is one-way: **never run v1.0.6 against the upgraded database**. Check version 1.0.7/schema 42, settings and email access after startup. See the [First-Time Setup Guide](docs/FIRST-TIME-SETUP.md#2-first-launch-or-upgrade-from-v106) for the upgrade and recovery procedure.

### Linux

The packages use a **Debian 12 / Bookworm baseline** (glibc 2.36), for compatible desktop Linux systems of the matching architecture. Older distributions are not supported merely by choosing AppImage. Compatible Ubuntu/Mint and Raspberry Pi OS installations need the corresponding system libraries and a working graphical desktop; this is not a headless application. See the [release notes](https://github.com/arnieSkyNet/DirectPaymentTimesheets/releases/tag/v1.0.7) for platform compatibility and known limitations.

#### Debian / Ubuntu / Raspberry Pi OS `.deb`

Download the matching `.deb`. Open Terminal in the folder containing the download and run the applicable command:

```bash
# Intel/AMD 64-bit PC:
sudo apt-get install ./direct-payment-timesheets_1.0.7_amd64.deb
# ARM32 / 32-bit Raspberry Pi OS:
sudo apt-get install ./direct-payment-timesheets_1.0.7_armhf.deb
# ARM64 / 64-bit Raspberry Pi OS:
sudo apt-get install ./direct-payment-timesheets_1.0.7_arm64.deb
```

Run **only the command matching your download**. `apt-get` installs the declared dependencies. Then open **Direct Payments Timesheets** from the desktop application menu, or run `direct-payment-timesheets` in Terminal.

#### AppImage

Download the matching AppImage and keep it somewhere convenient in your home folder. In its file properties, enable permission to execute/run it as a program, then double-click it. Alternatively, open Terminal in that folder and use the matching pair of commands:

```bash
# Intel/AMD 64-bit PC:
chmod +x DirectPaymentTimesheets-1.0.7-linux-x86_64.AppImage
./DirectPaymentTimesheets-1.0.7-linux-x86_64.AppImage
# ARM32:
chmod +x DirectPaymentTimesheets-1.0.7-linux-armhf.AppImage
./DirectPaymentTimesheets-1.0.7-linux-armhf.AppImage
# ARM64:
chmod +x DirectPaymentTimesheets-1.0.7-linux-aarch64.AppImage
./DirectPaymentTimesheets-1.0.7-linux-aarch64.AppImage
```

No administrator installation is required. AppImage still relies on compatible host graphics/desktop services. If it reports a missing FUSE facility, follow your distribution's AppImage/FUSE guidance; do not run the application with `sudo`. Packaged ARM Linux builds default to software Mesa rendering automatically; no manual launcher edit is needed.

### Windows

Choose by **Settings > System > About > System type**: Intel/AMD 64-bit uses the
`windows-x86_64` installer; an ARM-based Windows 10/11 PC uses the
`windows-arm64` installer. Neither is for 32-bit Windows. See the [release notes](https://github.com/arnieSkyNet/DirectPaymentTimesheets/releases/tag/v1.0.7) for compatibility and known limitations.

1. Download `DirectPaymentTimesheets-1.0.7-windows-x86_64-setup.exe` for Intel/AMD or `DirectPaymentTimesheets-1.0.7-windows-arm64-setup.exe` for ARM64 from the official release above.
2. Open the downloaded file. The installer is unsigned. If SmartScreen displays **Windows protected your PC**, use **More info**, check that the filename matches your official download, then choose **Run anyway** only if you trust that download.
3. Follow the installer prompts. Installation is per-user and does not require administrator access.
4. Launch **Direct Payments Timesheets** from the Start menu.

Both Windows architectures use the same per-user application folder and uninstall entry; an ARM64 upgrade replaces the application, rather than creating a second installation. Close the application before upgrading. Upgrades and uninstall preserve application data and payroll documents. The application checks for updates but does not install them automatically.

### macOS

The v1.0.7 macOS packages require **macOS 11.4 Big Sur or later for Intel (x86_64)** and **macOS 12.0 Monterey or later for Apple Silicon (arm64)**. See the [release notes](https://github.com/arnieSkyNet/DirectPaymentTimesheets/releases/tag/v1.0.7) for compatibility and known limitations. These are independently distributed builds, not Mac App Store applications.

#### Choose the correct DMG

1. Click the **Apple menu → About This Mac**.
2. If it lists **Chip: Apple M1, M2, M3, M4** or another Apple M-series chip, choose `DirectPaymentTimesheets-1.0.7-macos-arm64.dmg`.
3. If it lists an **Intel processor**, choose `DirectPaymentTimesheets-1.0.7-macos-x86_64.dmg`.

#### Copy the application into Applications

1. Download the matching DMG from the project's official v1.0.7 release page.
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

On macOS 11 and 12, the equivalent controls are **System Preferences → Security & Privacy → General**. On older macOS versions that offer it, Control-clicking the app in Finder and choosing **Open**, then confirming **Open**, may also grant the exception. On newer versions, use Privacy & Security if Finder's Open command still blocks it. If no exception is offered, consult [Apple's current safe-opening instructions](https://support.apple.com/en-gb/102445); a managed Mac may require help from its administrator.

Do not disable Gatekeeper globally or change security settings to allow all applications.

## Features

### Records and settings

- Employer, payroll-provider and Personal Assistant maintenance.
- Effective-dated PA pay-rate history, including employer top-up.
- Effective-dated contracted weekly hours.
- Application themes, folder/font settings, payroll settings and email settings.
- Employer and PA signature image import and authorised signature drawing.
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
- Select England and Wales (default), Scotland or Northern Ireland in Payroll Settings. GOV.UK holiday dates refresh in the background and can be refreshed manually; validated cached dates work offline. Coverage/fallback warnings remain visible. Existing drafts and historical PDFs are preserved, and identifying holidays never automatically subtracts worked hours.
- Submitted timesheets can be amended, saved and regenerated without an unlocking approval. Earlier versions/submissions remain auditable; the latest regenerated saved version governs later reconciliation. Actual unresolved delivery remains protected, and edited PDFs must be regenerated before sending. See [timesheet amendments](docs/TIMESHEET-AMENDMENTS.md).

By default, imported `worked_minutes` is parsed from the CSV Worked Hours field. Payroll Settings → CSV Import also supports Worked Hours minus Break, End minus Start, and End minus Start minus Break, with configurable heading aliases and enabled columns. Import preserves the selected interpretation and unrounded source minutes; later settings changes do not reinterpret historical imports. See [CSV import](docs/CSV-IMPORT.md). For open payroll, selected evidence from both sources is converted to payable minutes using Payroll Settings rounding; original source minutes remain unchanged. The current default is 15 minutes Up. CSV rate and amount fields are retained as imported data but are not the authoritative payroll rate; maintained PA rate history is authoritative.

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
- Production timesheet routing from the employer to the payroll department, optional default-enabled CC employer and BCC PA when available. Production payslips go individually to each selected PA, BCC the employer, with no CC or payroll-department recipient.
- Test routing through configured test addresses; payslip test email targets the configured PA test address.
- Import Payroll Documents classifies the source first. Ordinary payslips use an exact stored cycle, a plausible period chosen by the employer, or a PA archive fallback when no applicable cycle is stored. Unassociated archival payslips are not automatically emailed or used for settlement.
- UK PAYE tax-week payslip filenames; P60/P45 use their own tax year and PA directory without a payroll cycle.
- Collision-safe, idempotent payroll-information storage; P30/general documents never enter PA email. Each document uses its own explicit filename year, otherwise the configured information root.
- Mixed files/ZIPs can include prep sheets, ordinary payslips, P45/P60 supplements and shared information such as P30. Each entry is validated and staged independently; successful items remain successful when another entry fails, and results report imported, already-present, archived and failed items. ZIP path, entry-count, size and compression-ratio checks run before publication. The mixed workflow is not one filesystem/database transaction. See [import architecture](docs/ARCHITECTURE.md#payroll-document-import-and-delivery).
- P45/P60 have cycle-independent schema31 document IDs, hashes and delivery-history reconciliation, including former PAs and operation without a schedule. P45 never changes employment data. Their delivery does not settle ordinary payroll or complete a schedule. P30 and other shared information are stored separately and never enter PA email.
- Production and Test Payslip Email select the same eligible unsent ordinary payslip plus P60/P45 bundle; an ordinary payslip is optional. Sent documents, P30/general information and unassociated ordinary archives are excluded; uncertain delivery blocks retry.
- Standalone P60/P45 use `Payroll documents - {Personal Assistant Name}` and singular/plural neutral body text, independent of Dashboard dates. Ordinary/combined bundles use `Payslip for Week <PAYE week>` and the configured payslip body. MIME filenames use clean canonical payroll names; internal revision/hash storage filenames are not disclosed. Test sends retain TEST markers, target only the PA test address with no CC/BCC, and never mutate delivery/settlement/evidence state. See [exact email rules](docs/DOMAIN.md#email).

- Corrected payroll document replacement with retained originals and delivery history.
- Inactive-PA document archiving and **Resume incomplete filing** for interrupted moves.
- Email attempt history and reviewed resolution of uncertain deliveries; uncertain messages are never automatically retried. See [delivery review](docs/DOMAIN.md#reviewing-uncertain-delivery).

### Backup and restore

- Verified consistent SQLite backups protect PAs, hours, payroll records/history and embedded PDFs, including committed WAL data. When present, `config.toml` is also included with saved application preferences and credential references; passwords and Microsoft tokens remain in the OS credential store. A recovery manifest lists known external business paths.
- Before any schema upgrade, startup preserves a verified original recovery copy, runs migrations in isolation and installs transactionally. Backup failure aborts the upgrade.
- Supported legacy backups restore through isolated migration. Review identifies current rows absent/different in restored data; a documented reason and explicit rollback acknowledgement are required. Changed data invalidates approval; unresolved email uncertainty blocks restore.
- A separate verified pre-restore copy retains original submissions, delivery evidence and saved configuration when present. Restore replaces configuration only if the selected backup contains it. Password recovery requires the corresponding OS credential-store entry, or password re-entry; Microsoft may need reconnection. Restore requires closing other instances and restarting afterward.
- External PDF, payslip, signature, CSV/archive and other business files are **not** copied/restored by database backup. Immutable submitted PDF and dispatch attachment bytes already retained inside SQLite **are included**, along with recipients and delivery evidence. These sensitive records increase database/backup size; no automatic retention cleanup is provided. Separately protect the complete application root and configured external folders/files before upgrade or rollback.

## Runtime data and configuration

The default application data root is `~/.directpaymenttimesheets/` on Unix (Linux/macOS), or `%LOCALAPPDATA%/DirectPaymentTimesheets` on Windows.

Set `DIRECTPAYMENTTIMESHEETS_HOME` before launching to override either default, which is strongly recommended for disposable development/manual-test data. The override is used verbatim: use an absolute path; shell-quoted `~` is not expanded by this override. Redirect configured external business paths separately.

The application root contains `database.sqlite`, `config.toml` and internal import, archive, backup, log, template and cache directories. Application Settings also configures external business folders for CSV import, generated PDFs, payslips and returned payroll information. These paths can be outside the application root and may contain sensitive payroll data. Inspect them before running a workflow that writes files.

New configurations use provider-neutral paths beneath `~/Documents/DirectPaymentTimesheets/` on Unix, or the discovered Windows Documents directory’s `DirectPaymentTimesheets` subfolder, for those business folders. The `~` prefix is expanded to the current user's home directory at runtime. Existing configured paths are preserved and remain editable in Application Settings. Startup checks business folders in the background and warns without preventing Settings access. Saving configuration preserves the selected paths without creating those folders. Workflows validate their own required locations and create outputs only when actually used; no alternate folder is silently substituted.

The configured `email_archive` path is currently persisted but has no production consumer. Returned payroll information uses the separate payroll-information folder.

## Payroll-period selections

Three selections are intentionally independent:

- the Dashboard operational period drives preparation, generation and email operations;
- Import Payroll Documents selects a period only for ordinary payslips after classification; and
- View Payroll Schedule selects an imported payroll year for display only.

User-facing labels use Payroll Week, the four-week date range and pay date. Internal cycle numbers remain database identity rather than filename Payroll Week.

## Current limitations

- This is a single-user local desktop application.
- User accounts, authentication, roles and shared multi-user workflows are not implemented. Cross-process OS locks coordinate cooperating local instances during dispatch, reconciliation, generation, filing and recovery; they do not provide user authentication.
- Frequency, workweek and overtime preferences are displayed read-only as unsupported, retaining stored values. They do not configure downstream calculations; no overtime engine is implemented.
- Payroll Prep Sheet PDF import is implemented; DOCX import is recognised but not implemented.
- Backup/restore has no scheduling, retention cleanup, compression or cloud integration.
- Restore accepts only recognised DirectPaymentTimesheets backup directories, not arbitrary SQLite files.
- Historical annual-leave numeric rule history and sickness reference-period/accrual calculations are not implemented. Guidance flags relevant legacy sickness hours and structured sickness dates for additional variable-hours review without adding sickness accrual figures.
- Automatic promotion/emailing of archival ordinary payslips remains future work. Durable CSV SHA-256 content/path identity and row provenance are implemented (schema38). Timesheet and payslip/supplement uncertainty have explicit audited review workflows.
- Authentication, roles and web/mobile access are not implemented. `email_archive` has no actual email-archiving consumer.

The existing user-triggered GitHub **Check for updates** reports newer versions and installation guidance; it does not download or install updates. Users must manually download and install the appropriate GitHub release asset.

## Annual leave and employment

Payroll Settings includes two annual-leave settings groups: contracted-hours annual leave (recurring DD/MM boundary and statutory weeks) and variable-hours annual leave (recurring DD/MM boundary and accrual percentage). Save Payroll Settings saves all four together. Personal Assistant Maintenance provides read-only Annual Leave guidance with a compact April-to-March leave-year selector, entitlement/accrued, taken, remaining, Calculated to (Variable/mixed years), and an auditable Details view. Payroll remains definitive; negative remaining hours are retained. Guidance uses submitted work or safe retained historical weekly totals, not fresh raw-shift reconciliation. See [guidance rules and limitations](docs/DOMAIN.md#annual-leave-guidance).

Personal Assistants have an optional Leaving date, stored as canonical `DD/MM/YYYY`. Schema 27 adds nullable `personal_assistants.leaving_date` without backfilling dates or altering historical data. A supplied date must be real and not precede Start date. Stored Active/Inactive status is never changed automatically. Ordinary selected-period payroll inclusion uses inclusive Start/Leaving date overlap with the four-week period, independent of current Active/Inactive status. Missing employment boundaries remain unbounded. Existing selected-period preparation records remain included regardless of status or employment dates. Annual-leave guidance applies Start and Leaving dates inclusively without changing employment status.


### Payroll evidence and reconciliation

Completed Hours Shift records now feed payroll alongside Hours Keeper imports, while both original sources remain separate. Possible overlapping shifts use one consolidated, audited duplicate resolver. Preparation preserves sent submissions, offers contextual correction/resubmission or carry-forward, and treats each PA's definitively Sent payslip as settlement. Historical payment uncertainty and aggregate negative estimate corrections require explicit contextual reviews. Signed corrections persist, never make payable worked hours negative, and appear on PDFs only through final bold totals plus a subordinate `(Info only +/-X.XX hours)` line.

See [Payroll evidence and reconciliation](docs/PAYROLL-EVIDENCE.md) for storage, safeguards, limitations of historical evidence and the manual verification sequence.

## Documentation

- [First-Time Setup Guide for v1.0.7](docs/FIRST-TIME-SETUP.md)
- [v1.0.7 release notes, verification and known limitations](https://github.com/arnieSkyNet/DirectPaymentTimesheets/releases/tag/v1.0.7)
- [Architecture](docs/ARCHITECTURE.md)
- [Domain rules](docs/DOMAIN.md)
- [Development guide](docs/DEVELOPMENT.md)
- [Conceptual data model](docs/DATA-MODEL.md)
- [Database schema](docs/DATABASE-SCHEMA.md)

## Licence

DirectPaymentTimesheets is free software licensed under the [GNU General Public License version 3 or later](LICENSE), identified by the SPDX expression `GPL-3.0-or-later`. You may redistribute and modify the source under that licence. Charging for copies or genuine services is permitted, but recipients retain the GPL freedoms and distributed modified versions remain subject to the GPL's requirements.

Third-party dependencies and bundled font components remain subject to their respective terms; see [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).
