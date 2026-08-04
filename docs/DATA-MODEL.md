# Direct Payment Timesheets Processor Data Model

## Document Purpose

This document defines the information structure required by the Direct Payment Timesheets Processor application.

The purpose is to describe the data the application needs to store before implementing the complete database layer.

The application is designed to support the real-world Direct Payment payroll preparation process, including:

- Personal Assistant records.
- Employment information.
- Contracted hours history.
- Pay rate history.
- Hours Keeper CSV imports.
- Payroll adjustments.
- Payroll calendar management.
- Payroll preparation PDF generation.

The final output is a payroll preparation document compatible with the Payroll department's requirements.

---

# 1. Core Concept

The application does not treat a timesheet as the only primary record.

A timesheet is one part of a wider employment and payroll process.

The relationship is:

```
Employer

    |
    |
Personal Assistant

    |
    +-- Employment history
    |
    +-- Contracted hours history
    |
    +-- Pay rate history
    |
    +-- Imported worked shifts
    |
    +-- Annual leave records
    |
    +-- Payroll adjustments

    |
    |
Payroll preparation PDF
```

---

# 2. Employer

The system contains one employer record.

## Employer information

Fields:

- Employer name.
- Address.
- Postcode.
- Telephone.
- Email.

## Payroll provider information

Fields:

- Payroll provider name.
- Payroll provider address.
- Payroll provider telephone number.

## Document settings

Fields:

- Employer signature.
- Default PDF template.

Future additions may include:

- PDF naming rules.
- Document storage preferences.
- Email settings.

---

# 3. Personal Assistant

Each Personal Assistant has a personal record.

## Personal details

Fields:

- First name.
- Surname.
- Date of birth.
- National Insurance number.
- Address.
- Postcode.
- Telephone.
- Email.

The National Insurance number may be displayed on payroll documents after the employee name to help identify employees with the same name.

Example:

```
Employee's name: Cathy (NI: AB123456C)
```

---

# 4. Employment History

Employment information must be historical because circumstances can change.

Examples:

- Change of contracted hours.
- Change from fixed hours to variable hours.
- New employment arrangements.

Fields:

- Personal Assistant.
- Effective date.
- Employment type.
- Contracted weekly hours.
- Change reason.

Employment types:

- Fixed hours.
- Variable hours.

Examples:

```
01/04/2026
Fixed hours
18 hours
```

```
01/04/2026
Variable hours
Various
```

---

# 5. Pay Rate History

Pay rates must be historical because rates can change.

Fields:

- Personal Assistant.
- Effective date.
- Government/minimum rate.
- Employer top-up.
- Total hourly rate.
- Reason for change.

Example:

```
Effective date:
01/04/2026

Government rate:
£12.21

Employer top-up:
£0.51

Total paid:
£12.72
```

The PDF displays the total applied hourly rate.

---

# 6. Worked Shift Records

Worked shifts represent actual hours worked.

The initial source is the Hours Keeper CSV export.

A worked shift contains:

- Personal Assistant.
- Date.
- Start time.
- End time.
- Break duration.
- Worked minutes.
- Notes.
- Import source.

Hours Keeper records actual worked time only.

It does not contain:

- Annual leave.
- Public holiday classification.
- Sick leave.
- Travel claims.

---

# 7. Public Holiday Calendar

Public holidays are stored separately.

Fields:

- Date.
- Name.
- Country.

During CSV import:

The system checks the worked shift date against the public holiday calendar.

If worked hours occur on a public holiday, they are automatically classified.

Example:

Imported shifts:

```
25/12/2026

1.25 hours
2 hours
2.25 hours
```

System output:

```
Public Hols. Hours worked:

5.5 (25/12/2026)
```

The system records the hours only.

Payroll decides the payment treatment.

---

# 8. Annual Leave Records

Annual leave is entered manually.

Annual leave does not come from Hours Keeper.

A user action:

```
Enter Annual Leave
```

opens an entry form.

Fields:

- Personal Assistant.
- Date commencing.
- Date ending.
- Hours.

The dates are stored as a record of what happened.

They are used to allocate annual leave hours into the correct payroll period.

---

# 9. Optional Payroll Adjustments

The Payroll PDF contains additional columns that may not always be used.

Supported adjustments:

## Sick Leave / SSP

Status:

Optional.

The column exists because it is part of the Payroll department template.

No automatic SSP calculation is required initially.

## Travel

Fields:

- Miles claimed.
- Rate per mile.

Example:

```
Miles claimed @ £0.40/mile
```

---

# 10. Payroll Calendar

The system stores payroll periods.

Fields:

- Payroll year.
- Payroll period number.
- Week commencing date.
- Timesheet submission deadline.
- PA pay date.
- Notes.

This supports:

- Four-week payroll cycles.
- Christmas early submission arrangements.
- Future payroll changes.

---

# 11. Payroll Notices

Temporary payroll instructions are stored separately.

Examples:

- Payroll office closures.
- Christmas submission deadlines.
- Special arrangements.

These should not be hard-coded into the application.

---

# 12. Payroll Preparation PDF Layout

The PDF follows the Payroll department's existing format.

## Header

Example:

```
Employer's name: Mark

Employee's name: Cathy (NI: AB123456C)

Contracted weekly hours of work: 18 hours
```

or:

```
Employer's name: Mark

Employee's name: Andy Pandy (NI: AB123456C)

Contracted weekly hours of work: Various
```

---

## Main table columns

Column 1:

```
W/C date
```

Column 2:

```
Hours worked
Pay Rate
```

Column 3:

```
Annual leave hours
```

Column 4:

```
Sick leave
SSP
```

Column 5:

```
Public Hols. Hours worked
```

Column 6:

```
Travel

Miles claimed @ £0.40/mile
```

---

# 13. Design Principles

The system should:

- Preserve historical information.
- Avoid changing old payroll records when settings change.
- Keep imported Hours Keeper data separate from payroll adjustments.
- Support the current Payroll department workflow.
- Allow future replacement of Hours Keeper functionality.
- Avoid hard-coded payroll rules where configuration is more appropriate.

The database structure will be based on this model.

