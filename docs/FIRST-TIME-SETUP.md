# First-Time Setup Guide — v1.0.7

DirectPaymentTimesheets helps a Direct Payment employer record PA hours, prepare four-week payroll timesheets, send them to Payroll and distribute returned payslips. It does not calculate a complete payroll or replace your payroll provider's advice.

Use this guide with **v1.0.7, database schema 42**. Examples use UK dates, such as **09/11/2026**, and 24-hour times. Keep real payroll information private.

## 1. Install the right package

Download only from the project's [official releases](https://github.com/arnieSkyNet/DirectPaymentTimesheets/releases). Choose an asset labelled **1.0.7** when it is published; a successful build or an older download is not a published v1.0.7 release. Check the release's checksums if supplied. Close every running copy before installing an upgrade.

| Computer | Package to choose |
|---|---|
| Windows Intel/AMD 64-bit | `DirectPaymentTimesheets-1.0.7-windows-x86_64-setup.exe` |
| Windows ARM64 | `DirectPaymentTimesheets-1.0.7-windows-arm64-setup.exe` |
| Linux Intel/AMD 64-bit | `direct-payment-timesheets_1.0.7_amd64.deb` or `DirectPaymentTimesheets-1.0.7-linux-x86_64.AppImage` |
| 32-bit Raspberry Pi OS / ARMv7 | `direct-payment-timesheets_1.0.7_armhf.deb` or `DirectPaymentTimesheets-1.0.7-linux-armhf.AppImage` |
| 64-bit Raspberry Pi OS / Linux ARM64 | `direct-payment-timesheets_1.0.7_arm64.deb` or `DirectPaymentTimesheets-1.0.7-linux-aarch64.AppImage` |
| Intel Mac | `DirectPaymentTimesheets-1.0.7-macos-x86_64.dmg` |
| Apple Silicon Mac | `DirectPaymentTimesheets-1.0.7-macos-arm64.dmg` |

Choose for the operating system's architecture, particularly on Raspberry Pi. The packages target Windows 10 or later; compatible desktop Linux based on Debian 12's libraries or newer; Intel macOS 11.4 or later; and Apple Silicon macOS 12.0 or later. Package builds do not establish every hardware/minimum-OS combination: consult the release notes and [platform validation limits](RELEASE-BUILDS.md).

**Windows:** run the matching installer and launch **Direct Payments Timesheets** from the Start menu. Installation is per-user. The installer is unsigned: check its official origin before accepting any Windows warning. Do not run both architecture installers to create separate installations; they share the per-user installation.

**Linux:** install the matching `.deb` with your package manager, for example `sudo apt install ./direct-payment-timesheets_1.0.7_amd64.deb` on an amd64 PC. Launch **Direct Payments Timesheets** from the desktop menu. For AppImage, make the downloaded file executable in its properties, then open it. AppImage still needs compatible desktop/graphics libraries and may need your distribution's FUSE support. Do not run the application with `sudo`.

**macOS:** open the matching DMG, drag **Direct Payments Timesheets.app** to **Applications**, eject the disk and launch the installed copy. It is ad-hoc signed, not Apple-notarised. If blocked, use the macOS security approval process only for your verified official download; do not disable Gatekeeper globally or bypass a damaged-app/malware warning.

## 2. First launch, or upgrade from v1.0.6

Check the header or **About** for application version **1.0.7** and database schema **42**. There is no requirement to create a PA or payroll record before testing email.

For an existing v1.0.6 installation:

1. Close all application instances. Keep a recovery copy of your existing data and any external payroll files you need.
2. Install and launch v1.0.7 using the same OS account and existing application data location.
3. Before migrating schema 41 to 42, the application automatically creates and verifies a pre-upgrade database backup, including committed SQLite WAL data. It includes `config.toml` when present, with secure credential references instead of newly copied plaintext SMTP passwords.
4. Let startup finish. If backup creation fails, the upgrade is blocked. If migration fails, retain the original database and verified recovery backup; read the error before retrying. Do not delete files or create an empty profile to hide the problem.
5. Check employer/PA details, payroll periods, settings and email access before your next submission.

Migration is **one-way**. Never launch v1.0.6 against the upgraded database. Recovery uses the pre-upgrade backup, through the reviewed restore process in section 13.

The normal data location is `%LOCALAPPDATA%\DirectPaymentTimesheets` on Windows or `~/.directpaymenttimesheets` on Linux/macOS. Advanced installations may override it. Do not change the location during an upgrade unless you have deliberately prepared a separate recovery/test profile.

