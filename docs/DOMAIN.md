# DirectPaymentTimesheets Domain Guide

## Purpose

This document describes the real-world concepts that DirectPaymentTimesheets is designed to represent.

The purpose is to keep the software aligned with the needs of UK Direct Payment administration.

---

# Direct Payments

A Direct Payment allows a person who receives care and support funding to manage their own support arrangements.

The Direct Payment holder can use the funding to employ or arrange support from Personal Assistants (PAs).

The application is designed to support the administration required around these arrangements.

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

# Personal Assistant (PA)

A Personal Assistant provides support to the Direct Payment holder.

A PA may have:

- Working hours.
- Agreed pay rate.
- Employment information.
- Holiday entitlement.
- Sick leave records.

The application stores timesheet information relating to PA work.

---

# Timesheets

A timesheet records work completed by a Personal Assistant.

A timesheet entry may contain:

- Personal Assistant name.
- Start time.
- End time.
- Break duration.
- Worked duration.
- Hourly rate.
- Calculated amount.
- Notes.

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
- Calculating payment amounts.
- Future rate changes.
- Historical accuracy.

Previously imported records should preserve the rate that applied at that time.

---

# Payroll Periods

Payroll periods group completed work into a payment cycle.

Examples:

- Weekly payroll.
- Four-weekly payroll.
- Monthly payroll.

Future payroll functionality will use imported timesheet records to create payroll periods.

---

# Import Records

External timesheet information may be imported from sources such as Hours Keeper CSV files.

The import process should:

- Validate incoming data.
- Prevent duplicate records.
- Preserve original files.
- Record audit information.

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

# Leave Management

Future functionality may include:

## Annual Leave

Records of Personal Assistant holiday entitlement and usage.

## Sick Leave

Records of sickness absence and related payments.

## Public Holidays

Support for calculating relevant holiday rules.

These areas should be implemented carefully because employment rules can vary.

---

# Employment Records

Future versions may include additional employment information such as:

- Employment start dates.
- Contracted hours.
- Pay history.
- Leave balances.
- Employment status.

Sensitive information should only be stored when required.

---

# Privacy Principles

Direct Payment administration contains sensitive personal information.

The application should:

- Store only necessary information.
- Keep user data local where possible.
- Avoid exposing personal information in filenames.
- Maintain clear audit history.
- Prevent accidental sharing of private records.

---

# Future Payroll Engine

The Payroll Engine will transform approved timesheet records into payroll information.

Future responsibilities may include:

- Grouping work into payroll periods.
- Calculating gross pay.
- Applying leave rules.
- Supporting adjustments.
- Preparing payroll outputs.

The Payroll Engine should use validated timesheet data rather than raw imported files.

---

# Domain Development Principle

The software should reflect the real-world Direct Payment process.

Business rules should be:

- Clearly documented.
- Separated from technical implementation.
- Tested where possible.
- Changed carefully when regulations or requirements change.
