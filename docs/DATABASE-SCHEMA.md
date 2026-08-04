# Direct Payment Timesheets Processor Database Schema

## Purpose

This document describes the planned database structure for Direct Payment Timesheets Processor.

The purpose is to define the information the application needs to store before implementing the database layer.

The database must support:

- Personal Assistant records.
- Employer information.
- Worked timesheets.
- Pay rate history.
- Contracted hours history.
- Annual leave records.
- Public holiday calculations.
- Payroll preparation.
- Import auditing.
- Historical accuracy.

The database is designed to preserve records over time rather than overwrite historical information.

---

# Database Principles

The database should:

- Preserve historical records.
- Avoid storing duplicated information.
- Separate people from their work records.
- Support multiple Personal Assistants.
- Support changing employment details.
- Support future payroll processing.
- Keep audit information.
- Store user data locally.

---

# Database Versioning

The application will maintain a database schema version.

Example:

```
schema_version

version
```

When the application starts:

- The current database version is checked.
- Required upgrades are applied.
- The database version is updated.

This allows future database changes without losing existing information.

---

# Core Tables

## 1. Employer

Stores the Direct Payment employer information.

One employer record is expected.

Fields:

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

Used for:

- Timesheet headings.
- Payroll documents.
- Generated PDFs.
- Communication records.

---

# 2. Personal Assistant

Stores Personal Assistant details.

Fields:

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

Provides the main identity record for each PA.

The National Insurance number is stored separately to avoid confusion when two PAs have similar names.

---

# 3. Employment History

Stores changes to employment arrangements.

Fields:

```
id

pa_id

start_date
end_date

employment_status

notes
```

Purpose:

Allows historical records of employment changes.

---

# 4. Contracted Hours History

Stores agreed weekly contracted hours.

Fields:

```
id

pa_id

effective_date

contracted_hours

hours_type

notes
```

Examples:

```
18 hours
```

or:

```
Variable hours
```

Purpose:

Supports annual leave calculations and employment records.

Contracted hours changes should create a new historical record.

---

# 5. Pay Rate History

Stores hourly rate changes.

Fields:

```
id

pa_id

effective_date

government_rate

employer_top_up

total_hourly_rate

notes
```

Purpose:

Supports:

- Minimum wage changes.
- Employer top-up payments.
- Historical payroll accuracy.

A rate change should not overwrite previous rates.

---

# 6. Timesheets

Stores worked shifts imported from CSV or entered manually.

Fields:

```
id

pa_id

work_date

start_time
end_time

break_minutes

worked_minutes

pay_rate_id

amount

notes
```

Purpose:

Represents individual periods of work.

Historical pay rate information must remain linked to the rate that applied at that time.

---

# 7. Annual Leave

Stores Personal Assistant holiday records.

Fields:

```
id

pa_id

start_date
end_date

hours

notes
```

Purpose:

Records annual leave taken.

Annual leave is separate from worked hours.

When generating payroll timesheets:

- Annual leave hours are placed into the correct payroll period.
- Annual leave dates remain available as historical records.

---

# 8. Public Holidays

Stores public holiday dates.

Fields:

```
id

date

name

country

notes
```

Purpose:

Allows the system to identify work performed on public holidays.

Imported shifts that occur on public holidays can automatically be classified.

Example:

```
25/12/2026

Christmas Day
```

---

# 9. Payroll Periods

Stores payroll submission periods.

Fields:

```
id

start_date

end_date

pay_date

submission_date

status

notes
```

Purpose:

Supports:

- Four-week payroll cycles.
- Payroll preparation.
- Special submission dates.
- Christmas payroll arrangements.

---

# 10. Import Audit

Stores CSV import history.

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

Provides a complete audit trail of imported files.

---

# Relationships

The planned relationships are:

```
Employer

    |

    +---- Personal Assistant

              |

              +---- Employment History

              |

              +---- Contracted Hours History

              |

              +---- Pay Rate History

              |

              +---- Timesheets

              |

              +---- Annual Leave
```

Public holidays and payroll periods operate independently and are used during payroll processing.

---

# Future Expansion

Possible future tables:

## Approval Records

For:

- PA submission.
- Employer approval.
- Approval dates.

## Mileage Claims

For:

- Travel miles.
- Mileage rate.
- Reimbursement.

## Sick Leave

For:

- Sickness records.
- SSP calculations.

## Payroll Runs

For:

- Generated payroll submissions.
- Submission history.
- Adjustments.

---

# Design Principle

The database should represent the real-world Direct Payment employment process.

Changes to information should create new historical records where required rather than replacing previous values.

Historical accuracy is essential for payroll and audit purposes.