## 3. Find your way around and save changes

Use the bottom navigation for **Dashboard**, **Employer Maintenance**, **Personal Assistant Maintenance** and **Payroll Settings**. The header provides **Settings**, **Email Settings**, **About** and **Exit**.

Save each form before moving on. A highlighted Save control indicates unsaved changes; a success message confirms saving. If saving fails, read the message and retry after correcting the problem. Some screens have separate Save controls for separate records.

Open **Settings** for **Application Settings**, with these tabs:

- **Appearance:** theme, date display, time controls and Dashboard button sizes. Choose `DD/MM/YYYY — 09/09/2026` if you prefer numeric UK dates.
- **Folders:** CSV import, PDF output, email archive, payslip and payroll-information locations. Choose accessible folders you can recognise and back up.
- **PDF Settings:** font files and sizes. Keep chosen font files available.
- **Storage & Security:** storage privacy information and checks.
- **Backup & Restore:** database/configuration recovery copies.

Use **Save Application Settings**. Saving folder paths does not create all the folders or prove they are usable; the relevant workflow checks its own locations. The email archive path is saved but currently has no production filing operation.

## 4. Set up the employer and payroll rules

In **Employer Maintenance**, enter the employer's details and click **Save Employer**. Add a signature with **Select Signature...** if required. Signature drawing is offered where a usable saved signature is missing; save the employer before relying on it in documents.

Open **Payroll Settings**. Its five tabs are:

| Tab | What to set |
|---|---|
| **Payroll Provider** | Provider contact details and **Rounding Minutes** under Payroll Rules. The actual Payroll Department destination address is saved separately in Email Settings. |
| **Pay Rates** | Optional **National / Standard Pay Rate Update** for creating effective-dated PA rates. Review the proposed changes; do not use a national rate as a substitute for checking each PA's agreed rate. |
| **Holidays & Leave** | Public-holiday region and contracted/variable annual-leave settings. **Refresh holiday data** updates the calendar when a connection is available. |
| **CSV Import** | Introductory rows to skip, hours calculation method, enabled columns and heading aliases. |
| **Timesheet Footer** | Instructions/footer text and its font size on generated payroll timesheets. |

Click **Save Payroll Settings** after editing. The screen reports partial saves if provider/leave details saved but configuration could not be saved; retry to finish rather than assuming every setting changed.

Payroll periods and week-commencing dates come from the imported four-week schedule. **Payroll Frequency** and **Start of Workweek** are retained preferences, labelled as not applied. Overtime calculation is not enabled by saving settings. Public-holiday identification does not automatically subtract worked hours or establish entitlement: check the PA's contract and your provider's instructions.

## 5. Add and maintain Personal Assistants

In **Personal Assistant Maintenance**, choose **New Personal Assistant**, enter the PA's details and click **Save Personal Assistant**. Check names carefully: imported hours must resolve to the correct person. Add the PA's email address if payslips will be emailed.

For each PA, maintain effective-dated **Contracted Weekly Hours** and **Pay Rates**, using **Save Hours Basis** and **Save Pay Rate** in their sections. Choose the correct contracted/variable hours basis and agreed rate dates. A later rate should be added with its proper effective date rather than changing historical evidence to force a result.

Add a signature if needed. Keep linked signature files available. Record status and leaving date correctly when someone departs; departure does not erase their payroll history. Departed PAs are excluded from current email recipient lists, while retained historical/final records have separate selection paths. Read filing messages after changing PA details or departure information; use **Resume incomplete filing** when offered rather than moving registered files by hand.

## 6. Import your payroll schedule

Obtain the provider's Payroll Prep Sheet before preparing the first cycle. On **Dashboard**, choose **Import Payroll Documents** and select the supported Payroll Prep Sheet PDF; ZIP imports can contain recognised payroll documents too. Although the file picker also offers Prep Sheet DOCX, Word Prep Sheet importing is not implemented in v1.0.7: obtain the PDF version. Follow any payroll-period selection/review and inspect the result for unsupported or unmatched documents.

Use **View Payroll Schedule** to check the payroll year, four-week periods, week-commencing dates and pay dates. Select the intended operational payroll period on the Dashboard. Do not invent periods from the stored frequency preference or reuse an example schedule.

## 7. Record or import hours

### Enter shifts directly

