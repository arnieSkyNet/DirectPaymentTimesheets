# v1.0.6 setup screenshots: capture and isolation record

Captured on **15/10/2026**, on Florence, for the [First-Time Setup Guide](../../FIRST-TIME-SETUP.md). All people, addresses, identifiers, employment arrangements, schedule dates and work records are fictional. Email addresses use the reserved `example.com` domain. The masked SMTP password is a dummy value, never a real credential.

## Coverage and authenticity

There are **68 numbered PNGs**: **65 application views**, **two native Linux file-chooser views** (26–27), and **one PDF-reader view of the application's actual generated PDF** (62). Images illustrate first launch, employer and provider setup, SMTP examples, schedule import and verification, PA details, dated hours/pay, direct shifts, Hours Keeper CSV, preparation, generation, preview and final confirmation.

The application was built from an archive of tag **v1.0.6**, commit `e157cbd754020e250806b07d40d0fd1a8e755938`, using `cargo build --locked --offline` in a separate temporary source/target directory. All **169 archived release files** were byte-verified against the tag's Git blob objects after the build. Application source was not edited. The installed application was not used or replaced; the working branch was not changed.

Build executable SHA-256:

```text
bdcfb1a972668473e51bd4f0e8f20e129335645d98bcb39af817988f032e0da7
```

Captures came from a private Xvfb desktop, not the user's desktop. The real window was captured with ImageMagick. Cropping removed unrelated space; orange rectangle outlines identify controls, and numbered titles/captions occupy added margins outside the captured UI. Nothing inside the captured UI was repainted or synthesised. There was no AI image generation. The [capture manifest](capture-manifest.json) lists every filename, source-capture digest, crop, highlight rectangle, output dimensions and final digest.

## Path resolution checked before launch

The release's [environment.rs](https://github.com/arnieSkyNet/DirectPaymentTimesheets/blob/v1.0.6/src/environment.rs) gives `DIRECTPAYMENTTIMESHEETS_HOME` priority **verbatim**. Without it, Unix uses `$HOME/.directpaymenttimesheets`; Windows uses `%LOCALAPPDATA%/DirectPaymentTimesheets`. An override alone does not isolate all business files.

[context.rs](https://github.com/arnieSkyNet/DirectPaymentTimesheets/blob/v1.0.6/src/context.rs) opens `database.sqlite` and loads `config.toml` under that selected data root. [config.rs](https://github.com/arnieSkyNet/DirectPaymentTimesheets/blob/v1.0.6/src/config.rs) separately configures five business folders; on Unix their defaults start with `~/Documents/DirectPaymentTimesheets`. [paths.rs](https://github.com/arnieSkyNet/DirectPaymentTimesheets/blob/v1.0.6/src/paths.rs) expands `~/` through `dirs::home_dir()`. Existing absolute folder or signature paths must therefore never be copied from a production configuration.

The screenshot session started with an empty, newly created temporary directory. No existing application database, configuration, document, signature, cache or credential was copied or inspected. Before the first application launch, a preflight asserted that no SQLite database or symlink existed in the demonstration tree, every configured business folder resolved inside `/demo`, and production home/runtime paths were absent.

## Enforced isolation

The application ran inside Bubblewrap with separate user, process, mount, IPC and network namespaces. Its root exposed read-only system executables/libraries, fonts and a few non-personal system configuration files. It had **no host `/home` directory**, no host user runtime directory, no production X display, and no production session bus. Synthetic passwd/group/machine identity files were supplied. The sandbox's `/demo` and `/tmp` were backed solely by the newly created temporary session directory; other writable mounts were disposable namespace-local pseudo-filesystems. No host production directory was mounted, even read-only.

| State | Location inside the sandbox |
| --- | --- |
| HOME | `/demo/home` |
| Application data override | `/demo/app` |
| Database, including SQLite sidecars/recovery files | `/demo/app/database.sqlite` and siblings |
| Configuration | `/demo/app/config.toml` |
| Application cache, archive, backups, logs, templates | Corresponding subdirectories of `/demo/app` |
| CSV import | `/demo/Documents/DirectPaymentTimesheets/import` |
| Generated PDFs and retained timesheet files | `/demo/Documents/DirectPaymentTimesheets/pdf` |
| Email archive/attachment output | `/demo/Documents/DirectPaymentTimesheets/emails` |
| Payslips | `/demo/Documents/DirectPaymentTimesheets/payslips` |
| Payroll information and stored preparation sheet | `/demo/Documents/DirectPaymentTimesheets/payroll-information` |
| XDG configuration/cache/data/runtime | `/demo/config`, `/demo/cache`, `/demo/data`, `/demo/runtime` |
| Temporary files | `/tmp`, bound to the disposable session's own temporary folder |
| Process traces and capture logs | `/demo/logs` |
| Original fictional input PDF and CSV | `/demo/Documents` |
| Raw captures | `/demo/raw` |
| PDF-reader copy | `/tmp/fictional-timesheet.pdf`, identical to the generated PDF |

Every business folder was set to an explicit absolute `/demo/...` path **before launch**. A fresh, dummy-only config was supplied; it did not inherit host settings. Physical signatures were left unconfigured. The Linux file chooser needed local-only GIO and software GTK rendering; these were process environment settings inside the sandbox. No production desktop or system settings were changed. Xvfb was downloaded and extracted into disposable storage, not installed on the host.

The network namespace contained **loopback only**, with no external interface or route and no SMTP relay. Thus even the displayed real provider hostnames could not reach GMX or Gmail. No real credentials were available. Automatic holiday/update requests could not leave the namespace; the visible offline holiday warnings are genuine. No external authentication, test-email transmission or production send was attempted.

## Verification

- The real application imported a fictional readable preparation PDF as **13 periods**.
- UI actions saved **one employer**, **one PA**, **one dated pay rate** and **one dated contracted-hours record**.
- Direct entry created **one completed shift**. CSV import created **four imported rows**, without failures or skipped rows. Worked totals were **10, 6, 0, 0 hours** for the four weeks.
- The application saved preparation and generated the PDF. Its text and visual output were inspected, including the demonstration-only note and blank signature boxes.
- Email preview showed the fictional From/To/CC/BCC and generated attachment. Final confirmation was captured, then **Cancel** was used. No Test Email or Send button was clicked.
- The final database had **zero** rows in `timesheet_delivery_attempts`, `payroll_bundle_attempts` and `payroll_timesheet_email_status`; the application displayed **Current PDF not sent**.
- Application file/connect traces contained no production-home paths and no SMTP connections on ports 25, 465 or 587. The mount/network preflight and database counts are summarised in the manifest.
- Reopening the isolated app retained the schedule and cleared the stale first-import warning, verifying the guide's troubleshooting instruction.
- All 68 finished images were visually reviewed for matching controls and fictional content. No personal host paths or real credentials are intentionally included. `/demo` is the synthetic sandbox path; any system font paths in the underlying UI are generic system locations.

## Deliberate limits

These are genuine **Linux source-build** screenshots, not screenshots of Windows/macOS installers. They use the light theme and UK date display. The file chooser varies by desktop. Optional drawing/signature import and backup/restore were not exercised. Live SMTP authentication, actual test-email delivery, accepted submission, resending and receipt were deliberately not tested or depicted as successful. Existing GMX and Microsoft account-testing statements in the guide come from the user's supplied **15/10/2026** testing, not this screenshot session.

No production application state was accessed. Only documentation and screenshot assets were added to the repository; nothing was committed or pushed. Temporary build/demo material was retained rather than deleted.
