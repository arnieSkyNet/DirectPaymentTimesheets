# DirectPaymentTimesheets

# Project State

---

## Project Overview

DirectPaymentTimesheets is an open-source application for managing UK Direct Payment administration.

The long-term goal is to provide a complete workflow from recording Personal Assistant (PA) hours through to generating payroll timesheets, PDFs and payroll emails.

The application is designed to reduce the manual administration currently required using separate tools such as Hours Keeper, spreadsheets and document templates.

The project is designed to be cross-platform and written in Rust.

---

# Current Version

Pre-release

Development Version:

```
0.0.x
```

Status:

```
Active Development
```

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
- Public holiday calculations.
- Mileage recording.
- Audit trail.
- Reporting.
- Future Personal Assistant self-service.
- Future client approval workflow.
- Cross-platform desktop application.
- Future browser/mobile interface.

---

# Current Architecture

The application is structured into separate layers.

Current structure:

```
main.rs

    |

    v

application.rs

    |

    +-- Application context
    +-- Environment handling
    +-- Database initialisation
    +-- Repository creation
    +-- Service startup

    |

    v

Services

    |

    +-- Import Service

    |

    v

Repositories

    |

    +-- Timesheet repository
    +-- Import audit repository
    +-- Employer repository
    +-- Personal Assistant repository
    +-- Pay rate repository
```

---

# Application Data

Application data is stored outside the Git repository.

Default application directory:

```
~/.directpaymenttimesheets/
```

Current structure:

```
~/.directpaymenttimesheets/

config.toml

database.sqlite

archive/

backups/

cache/

import/

logs/

templates/
```

User data should never be stored inside the source repository.

---

# Database

The database uses SQLite.

Database creation uses schema versioning and migrations.

Current database tables:

---

## schema_version

Purpose:

Tracks database structure versions.

Used for:

- Applying future migrations.
- Maintaining database upgrades.

---

## timesheets

Stores imported working records.

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

Future improvement:

Replace:

```
pa_name
```

with:

```
personal_assistant_id
```

while preserving historical imported data.

---

## import_audit

Stores import history.

Current fields:

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

The import audit provides traceability for every imported CSV file.

---

## employers

Stores Direct Payment employer information.

Current fields include:

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

---

## personal_assistants

Stores Personal Assistant employment information.

Current fields include:

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

---

## personal_assistant_pay_rates

Stores historical PA pay rates.

Purpose:

- Preserve old rates.
- Support future payroll calculations.
- Allow rate changes over time.

---

# Current Components

Implemented:

- Application configuration.
- Environment handling.
- Application context.
- SQLite database.
- Database schema versioning.
- Database migrations.
- Repository layer.
- Timesheet repository.
- Employer repository.
- Personal Assistant repository.
- Pay rate repository.
- CSV import.
- CSV validation.
- Money validation.
- Duration validation.
- Duplicate detection.
- CSV archive.
- Import audit trail.
- Failed import logging.

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

Archive Original CSV

        |

        v

Write Import Audit Record
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

Archive files are preserved as historical records.

Example:

```
archive/

    2026/

        08/

            2026-08-02_150024_timesheet.csv
```

Archive files should not be modified after creation.

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
- Employer repository operations.
- Personal Assistant repository operations.
- Pay rate repository operations.

Tests use isolated databases where appropriate.

---

# Current Development Status

The application foundation is established.

Completed foundation work:

- Application startup structure.
- Configuration layer.
- Environment handling.
- Database abstraction.
- Schema migration system.
- Core repositories.
- Import pipeline.

The project is ready for further business feature development.

---

# Next Development Areas

Planned future work:

## Leave Management

Including:

- Annual leave records.
- Public holiday dates.
- Payroll allocation rules.

---

## Payroll Engine

Including:

- Payroll period calculations.
- Historical pay rates.
- Leave handling.
- Public holiday handling.
- Payroll outputs.

---

## Document Generation

Including:

- Four-week payroll timesheet PDFs.
- Email preparation.
- Payroll submission workflow.

---

## User Access

Future support for:

- Employer login.
- Personal Assistant login.
- Optional self-service hours submission.

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
- Separate business logic from technical implementation.

---

# Long-Term Vision

DirectPaymentTimesheets will eventually replace the current manual workflow:

```
Hours Keeper

        |

        v

Google Sheets

        |

        v

Google Docs

        |

        v

Manual PDF Export

        |

        v

Manual Email
```

with:

```
DirectPaymentTimesheets

        |

        v

Payroll Engine

        |

        v

PDF Generation

        |

        v

Email Generation

        |

        v

Completed Payroll Submission
```

---

# Current Project Status

Foundation complete.

Database structure established.

Import pipeline operational.

Core employment records implemented.

Documentation aligned with current implementation.

Ready for continued business feature development.