Choose **Enter Hours/Shifts** and select an active PA. Use **Clock in now** or **Clock in at…**, enter any break minutes and save notes with **Save Notes**, then finish with **Clock out now** or **Clock out at…**. The “at…” controls let you choose exact dates/times. Review the recorded shift before preparing payroll. These are source hours; payroll rounding and pay-rate allocation happen later.

### Import CSV files

First open **Payroll Settings → CSV Import**:

1. Set **CSV rows to skip** to the number of introductory rows before the heading row. The default is one; change it if your export starts with headings immediately.
2. Map headings to PA, Start, End and the other enabled fields. Separate alternative headings with `|`.
3. Choose one of the four hours methods:

| Method | Hours used before payroll rounding |
|---|---|
| **CSV Worked Hours** | The exported Worked Hours value; no further break deduction |
| **CSV Worked Hours minus Break** | Exported Worked Hours less Break |
| **End minus Start (no break deduction)** | Elapsed time between Start and End |
| **End minus Start minus Break** | Elapsed time less Break |

Required fields cannot be disabled. A break-deducting method requires an explicit Break on every row, including zero for no break. Otherwise an optional break does not reduce hours. Do not subtract a break twice. Disabled/unknown columns are ignored; omitted Rate/Amount values display as Missing and do not supply the PA's authoritative payroll rate.

Save settings, put exports in the configured **CSV Import Folder**, then choose **Import Hours CSV** on the Dashboard. This scans the folder first; it is not a file-picker action. Review **Import Summary**, refused rows/files and any duplicate review. Use **View Imported Hours** to inspect the evidence. Changing parser settings affects new evidence, not the interpretation of previously retained imports.

### Clipboard fallback

After folder import, the application may list **Outstanding active Personal Assistants** for the selected period. Choose **Yes**, select the PA, then **Import from Clipboard**. Paste the CSV text, including the headings and introductory rows required by your saved settings, into the editor and click **Import**.

Every pasted row must identify the selected active PA unambiguously. Clipboard text is saved as a uniquely named CSV in the import folder and goes through the same importer. If import fails after saving, the CSV remains for review/retry. **Cancel** closes the editor; **Cancel remaining — import saved CSVs** finishes with CSVs already captured. This fallback is not a general arbitrary-text or spreadsheet-cell importer.

### Check configured rounding

In **Payroll Settings → Payroll Provider**, select the agreed increment and Up/Down direction. Available increments are 1, 5, 10, 15, 30 and 60 minutes. Rounding is applied **once per shift during payroll preparation**, not once to the four-week total and not while importing CSV evidence. For example, 7 hours 32 minutes becomes 7 hours 45 minutes with 15 Minutes Up, or 7 hours 30 minutes with 15 Minutes Down.

## 8. Prepare, generate and send timesheets

1. Select the correct Dashboard payroll period and open **Payroll Timesheet Preparation**.
2. Review each PA's worked hours, rates, leave, sickness, public holidays, mileage and notes. Check carried-forward/returned information where shown; returned hours do not silently replace outgoing payroll.
3. Use the PA's **Save [PA name]** control. Resolve validation messages before continuing.
4. Choose **Generate Payroll Timesheets** on the Dashboard. Review the PA selection, choose **Generate selected PAs** and follow any signature warnings to complete generation. Inspect the generated PDFs in the configured output location before sending.
5. Choose **Email Payroll Timesheets**. Decide whether to add an additional note, review the PA selection, recipients and period, then confirm **Send**. Check the Payroll Department address and whether employer CC is enabled.
6. Read the per-PA result and **Timesheet Email Status**. A generated PDF is not a submission; a successful application SMTP send records the submitted baseline. SMTP acceptance alone does not prove receipt by Payroll.

Do not repeatedly click Send after a timeout or uncertain result. The application retains uncertain delivery evidence and blocks unsafe retries. Reconcile the outcome with Payroll and use the offered documented review before further sending. Resends and larger batches require additional acknowledgement.

### Amend a submitted or settled timesheet

Open its period in **Payroll Timesheet Preparation**, make and save the supported amendments, generate a new PDF, inspect it and separately send it. Saving does not automatically regenerate, resend, change an old submitted PDF or erase prior submission/settlement history. An unchanged resend is different from sending an amended document. Keep Payroll informed so a replacement is not processed as an additional payment. An indeterminate delivery outcome must be resolved before editing/sending can safely continue.

## 9. Import and send returned payslips

