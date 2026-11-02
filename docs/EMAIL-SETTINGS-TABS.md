# Email Settings tabs — v1.0.7 development

The existing settings now use four tabs. The selected tab, selected test recipient
and preview type are view state only. No configuration keys or database schema
have been added or migrated. Drafts continue to use the same existing config,
employer-signature and payroll-provider editors.

| Tab | Contents |
| --- | --- |
| SMTP Config | Transport and security controls side by side; server/port/username on one row; masked password or Microsoft OAuth2 sign-in/reconnect, cancellation and disconnect controls; existing TLS and credential-store guidance. |
| Test Email | Existing Payroll and PA test-address editors, recipient selection, prominent Send Test Email and Preview Email buttons, and the existing optional explicit test sender under Advanced SMTP Options. Document-dependent test/preview actions are preserved in a collapsed section below. |
| Templates | Existing timesheet subject/placeholder insertion and timesheet/payslip body editors, plus Preview Email using fictional details and the draft signature. |
| Signature & Options | Existing employer email signature with its Save button, employer timesheet-CC preference, and payroll department address with its separate Save button. |

SMTP Config is the initial tab. Tab selection and test recipient selection remain
in memory across navigation in the current app session. Clicking a tab gives it
keyboard focus; Tab and Space/Enter can navigate/activate the tab controls.
Switching tabs never saves, clears, reloads or duplicates existing draft values.
Merely viewing optional signature/address editors preserves absent (`None`)
values; their retained drafts change only when an editor is changed.

## Footer and saving

One **Save Email Settings** button and status area remain fixed beneath the
scrolling tab contents, together with the existing navigation controls. The tab
bar is outside the scroller. Content scrolls only when the available height is
insufficient; each tab retains its own scroll position. Server fields adapt to
normal window widths rather than forcing a 900-pixel-wide form. Test-address
and template editors stack on narrow windows.

Save Email Settings retains its existing scope: SMTP settings, test addresses,
email preferences and templates. Signature and payroll department address retain
their own explicit saves because they belong to existing database records.
Persistent footer indicators identify unsaved signature/address edits on every
tab, and explanatory text identifies the separate save controls. Saving config
does not discard those drafts. Microsoft sign-in progress and completion/error
feedback remain available when viewing other tabs; View sign-in returns to SMTP
Config while a sign-in worker is active.

## Test and preview behaviour

**Send Test Email** sends the existing short, attachment-free connection test to
the selected Payroll or PA test address only. It needs a valid sender/recipient
and the existing transport/credential requirements, but no PA record, payroll
cycle, employer, PDF or document history. The unselected test address is not a
prerequisite. There is no CC/BCC, attachment generation or payroll-history write.
Duplicate sends remain blocked while the worker is running; uncertain delivery
and closure protections are unchanged. Success, rejection and uncertain-delivery
messages remain visible in the footer across tab switches.

The Test Email preview shows this connection-test wording and selected address.
Missing sender/recipient values are marked rather than preventing preview.
The Templates preview uses current draft subject/body/signature and fictional
addresses, PA details and attachment names. It never opens attachments or
requires SMTP configuration. Deliberately empty draft bodies/signatures are
shown empty rather than silently replaced with defaults.

Preview opens as a separate OS window when supported; otherwise it uses a
resizable embedded egui window. An open preview refreshes after draft edits.
Preview is never a send action, and template edits do not change the short
connection-test message. The selected preview type changes when its Preview
Email button is clicked.

## Security and acceptance boundaries

The Microsoft OAuth2 protocol, native credential-store implementation, TLS
requirements and provider authentication paths are unchanged. Tabs introduce no
new secret persistence. OAuth2 access/refresh tokens and device codes remain
outside config, backups and diagnostic output; sign-in codes are displayed only
by the existing transient SMTP sign-in controls. Existing Gmail, GMX, password
SMTP and unauthenticated local relay settings are retained.

Automated coverage uses headless egui frames, synthetic temporary profiles,
mock workers and loopback SMTP servers. It checks layout/footer visibility,
keyboard tab activation, retained drafts and explicit saves, both standalone
recipients, independent/live previews, visible outcomes and document integrity
checks. It does not launch the application for interactive acceptance, access a
live mailbox or execute native terminal SQLite fixtures.

Interactive visual, platform-window and real-account acceptance remains for the
planned joint review. Microsoft application-registration and live native-vault
acceptance requirements remain in [MICROSOFT-OAUTH2-SMTP.md](MICROSOFT-OAUTH2-SMTP.md).

## Automated validation — 2026-11-01, Linux

- Focused settings/UI tests: **27 passed**.
- Final combined regression selection: **126 passed, 0 failed, 1 intentionally
  ignored native credential-store roundtrip**. The focused tests are included
  in this selection and must not be added to its count.
- `cargo check --offline` and `git diff --check`: passed. Existing dead-code
  warnings remain.

```text
cargo test --offline --bin direct_payment_timesheets -- microsoft smtp email_service email_settings config::tests gui::standalone_email_ui_tests gui::preparation_navigation_tests backup_service::tests --test-threads=2
```

Headless layout checks cover 400×400, 600×500 and larger windows, including the
fixed Save/status/navigation footer. State checks include all four tab selectors,
keyboard activation, real text editing of optional fields, both test recipients,
separate signature/address saves and expanded document tools. The configuration
schema, OAuth2 module, credential-store abstraction and SMTP transport source
remain unchanged. Interactive acceptance is deliberately deferred.
