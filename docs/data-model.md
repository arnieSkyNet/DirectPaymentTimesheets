# DirectPaymentTimesheets Data Model

## Document Purpose

This document describes the initial data model for DirectPaymentTimesheets.

The purpose is to define the information the application needs to store before implementing the database layer.

The first implementation milestone focuses on importing and storing timesheet records.

---

# 1. Core Concept

The primary record in the system is a timesheet entry.

A timesheet entry represents a period of work completed by a Personal Assistant (PA) for a Direct Payment / Personal Budget arrangement.

Each entry should contain enough information to:

- Record hours worked.
- Calculate pay.
- Support approval workflows.
- Generate reports and documents.

---

# 2. Initial Timesheet Entry

The first version of the system will contain:

```
TimesheetEntry

id
pa_name
date
start_time
end_time
break_minutes
notes
```

---

# 3. Field Descriptions

## id

Purpose:

Unique identifier for the record.

Future database implementation:

- SQLite integer primary key.

---

## pa_name

Purpose:

The name of the Personal Assistant who completed the work.

Future improvements:

- Replace free text with a separate PA table.
- Allow multiple assistants.
- Store contact and payroll details.

---

## date

Purpose:

The date the work took place.

Used for:

- Weekly calculations.
- Payroll periods.
- Reports.

---

## start_time

Purpose:

The time the PA started work.

Used for:

- Calculating hours worked.
- Displaying timesheets.

---

## end_time

Purpose:

The time the PA finished work.

Used for:

- Calculating hours worked.
- Payroll calculations.

---

## break_minutes

Purpose:

The unpaid break duration in minutes.

Used for:

- Accurate paid hours calculation.

Example:

```
Start: 09:00
Finish: 17:00
Break: 60 minutes

Paid time: 7 hours
```

---

## notes

Purpose:

Optional additional information about the entry.

Examples:

- Reason for additional hours.
- Special circumstances.
- Approval comments.

---

# 4. Future Expansion

The initial model is intentionally simple.

Future versions may add:

## Personal Assistant

Information such as:

- Name.
- Contact details.
- Pay rate.
- Employment details.

## Approval

Information such as:

- Approved by.
- Approval date.
- Approval status.

## Payroll

Information such as:

- Hourly rate.
- Pay period.
- Payroll submission status.

## Settings

Information such as:

- Currency.
- Week start day.
- Pay frequency.

---

# 5. Design Principle

The data model should evolve with the application.

The first version should remain simple while allowing future expansion without major redesign.

The database structure will be based on this model.

