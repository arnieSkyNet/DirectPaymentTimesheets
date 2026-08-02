# DirectPaymentTimesheets

**Project State**

---

## Project Overview

DirectPaymentTimesheets is an open-source application for managing UK Direct Payment administration.

The long-term goal is to provide a complete workflow from recording Personal Assistant (PA) hours through to generating payroll timesheets, PDFs and payroll emails, reducing administration for Direct Payment holders.

The project is designed to be cross-platform and written in Rust.

---

# Current Version

Pre-release

Development Version: 0.0.x

Status: Active Development

---

# Project Objectives

The application will eventually provide:

- Recording of PA working hours.
- Import of Hours Keeper CSV files.
- Payroll calculations.
- Timesheet generation.
- PDF generation.
- Payroll email generation.
- Annual leave management.
- Sick leave management.
- Public holiday calculations.
- Mileage recording.
- Audit trail.
- Reporting.
- Future self-service for Personal Assistants.
- Future client approval workflow.
- Cross-platform desktop application.
- Future browser/mobile interface.

---

# Current Architecture

Repository

```
GitHub Repository
        
        ¼
Rust Source Code
        
        ¼
Application
        
        ¼
User Data
```

Application data lives outside the repository.

Application data directory:

```
~/.directpaymenttimesheets/
```

Current structure:

```
~/.directpaymenttimesheets/

archive/
backups/
cache/
config.toml
database.sqlite
import/
logs/
templates/
```

---

# Current Components

Implemented:

- Application configuration
- Environment handling
- Application context
- SQLite database
- Repository layer
- CSV import
- CSV validation
- Duplicate detection
- CSV archive
- Import audit trail
- Failed import logging

---

# Database

Current tables

## timesheets

Stores imported working records.

Current fields:

- id
- pa_name
- start_time
- end_time
- break_minutes
- worked_minutes
- hourly_rate
- amount
- notes

---

## import_audit

Stores import history.

Current fields:

- id
- import_time
- original_filename
- archive_filename
- rows_processed
- rows_imported
- rows_skipped
- status
- error_message

---

# Import Pipeline

Current workflow

```
Configured Import Folder
        
        ¼
Discover CSV Files
        
        ¼
Check Import History
        
         Already Imported
               
               ¼
             Skip
        
        ¼
Validate CSV
        
        ¼
Insert Records
        
        ¼
Archive CSV
        
        ¼
Write Audit Record
```

---

# Archive Strategy

Imported CSV files are archived.

Archive layout:

```
archive/

2026/
    08/

        2026-08-02_150024_Cedar Fixture_20260702-20260801.csv
```

Archive files are immutable.

No archive file should ever be modified after creation.

---

# Design Principles

The project follows several important principles.

- Cross-platform.
- Rust-first.
- SQLite database.
- Human-readable configuration.
- No hard-coded paths.
- Small incremental development.
- Git used as permanent project memory.
- Every significant feature committed.
- Every import auditable.
- Preserve historical data.

---

# Current Workflow

The software currently performs:

Configuration



CSV Import



Validation



Duplicate Detection



Database Storage



Archive



Audit Trail

---

# Next Milestone

Implement the Payroll Engine.

The Payroll Engine will transform imported work records into payroll periods suitable for timesheet generation.

Planned features include:

- Weekly grouping.
- Contracted hours.
- Estimated hours.
- Carry-forward hours.
- Public holidays.
- Annual leave.
- Sick leave.
- Future overtime support.
- Future top-up pay support.

---

# Long-Term Vision

DirectPaymentTimesheets will eventually replace the current manual workflow consisting of:

Hours Keeper



Google Sheets



Google Docs



Manual PDF Export



Manual Email

with:

DirectPaymentTimesheets



Payroll Engine



PDF Generation



Email Generation



Completed Payroll Submission

---

# Current Project Status

Foundation Complete.

Architecture Stable.

Ready to begin Payroll Engine development.


