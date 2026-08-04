# DirectPaymentTimesheets Processor

## Project State

---

# Project Overview

DirectPaymentTimesheets Processor is an open-source application for managing UK Direct Payment administration.

The long-term goal is to provide a complete workflow from recording Personal Assistant (PA) hours through to preparing payroll documentation, generating PDFs and assisting with payroll communication.

The application is designed to reduce the manual administration currently required by Direct Payment holders while maintaining accurate historical records and audit information.

The project is:

- Written in Rust.
- Cross-platform.
- Based around a local SQLite database.
- Designed to keep user data local where possible.

---

# Current Version

Pre-release

Development Version:

```
0.0.x
```

Status:

Active Development

---

# Project Objectives

The application will eventually provide:

- Personal Assistant record management.
- Employer information management.
- Hours Keeper CSV import.
- Worked shift recording.
- Payroll period management.
- Contracted hours history.
- Pay rate history.
- Employer top-up rate support.
- Annual leave management.
- Public holiday detection.
- Payroll preparation sheet generation.
- PDF generation.
- Payroll email preparation.
- Import audit trail.
- Reporting.
- Future Personal Assistant self-service.
- Future approval workflow.
- Cross-platform desktop application.
- Future browser/mobile interface.

Optional future areas:

- Sick leave / SSP handling.
- Mileage claims.
- Additional payroll adjustments.

---

# Current Development Stage

The project has completed the initial application foundation.

The current focus is moving from the import prototype into the wider payroll domain model.

The agreed direction is:

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
    +-- Worked shifts
    |
    +-- Annual leave
    |
    +-- Public holidays
    |
    +-- Payroll preparation
```

---

# Current Architecture

The application is structured into separate layers.

Current structure:

```
main.rs

    |

    v

Application

    |

    +-- Application context
    +-- Environment handling
    +-- Configuration loading
    +-- Database initialisation
    +-- Repository creation

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

    +-- Database operations
    +-- Duplicate checking
    +-- Import audit storage
```

Future layers will include:

```
Payroll Domain

    |

    +-- Employer management
    +-- PA management
    +-- Pay rate management
    +-- Leave management
    +-- Payroll preparation
    +-- PDF generation
```

---

# Application Data Location

Application data is stored outside the repository.

Default location:

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

# Implemented Components

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
- Import summary display.
- Timesheet viewing dashboard.

---

# Current Database Implementation

The current database contains:

## timesheets

Stores imported worked records.

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

# Future Database Expansion

The database model will expand to include:

- Employer records.
- Personal Assistant records.
- Employment history.
- Contracted hours history.
- Pay rate history.
- Payroll periods.
- Public holiday calendar.
- Annual leave records.
- Payroll adjustments.

---

# Import Pipeline

Current workflow:

```
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
```

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

Example:

```
archive/

2026/

08/

2026-08-02_150024_timesheet.csv
```

Archive files are treated as historical records and should not be modified after creation.

---

# Documentation Status

Current documentation includes:

- Architecture documentation.
- Domain guide.
- Data model.
- Development information.
- Project constitution.

The documentation has been updated to reflect:

- Payroll workflow requirements.
- Personal Assistant records.
- Rate history.
- Contract history.
- Annual leave handling.
- Public holiday detection.
- Payroll document requirements.

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

The project follows these principles:

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
- Separate imported data from payroll adjustments.
- Keep business logic separated from technical implementation.

---

# Current Workflow

The software currently performs:

```
Configuration

        |

CSV Import

        |

Validation

        |

Duplicate Detection

        |

Database Storage

        |

Archive

        |

Audit Trail
```

---

# Next Development Milestone

The next milestone is implementing the payroll domain foundation.

Planned areas:

- Create Employer model.
- Create Personal Assistant model.
- Introduce payroll configuration.
- Create database schema expansion.
- Add employment history.
- Add contracted hours history.
- Add pay rate history.
- Prepare payroll document generation model.

---

# Long-Term Vision

DirectPaymentTimesheets Processor will eventually replace the current manual workflow:

```
Hours Keeper

        |

Google Sheets

        |

Google Docs

        |

Manual PDF Export

        |

Manual Email
```

with:

```
DirectPaymentTimesheets Processor

        |

Payroll Engine

        |

PDF Generation

        |

Payroll Communication

        |

Completed Payroll Submission
```

---

# Current Project Status

Foundation complete.

Import pipeline operational.

Core application structure established.

Domain model documented.

Data model documented.

Ready to begin payroll domain implementation.