Choose **Import Payroll Documents** for a provider ZIP or recognised PDF. The offered Prep Sheet DOCX option is not implemented; use a Prep Sheet PDF. Recognised payslips and supported P45/P60 documents are matched to maintained PAs; follow the period prompts where required. Inspect the import report and correct ambiguous/missing PA details rather than guessing. This is a provider-format importer, not unrestricted PDF/OCR recognition.

Choose **Email Payslips**. Review eligible PAs, the actual documents, destination addresses and any additional note, then confirm **Send**. The timesheet employer-CC option does not add that CC to PA payslip mail. Check **Payslip Delivery Status** and reconcile uncertain results before retrying. Keep imported originals and filed documents available; a registered filename alone cannot replace missing attachment bytes.

## 10. Configure and test email

Open **Email Settings**. The four tabs are **SMTP Config**, **Test Email**, **Templates** and **Signature & Options**.

- **SMTP Config:** transport, authentication, server/security and sign-in settings.
- **Test Email:** test addresses, recipient selection and the standalone connection test.
- **Templates:** timesheet subject and timesheet/payslip bodies. Use the offered insertion controls for supported fields.
- **Signature & Options:** employer CC, Payroll Department destination and employer email signature.

**Save Email Settings** saves SMTP settings, test addresses and templates. **Save Payroll Department Email** and **Save Employer Email Signature** are separate controls on **Signature & Options**. An employer email signature is separate from the image signature used on a PDF.

### Gmail, GMX and a local relay

| Service | Transport / authentication | Host and port | Security |
|---|---|---|---|
| Gmail | **SMTP Server**; password authentication | `smtp.gmail.com`, 587 | **Required STARTTLS** |
| GMX | **SMTP Server**; password authentication | `mail.gmx.com`, 587 | **Required STARTTLS** |
| Existing local relay | **Local SMTP Server**; no credentials | Fixed `localhost`, 25 | Unencrypted local connection |

For Gmail/GMX use the full mailbox address as **Username** and an application-specific password where the provider requires it. Gmail normally requires 2-Step Verification for App Passwords; availability depends on account policy. GMX project account testing required **Allow POP3 and IMAP access** in the provider's account settings. Check your own account's current requirements. v1.0.7 does not provide Google OAuth sign-in.

Changing SMTP security does not automatically correct the port. Do not select plaintext remote authentication as a workaround. The local transport neither installs a relay nor sends directly through your mailbox provider; an existing relay must accept localhost connections and secure onward delivery. Other SMTP providers can be configured using their supported settings.

### Microsoft Outlook.com, Hotmail.com and Live.com

