# Offline recovery from a known-good backup

Users maintain their backups. If DirectPaymentTimesheets cannot open or validate
the live database, stop payroll work and recover from a known-good backup. In-app
restore expects a readable live database so it can compare evidence and obtain
approval; it is not a recovery path for every corrupt or unreadable database.

1. **Close all application instances normally.** Let active sends finish and keep
   their results. Do not force closure to bypass an uncertain delivery. If normal
   closure is blocked, seek assistance; do not run experimental terminal fixtures.
2. **Preserve the damaged installation before changing it.** Keep the entire
   application data directory, including database.sqlite, any WAL/SHM/journal,
   configuration, backups and private staging remnants. Do not copy just the main
   file of an open database. Do not edit SQL, reset the schema, delete sidecars,
   clear read-only flags or relax permissions to make it open.
3. **Choose and validate a known-good backup.** Check its date and successful
   creation evidence. Application backups carry a README identity manifest; a
   `.incomplete` marker means the backup is not eligible. Never remove that marker
   to bypass refusal. Have integrity and application/schema compatibility checked
   on a separate copy with supported validation tools or qualified assistance.
   Keep the original backup unchanged. If safe validation is unavailable, stop.
4. **Restore into a clean private application data directory while closed.**
   Preserve the old directory separately instead of overwriting its evidence.
   Copy the validated database and, when appropriate, its matching configuration
   into the clean directory. Do not place old live WAL/SHM/journal files beside the
   restored database. Do not attach a backup copied from a still-running source.
   Keep the existing owner/access policy requirements; failures need separate
   reviewed repair, not permission bypasses. A directory that cannot be moved may
   still be in use; do not force it.
5. **Restore external files separately.** Database/configuration backups do not
   copy the complete PDFs, payslips, signatures, fonts, imports or business folders.
   Retained submission/attachment bytes inside SQLite do not replace that estate.
   Use separately protected copies and review their consistency with the backup
   date. Configuration can point outside the application directory; changing
   DIRECTPAYMENTTIMESHEETS_HOME does not redirect those business folders.
6. **Restart the matching or newer supported application and review.** Use the
   existing verified migration/validation path if an upgrade is needed. Do not
   open newer data with an older build or force a failed upgrade. Confirm the
   intended employer, payroll period, documents and configuration before work.
   Vault references are not exported secrets: another account/computer or missing
   vault entries may require SMTP password re-entry through normal settings.
7. **Reconcile work and sending after the backup date.** Later changes may be lost.
   Restored delivery evidence can be older than actual SMTP acceptance or Payroll
   processing. Check retained documents, email evidence and Payroll before any
   resend. Preserve uncertainty and use the existing audited review tools where
   applicable; never treat old unsent state as proof that a message was not sent.

Default internal data locations are `~/.directpaymenttimesheets` on Unix and
`%LOCALAPPDATA%\DirectPaymentTimesheets` on Windows, unless the application's
explicit data-root override was used. Check the actual configured installation;
these instructions do not authorise changes to another profile or business folder.
There is no automatic catastrophic-recovery engine or independent observer journal.

## Verification boundary — 1 November 2026

[macOS acceptance run 41305531150](https://github.com/arnieSkyNet/DirectPaymentTimesheets/actions/runs/41305531150)
compiled the application and passed 43 exact safe synthetic tests per architecture
on Intel and Apple Silicon. Candidate:
`d9b046bed1739843f854e57968ab5ce9947b26ce`; controller:
`1afde972f46f0f66438952bbd56e6e8ec0aa1d6c`. The selected backup/recovery tests
cover isolated staging, WAL-inclusive copying, failure preservation and protected
installation. They do not certify recovery from arbitrary corruption, power loss,
external-file restoration or an interactive real-profile restore. No production
data was used. These results support the bounded implementation described in
[ownership status](SQLITE-CONNECTION-OWNERSHIP.md); they do not relax any recovery
step or documented limitation above.
