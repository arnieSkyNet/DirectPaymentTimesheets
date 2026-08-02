# DirectPaymentTimesheets

Project State

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

The application is structured into separate layers.

Current structure:

    main.rs
        |
        v

    application.rs
        |
        +-- Application context
        +-- Database initialisation
        +-- Repository creation
        +-- Import service startup

        |
        v

    ImportService

        |
        +-- Discover CSV files
        +-- Validate CSV data
        +-- Import records
        +-- Archive files
        +-- Create audit records

        |
        v

    Repository

        |
        +-- Timesheet storage
        +-- Duplicate checking
        +-- Import audit storage

Application data lives outside the repository.

Application data directory:

    ~/.directpaymenttimesheets/

Current structure:

    ~/.directpaymenttimesheets/

    archive/
    backups/
    cache/
    config.toml
    database.sqlite
    import/
    logs/
    templates/

---

# Current Components

Implemented:

- Application configuration.
- Environment handling.
- Application context.
- Application startup layer.
- SQLite database.
- Repository layer.
- CSV import.
- CSV validation.
- Money validation.
- Duration validation.
- Duplicate detection.
- CSV archive.
- Timestamped archive naming.
- Import audit trail.
- Failed import logging.

---

# Database

Current tables:

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

Current workflow:

    Configured Import Folder

            |

            v

    Discover CSV Files

            |

            v

    Check Previous Successful Imports

            |

            v

    Validate CSV

            |

            v

    Insert Records

            |

            v

    Archive CSV

            |

            v

    Write Audit Record

The import system records:

- Files processed.
- Rows processed.
- Rows imported.
- Rows skipped.
- Failed imports.
- Archive locations.

---

# Archive Strategy

Imported CSV files are archived.

Archive files use timestamped filenames to prevent collisions.

Archive layout example:

    archive/

    2026/

        08/

            2026-08-02_150024_timesheet.csv

Archive files are treated as historical records and should not be modified after creation.

---

# Testing

The project includes automated Rust tests.

Current coverage includes:

- CSV duration parsing.
- CSV money parsing.
- Invalid CSV values.
- Archive filename handling.
- Repository database operations.
- Duplicate detection.
- Import audit checking.

Tests use isolated databases where appropriate.

---

# Design Principles

The project follows several important principles:

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
- Keep application logic separated from startup code.

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

# Next Development Milestone

Continue strengthening the application foundation before implementing the Payroll Engine.

Planned next areas:

- Improve application error handling.
- Expand automated testing.
- Improve service separation.
- Prepare domain models for payroll calculations.

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

Application structure established.

Import pipeline operational.

Documentation aligned with current implementation.

Ready for continued foundation improvements before Payroll Engine development.
