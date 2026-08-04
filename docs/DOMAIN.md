# DirectPaymentTimesheets Domain Guide

## Purpose

This document describes the real-world concepts that DirectPaymentTimesheets is designed to represent.

The purpose is to keep the software aligned with the needs of UK Direct Payment administration and the actual workflow used by Direct Payment holders.

---

# Direct Payments

A Direct Payment allows a person who receives care and support funding to manage their own support arrangements.

The Direct Payment holder can use the funding to employ or arrange support from Personal Assistants (PAs).

The application is designed to support the administration required around these arrangements.

---

# Direct Payment Holder (Employer)

The Direct Payment holder is the person responsible for managing their care funding and employment arrangements.

They may:

- Employ Personal Assistants.
- Approve timesheets.
- Maintain employment records.
- Prepare payroll information.
- Keep financial and audit records.

The employer record contains information required for:

- Timesheet documents.
- Payroll submissions.
- Generated PDFs.

---

# Personal Assistant (PA)

A Personal Assistant provides support to the Direct Payment holder.

A PA may have:

- Working hours.
- Agreed pay rates.
- Employment information.
- Holiday entitlement.
- Optional system access.

A Personal Assistant does not automatically require a login account.

The system should allow:

- PAs who only have hours recorded.
- PAs who can submit their own hours in future versions.

Authentication and employment records should remain separate.

---

# Timesheets

A timesheet records work completed by a Personal Assistant.

A timesheet entry may contain:

- Personal Assistant.
- Date worked.
- Start time.
- End time.
- Break duration.
- Worked duration.
- Hourly rate.
- Calculated amount.
- Notes.

Timesheet records should be:

- Accurate.
- Traceable.
- Auditable.
- Preserved historically.

Imported records should not be changed after payroll processing without an audit record.

---

# Working Time

Working time represents hours a Personal Assistant has provided support.

The application records:

- Start of work period.
- End of work period.
- Breaks taken.
- Total worked minutes.

Working time calculations should be consistent and transparent.

---

# Pay Rates

A Personal Assistant may have different hourly rates over time.

The system must support:

- Current hourly rate.
- Historical rates.
- Effective dates.
- Employer-funded top-up rates.

Historical timesheets must retain the rate that applied when the work was completed.

A future payroll calculation must not recalculate old records using a new rate.

---

# Payroll Periods

Payroll periods group completed records into payment cycles.

Examples:

- Four-weekly payroll.
- Monthly payroll.
- Special payroll runs.

The application should support the Direct Payment workflow where timesheets are prepared and submitted to payroll departments.

---

# Annual Leave

Annual leave is separate from worked hours.

Annual leave records should store:

- Personal Assistant.
- Date commencing.
- Date ending.
- Hours taken.

The dates are a record of when the leave occurred.

The hours are used when producing payroll documentation.

Annual leave should not appear as normal worked hours in the source timesheet data.

When generating payroll PDFs, annual leave hours may need to be placed into the correct week of a four-week payroll period.

---

# Sick Leave and SSP

The system may support sick leave and Statutory Sick Pay (SSP) in future versions.

Some Direct Payment employment arrangements may require these records.

The feature should remain optional because individual employers may not use sick leave payments.

---

# Public Holidays

Public holidays require special handling.

The system should maintain a list of public holiday dates.

During CSV import:

- Imported work dates should be checked against public holiday dates.
- Hours worked on public holidays should be identified automatically.
- Multiple shifts on the same public holiday should be combined.

Example:

Three imported shifts:

```
1.25 hours
2.00 hours
2.25 hours
```

On:

```
25/12/2026
```

Should become:

```
5.50 hours (25/12/2026)
```

Public holiday hours are recorded separately for payroll reporting.

They should not simply replace normal worked hours.

---

# Travel and Mileage

Some employment arrangements may include travel claims.

The system should support future mileage recording.

Possible information:

- Miles claimed.
- Mileage rate.
- Total mileage payment.

Example:

```
Miles claimed @ £0.40 per mile
```

Travel should remain separate from worked hours.

---

# PDF Timesheet Layout

Generated payroll documents should support the existing four-week timesheet format.

Expected columns:

## Column 1

```
W/C date
(Week commencing date)
```

## Column 2

```
Hours worked

Pay Rate
```

## Column 3

```
Annual leave hours
```

## Column 4

```
Sick leave

SSP
```

## Column 5

```
Public Hols. Hours worked
```

## Column 6

```
Travel

Miles claimed @ £0.40 per mile
```

Unused columns should remain available because other employers may require them.

---

# Import Records

External timesheet information may be imported from sources such as Hours Keeper CSV files.

The import process should:

- Validate incoming data.
- Prevent duplicate records.
- Preserve original files.
- Record audit information.
- Detect public holiday dates.

---

# Audit Trail

An audit trail records important system events.

The application records:

- Import activity.
- Changes to important records.
- Payroll preparation events.
- Generated documents.

Audit information helps maintain trust and accountability.

---

# Future User Accounts

Future versions may support optional user access.

Possible users:

- Employer / Administrator.
- Personal Assistant.
- Payroll administrator.

A user account should only exist when access is required.

---

# Future Payroll Engine

The Payroll Engine will transform approved records into payroll information.

Responsibilities may include:

- Grouping work into payroll periods.
- Applying pay rates.
- Handling annual leave.
- Handling public holiday reporting.
- Preparing payroll outputs.
- Generating PDF documents.
- Preparing payroll emails.

The Payroll Engine should use validated records rather than raw imported files.

---

# Privacy Principles

Direct Payment administration contains sensitive personal information.

The application should:

- Store only necessary information.
- Keep data local where possible.
- Protect personal records.
- Avoid unnecessary personal details in filenames.
- Maintain clear audit history.

---

# Domain Development Principle

The software should reflect the real-world Direct Payment process.

Business rules should be:

- Clearly documented.
- Separated from technical implementation.
- Tested where possible.
- Changed carefully when requirements or regulations change.

