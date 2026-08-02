# DirectPaymentTimesheets Architecture

## Purpose

This document describes the technical architecture and design decisions of DirectPaymentTimesheets.

The application is designed as a cross-platform Rust application for managing UK Direct Payment timesheets and payroll administration.

---

# Design Goals

The application should be:

- Reliable.
- Easy to maintain.
- Cross-platform.
- Suitable for individual Direct Payment holders.
- Suitable for future organisational use.
- Auditable.
- Privacy conscious.

---

# Technology Choices

## Rust

Rust was selected because it provides:

- Memory safety.
- Good performance.
- Cross-platform support.
- Long-term maintainability.

---

## SQLite

SQLite is used as the local database because:

- It requires no server.
- It is portable.
- It is suitable for desktop applications.
- It allows complete backups by copying the database file.

---

# Application Data

The application stores user data outside the source repository.

Default location:

    ~/.directpaymenttimesheets/

Example:

    ~/.directpaymenttimesheets/

    config.toml
    database.sqlite

    archive/
    backups/
    cache/
    import/
    logs/
    templates/

---

# Configuration

The application must not contain hard-coded user paths.

Configuration controls locations such as:

- CSV import folders.
- Archive folders.
- Future generated documents.
- Future email settings.

The importer does not depend on a fixed location. Users can configure their own import folder.

---

# Application Structure

The application is separated into layers.

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

    ImportService
        |
        +-- Discover CSV files
        +-- Validate CSV data
        +-- Import records
        +-- Archive files
        +-- Create audit records

    Repository
        |
        +-- Timesheet storage
        +-- Duplicate checking
        +-- Import audit storage

---

# Application Context

The application uses an application context layer.

The purpose is to provide:

- Configuration.
- Paths.
- Environment information.
- Application settings.

Individual modules should request resources from the application context rather than creating their own paths.

---

# Import Pipeline

The current import workflow is:

    Configured Import Folder

            |

            v

    Find CSV Files

            |

            v

    Validate CSV Structure

            |

            v

    Import Timesheet Records

            |

            v

    Check For Duplicates

            |

            v

    Store In SQLite Database

            |

            v

    Archive Original CSV

            |

            v

    Write Import Audit Record

Each import records:

- Source filename.
- Archive filename.
- Rows processed.
- Rows imported.
- Rows skipped.
- Success or failure status.
- Error information if required.

---

# Service Architecture

The application is moving towards service-based architecture.

Current service:

    Import Service
        |
        +-- Discover files
        +-- Validate data
        +-- Import records
        +-- Archive files
        +-- Create audit records

Future services:

    Payroll Service
        |
        +-- Calculate hours
        +-- Apply rates
        +-- Handle leave
        +-- Generate payroll records

---

# Testing Strategy

The project uses automated Rust tests.

Current coverage includes:

    CSV Import
        - Duration parsing
        - Money parsing
        - Invalid input handling

    Archive
        - Timestamped archive filenames

    Repository
        - Database inserts
        - Duplicate detection
        - Import audit checking

Tests use isolated in-memory databases where appropriate.

---

# Data Protection

The application should minimise exposure of sensitive information.

Sensitive identifiers should not be placed unnecessarily into filenames.

Preferred:

    timesheet_2026-08-01.csv

Avoid:

    employee_name_NI_NUMBER.csv

---

# Repository Separation

The Git repository contains:

- Source code.
- Documentation.
- Tests.

User data remains outside the repository.

This prevents accidental commits of private payroll information.

