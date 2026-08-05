# DirectPaymentTimesheets Data Model

## Document Purpose

This document describes the data model used by DirectPaymentTimesheets.

The purpose is to define the information the application stores and the relationships between the main business records.

The system is designed to support UK Direct Payment administration, including:

- Personal Assistant records.
- Timesheet records.
- Payroll preparation.
- Pay rate history.
- Leave management.
- Future self-service features.

---

# 1. Core Design Principles

The database must:

- Preserve historical information.
- Avoid overwriting previous payroll information.
- Keep employment records separate from login/security records.
- Support future expansion.
- Maintain an audit trail.
- Keep user data separate from application source code.

---

# 2. Main Entities

Current entities:

```
Employer

PersonalAssistant

PersonalAssistantPayRate

TimesheetEntry

ImportAudit
```

Future entities:

```
UserAccount

AnnualLeaveRecord

PublicHoliday

PayrollPeriod
```

---

# 3. Employer

An employer represents the Direct Payment holder responsible for managing Personal Assistants.

Current fields:

```
id

name
address
postcode
telephone
email

payroll_provider
payroll_provider_address
payroll_provider_phone

employer_signature

default_pdf_template
```

Purpose:

Stores employer details used for:

- Timesheet generation.
- Payroll preparation.
- PDF documents.

---

# 4. Personal Assistant

A Personal Assistant represents an employee providing support.

Current fields:

```
id

first_name
surname

date_of_birth

national_insurance_number

address
postcode
telephone
email

employment_status
```

Purpose:

Stores employment information required for:

- Timesheets.
- Payroll administration.
- Future leave calculations.

---

# 5. Personal Assistant Pay Rates

Pay rates are stored separately from the Personal Assistant record.

This allows historical rates to be preserved.

Current fields:

```
id

personal_assistant_id

effective_date

base_hourly_rate

employer_top_up_rate

created_at
```

Date storage format:

```
YYYY-MM-DD
```

Example:

```
2026-04-01
```

Display format:

```
DD/MM/YYYY
```

Example:

```
01/04/2026
```

Purpose:

Allows the system to determine:

- What rate applied on a particular date.
- When a rate changed.
- The employer-funded top-up amount.

The payable hourly rate is:

```
base_hourly_rate + employer_top_up_rate
```

---

# 6. Timesheet Entry

A timesheet entry represents work completed by a Personal Assistant.

Current fields:

```
id

pa_name

start_time
end_time

break_minutes

worked_minutes

hourly_rate

amount

notes
```

Current implementation stores:

```
pa_name
```

Future improvement:

Replace this with:

```
personal_assistant_id
```

The transition will be gradual because imported Hours Keeper CSV files currently contain names rather than database IDs.

The system should preserve historical imported information during this change.

---

# 7. Import Audit

Import audit records track CSV imports.

Fields:

```
id

import_time

original_filename

archive_filename

rows_processed

rows_imported

rows_skipped

status

error_message
```

Purpose:

Provides traceability for imported Hours Keeper records.

The system records:

- What file was imported.
- When it was imported.
- How many records were processed.
- Whether the import succeeded.

---

# 8. Future User Accounts

Authentication should be separate from employment records.

The system should eventually support optional user accounts.

Possible roles:

```
Employer / Administrator

Personal Assistant

Payroll User
```

User accounts should store:

```
username

password_hash

role

linked_person_id

active_status
```

A Personal Assistant may exist without having login access.

---

# 9. Future Leave Records

Future versions may include:

## Annual Leave

Stores:

- Personal Assistant.
- Start date.
- End date.
- Hours taken.

Annual leave records are separate from worked hours.

---

## Public Holidays

The system should support:

- Public holiday dates.
- Automatic detection during imports.
- Correct allocation of hours.

---

# 10. Future Payroll Periods

Payroll periods will represent the payment cycle.

They may contain:

- Period start date.
- Period end date.
- Submission date.
- Payroll payment date.

Special periods may be required for Christmas payroll arrangements.

---

# 11. Data Protection

The application handles sensitive personal information.

The system should:

- Store only required information.
- Keep data local where possible.
- Avoid unnecessary personal information in filenames.
- Maintain audit records.
- Protect access through future user authentication.

---

# 12. Payroll Adjustments (Future)

Some payroll items are not part of the hours worked but still appear on payroll and timesheets.

Examples include:

- Statutory Sick Pay (SSP)
- Sick hours
- Mileage claims

These should not be stored directly within the Timesheet Entry.

Instead, future versions of DirectPaymentTimesheets should store them as payroll adjustments linked to an individual payroll period.

Employer Settings

Each employer should define default capabilities:

- Sick Pay / SSP Enabled
- Mileage Claims Enabled

These defaults represent the normal policy for the employer.

Personal Assistant Settings

Each Personal Assistant should also store:

- Sick Pay / SSP Enabled
- Mileage Claims Enabled

When a new Personal Assistant is created, these values should default from the Employer settings but may be changed for that individual if required.

Timesheet Generation

When generating a payroll timesheet, the application should:

1. Load the Personal Assistant record.
2. Check whether Sick Pay / SSP is enabled.
3. Check whether Mileage Claims are enabled.
4. If disabled, leave the corresponding sections blank on the generated PDF.
5. If enabled, allow payroll adjustment values to be entered before PDF generation.
6. Store those values so regenerated PDFs remain consistent.

Future Entity

A future PayrollAdjustment entity may contain fields such as:

- payroll_period_id
- personal_assistant_id
- sick_hours
- ssp_amount
- mileage_miles
- mileage_rate
- mileage_amount
- notes

This approach keeps worked hours separate from payroll adjustments while allowing future expansion for additional payroll items.

# 13. Development Principle

The data model should evolve carefully.

Changes should:

- Preserve existing data.
- Use database migrations.
- Maintain historical accuracy.
- Be documented before major implementation changes.

The database should represent the real-world Direct Payment employment process.