1. Under **SMTP Config**, select **SMTP Server**, then **Microsoft OAuth2**. This selects `smtp-mail.outlook.com`, port 587 and required STARTTLS. Enter the full mailbox address.
2. Enter the **Microsoft public-client application ID** supplied for an appropriately registered application. If none has been provided, ask the application maintainer for registration instructions; do not invent an ID or paste a client secret. See [Microsoft registration requirements](MICROSOFT-OAUTH2-SMTP.md#app-registration-required-before-real-use).
3. Click **Sign in / reconnect with Microsoft**. Open the displayed Microsoft link, enter the code shown by this application, sign in and review consent. Sign-in itself sends no email. Use only a code from your own sign-in attempt.
4. After completion, click **Save Email Settings**, then send the standalone test below.

This flow supports personal Microsoft accounts, not organisational Microsoft 365/shared-mailbox setups. Password/app-password fallback for these Microsoft services is blocked. Tokens are refreshed when needed; missing/revoked authorisation or failed secure-store access requires recovery/reconnection rather than a password fallback.

**Disconnect Microsoft account** removes the draft reference; save to keep that change. **Remove local Microsoft authorization…** is a separate confirmed cleanup that also affects profiles/backups sharing that authorisation. Neither action revokes Microsoft consent or tokens already held elsewhere; review Microsoft account permissions separately when needed.

### Send a standalone test before payroll mail

In **Test Email**, enter **Payroll Test Address** or **PA Test Address**, select **Payroll** or **PA** under **Send to**, and click **Send Test Email**. It uses current draft settings. Save settings to retain them for later use.

If the SMTP username is not an email address, open **Advanced SMTP Options** and set **Explicit Test Sender**. A valid email-shaped username is otherwise used as the sender.

The standalone test has one selected recipient, no attachments, no CC/BCC and no payroll-history changes. It needs no PA, schedule or generated document. Check the recipient's inbox and junk folder separately: “SMTP accepted” is not proof of delivery. **Document email tests and previews** contains additional actions that do require eligible records/documents; these are optional and different from the standalone test.

## 11. Keep credentials secure

v1.0.7 keeps SMTP passwords and Microsoft tokens in **Windows Credential Manager**, **macOS Keychain** or a persistent **Linux Secret Service**. Saved configuration contains references, not newly saved plaintext passwords. Allow access to your own trusted credential store; on Linux ensure a persistent desktop keyring is available and unlocked.

Migration from legacy plaintext settings verifies secure storage before replacing the password field. A failed migration retains the original recoverable file and blocks authenticated sending. Old v1.0.6 backups may still contain plaintext passwords; upgrading does not erase historical copies.

A copied database/configuration backup does not transport OS credentials to another account or computer. If the matching entry is missing, unlock/recover the store, re-enter the SMTP password or reconnect Microsoft. Clearing/changing a password does not automatically delete historical entries used by older backups. Do not share raw configuration, vault contents, payroll databases or authentication codes when asking for help.

## 12. Keep a recovery copy

Open **Settings → Backup & Restore → Create Backup**. Keep the reported location safe.

The backup contains a verified SQLite snapshot, including committed WAL data, PAs, hours, payroll records/history and embedded PDF/attachment bytes. It also includes `config.toml` **when present**: appearance, folder paths, CSV settings, payroll/PDF/email preferences and credential references.

It is not a complete profile backup. External PDFs, payslips, signatures, original CSV files, import/archive files, fonts and other filesystem content are not copied. Their known locations are listed in the recovery manifest. Protect external records separately where needed. OS credential-store secrets are not exported. Do not copy just an open SQLite main file and assume its WAL data is included.

## 13. Restore carefully

Close other application instances. In **Settings → Backup & Restore**, select a backup and choose **Restore Selected Backup...**. Review the affected data, enter the reason and explicitly acknowledge the rollback before confirming.

Restore validates the backup, migrates supported older data on an isolated copy and makes a safety backup of the current database/configuration first. Newer submissions or delivery evidence may leave the active database, so reconcile with Payroll to prevent duplicate processing. Unresolved email uncertainty blocks unsafe restore.

Configuration is replaced only if the selected backup includes it; otherwise current configuration is kept. External files and OS vault entries are not restored. Close and restart the application after restore. If database restoration succeeds but configuration publication fails, preserve the safety backup and follow the restart/recovery message. Do not use an older executable to overwrite schema-42 data. See [recovery details](DATABASE-SCHEMA.md#verified-startup-upgrades-and-legacy-restore).

## 14. Common problems

| Problem | What to check |
|---|---|
| No payroll period | Import the provider's Prep Sheet through **Import Payroll Documents**, then check **View Payroll Schedule**. |
| No eligible PA/document | Check PA status, selected period, saved preparation, generation/import results and any unresolved delivery. Current recipient lists exclude departed PAs. |
| CSV rejected or hours wrong | Check introductory rows, heading aliases, selected hours method, explicit breaks where required and unambiguous PA names. Inspect duplicate/error reports before importing again. |
| PDF cannot be generated | Check configured output folder, font files, effective pay rates, preparation validation and signature messages. |
| File path/storage warning | Check permissions and the configured location. Do not bypass ownership/ACL safeguards or run as administrator merely to suppress the warning. Preserve existing data before changing paths. |
| Password/store error | Unlock the relevant store; check the saved account/server settings. Re-enter the password or reconnect if the entry is missing. No plaintext fallback is available. |
| Microsoft sign-in fails | Check connectivity, public-client ID and personal-account support. A browser working does not prove the application's network/proxy route works. Read the displayed category; do not share codes or tokens. |
| SMTP rejects authentication | Recheck account, application password or OAuth consent and provider SMTP policy. Do not weaken TLS to make authentication work. |
| Send appears stalled or uncertain | Do not resend blindly. Check delivery status and reconcile with the recipient before using the documented review. |
| Dates look different | Select the preferred display under **Settings → Appearance**. Check the actual date; examples in this guide are UK day/month/year. |
| Save did not finish | Read the status message. Some settings have independent saves or partial-save messages; retry the failed part. |
| Controls seem missing | Enlarge the window or use its scrollbars; narrow Dashboard layouts can scroll horizontally. |

When requesting help, provide the application version, screen, action and redacted error message. Remove names, addresses, National Insurance numbers, payroll attachments and all authentication material.
