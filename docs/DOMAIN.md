# DirectPaymentTimesheets Domain Guide

## Purpose

This document describes the real-world concepts that DirectPaymentTimesheets is designed to represent.

The purpose is to keep the software aligned with the needs of UK Direct Payment administration.

---

# Direct Payments

A Direct Payment allows a person who receives care and support funding to manage their own support arrangements.

The Direct Payment holder can use the funding to employ or arrange support from Personal Assistants (PAs).

The application supports the administration required around these arrangements, including:

- Recording worked hours.
- Maintaining employment information.
- Preparing payroll documentation.
- Keeping accurate audit records.

---

# Direct Payment Holder

The Direct Payment holder is the person responsible for managing their care funding.

They may:

- Employ Personal Assistants.
- Approve timesheets.
- Manage payroll information.
- Maintain employment records.
- Keep financial and audit records.

The application should support the holder by reducing administration while keeping records accurate.

---

# Employer

The employer is the person or organisation responsible for employing Personal Assistants.

The system stores employer information including:

- Employer name.
- Address.
- Contact details.
- Payroll provider information.
- Payroll document settings.

---

# Personal Assistant (PA)

A Personal Assistant provides support to the Direct Payment holder.

A PA may have:

- Personal information.
- Employment information.
- Contracted hours.
- Variable or fixed working arrangements.
- Pay rate history.
- Holiday records.

The application stores information relating to the PA's employment and payroll preparation.

---

# Employment Records

Employment information may change over time.

The system should preserve historical records for:

- Employment start information.
- Contracted hours.
- Fixed or variable hour arrangements.
- Changes of circumstances.

Example:

```
Fixed hours:
18 hours per week
```

or:

```
Variable hours:
Hours vary according to support requirements
```

Historical records ensure previous payroll information remains accurate.

---

# Worked Shifts

A worked shift represents actual time worked by a Personal Assistant.

The initial source of worked shifts is Hours Keeper CSV import.

A worked shift may contain:

- Personal Assistant.
- Date.
- Start time.
- End time.
- Break duration.
- Worked duration.
- Notes.
- Import source.

Worked shifts represent actual work completed.

They do not include:

- Annual leave.
- Sick leave.
- Travel claims.
- Other payroll adjustments.

---

# Timesheets

A payroll timesheet is created by combining worked shifts and relevant adjustments.

A timesheet may include:

- Hours worked.
- Pay rate applied.
- Annual leave hours.
- Public holiday hours.
- Optional payroll adjustments.

Timesheet records should be accurate, traceable and auditable.

---

# Working Time

Working time represents the hours a Personal Assistant has provided support.

The application records:

- Start of work period.
- End of work period.
- Breaks taken.
- Total worked minutes.

Working time calculations should be consistent and transparent.

---

# Pay Rates

A Personal Assistant may have an agreed hourly rate.

The system should support:

- Recording hourly rates.
- Recording government/minimum rates.
- Recording employer top-ups.
- Calculating the total hourly payment rate.
- Recording future rate changes.
- Maintaining historical accuracy.

Previously completed payroll records should preserve the rate that applied at that time.

---

# Payroll Periods

Payroll periods group completed work into a payment cycle.

The application supports:

- Four-week payroll cycles.
- Payroll submission dates.
- PA pay dates.
- Special payroll arrangements.

Payroll periods are used when generating payroll preparation documents.

---

# Import Records

External timesheet information may be imported from sources such as Hours Keeper CSV files.

The import process should:

- Validate incoming data.
- Prevent duplicate records.
- Preserve original files.
- Record audit information.
- Classify worked hours where additional rules apply.

---

# Public Holiday Calendar

Public holidays are stored separately from worked shifts.

The system maintains a calendar containing:

- Date.
- Holiday name.
- Country.

During import, worked shifts are checked against the public holiday calendar.

If hours are worked on a public holiday, those hours are automatically shown as public holiday hours on the payroll document.

Example:

```
25/12/2026

Imported shifts:
1.25 hours
2 hours
2.25 hours

Public holiday hours:
5.5 (25/12/2026)
```

The system records the hours.

The Payroll department determines the payment treatment.

---

# Annual Leave

Annual leave is a separate record from worked shifts.

Annual leave is entered manually.

Records include:

- Personal Assistant.
- Date commencing.
- Date ending.
- Hours.

The dates provide a record of when annual leave occurred.

The system uses the date range and payroll period to place the correct annual leave hours into the appropriate payroll document section.

---

# Optional Payroll Adjustments

Some payroll columns exist because they are part of the Payroll department template.

## Sick Leave / SSP

Sick leave is supported as an optional future adjustment.

The column may appear on payroll documents but is not part of the normal workflow.

No automatic SSP calculation is required initially.

## Travel

Travel claims may be supported in future.

Information may include:

- Miles claimed.
- Mileage rate.

---

# Payroll Preparation Document

The final output is a Payroll Preparation Sheet matching the Payroll department's required format.

The document contains:

Header information:

- Employer name.
- Employee name.
- National Insurance number.
- Contracted weekly hours.

Main columns:

1. Week commencing date.

2. Hours worked and pay rate.

3. Annual leave hours.

4. Sick leave / SSP.

5. Public holiday hours worked.

6. Travel miles claimed.

---

# Audit Trail

An audit trail records important system events.

The application records import activity including:

- When an import occurred.
- Which file was imported.
- How many rows were processed.
- How many records were created.
- How many records were skipped.
- Whether the import succeeded or failed.

Audit information helps maintain trust and accountability.

---

# Privacy Principles

Direct Payment administration contains sensitive personal information.

The application should:

- Store only necessary information.
- Keep user data local where possible.
- Avoid exposing unnecessary personal information.
- Maintain clear audit history.
- Prevent accidental sharing of private records.

---

# Future Payroll Engine

The Payroll Engine will transform validated worked shift records and payroll adjustments into payroll information.

Future responsibilities may include:

- Grouping work into payroll periods.
- Calculating gross pay.
- Supporting additional payroll rules.
- Preparing payroll outputs.

The Payroll Engine should use validated records rather than raw imported files.

---

# Domain Development Principle

The software should reflect the real-world Direct Payment process.

Business rules should be:

- Clearly documented.
- Separated from technical implementation.
- Tested where possible.
- Changed carefully when regulations or requirements change.

