# DirectPaymentTimesheets Architecture

## Purpose

This document describes the technical architecture and design decisions of DirectPaymentTimesheets.

The application is designed as a cross-platform Rust application for managing UK Direct Payment timesheets, employment records and future payroll administration.

The architecture is designed to support a gradual move from simple timesheet recording into a complete payroll preparation system.

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
- Expandable without major redesign.

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
- It keeps user data local.

---

# Application Data

The application stores user data outside the source repository.

Default location:

```
~/.directpaymenttimesheets/
```

Example:

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

The source repository contains:

- Application code.
- Documentation.
- Tests.

User information remains outside Git.

---

# Configuration

The application must not contain hard-coded user paths.

Configuration controls locations such as:

- CSV import folders.
- Archive locations.
- Generated documents.
- Templates.
- Future email settings.

Application modules should obtain paths through the application context rather than creating their own locations.

---

# Application Structure

The application is separated into layers.

Current structure:

```
main.rs

    |

    v

application.rs

    |

    +-- Application startup
    +-- Environment initialisation
    +-- Database setup
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

    +-- Timesheet Repository
    +-- Import Audit Repository
    +-- Employer Repository
    +-- Personal Assistant Repository
    +-- Pay Rate Repository
```

---

# Application Context

The application uses an application context layer.

The purpose is to provide:

- Configuration.
- Paths.
- Environment information.
- Shared application resources.

Individual modules should request resources from the application context rather than managing their own configuration.

---

# Database Architecture

The database uses SQLite with schema version tracking.

Database creation is handled through:

```
database.rs
```

Responsibilities:

- Create required tables.
- Check schema version.
- Apply migrations.
- Maintain database upgrades.

Current migration approach:

```
schema_version

        |

        v

migration functions

        |

        v

updated database structure
```

This allows future changes without destroying existing data.

---

# Current Database Areas

## Timesheets

Stores imported working records.

Responsibilities:

- Store PA hours.
- Preserve historical pay information.
- Support payroll calculations.

---

## Employer Records

Stores Direct Payment employer information.

Responsibilities:

- Employer details.
- Payroll provider information.
- Document generation details.

---

## Personal Assistant Records

Stores PA employment information.

Responsibilities:

- Employee details.
- Employment status.
- Future payroll relationships.

---

## Pay Rate History

Stores PA pay rate changes.

Responsibilities:

- Preserve historical rates.
- Apply correct rate based on work date.
- Support future payroll calculations.

---

## Import Audit

Stores import history.

Responsibilities:

- Record imported files.
- Track processing results.
- Provide audit trail.

---

# Repository Architecture

Database access is separated into repository modules.

Current repositories:

```
TimesheetRepository

EmployerRepository

PersonalAssistantRepository

PayRateRepository
```

Repositories are responsible for:

- Database queries.
- Inserts.
- Retrieval.
- Data persistence.

Business rules should remain outside repositories.

---

# Import Pipeline

The current import workflow is:

```
Configured Import Folder

        |

        v

Find CSV Files

        |

        v

Validate CSV Structure

        |

        v

Check Duplicate Imports

        |

        v

Import Timesheet Records

        |

        v

Store In SQLite Database

        |

        v

Archive Original CSV

        |

        v

Write Import Audit Record
```

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

```
Import Service
```

Responsibilities:

- Discover files.
- Validate data.
- Import records.
- Archive files.
- Create audit records.

Future services:

```
Payroll Service

Leave Service

Document Service

Email Service

Authentication Service
```

---

# Authentication Design

Authentication should remain separate from employment records.

Future user accounts should support:

- Employer access.
- Personal Assistant access.
- Payroll access.

A Personal Assistant does not automatically require a login account.

Employment records and login permissions are separate concepts.

---

# Leave and Public Holiday Design

Future services will handle:

## Annual Leave

Responsibilities:

- Record leave periods.
- Record leave hours.
- Allocate leave into payroll periods.

---

## Public Holidays

Responsibilities:

- Maintain public holiday dates.
- Detect public holiday work during imports.
- Support payroll reporting.

---

# Payroll Architecture

The future Payroll Engine will transform validated records into payroll outputs.

Responsibilities:

- Group timesheets into payroll periods.
- Apply historical pay rates.
- Include leave records.
- Include public holiday information.
- Generate payroll documents.

The Payroll Engine should not operate directly on raw imported files.

---

# Testing Strategy

The project uses automated Rust tests.

Current coverage includes:

```
CSV Import

    - Duration parsing
    - Money parsing
    - Invalid input handling


Archive

    - Timestamped archive filenames


Repositories

    - Database inserts
    - Record retrieval
    - Duplicate detection
    - Import audit checking
    - Employer records
    - Personal Assistant records
    - Pay rate history
```

Tests use isolated databases where appropriate.

---

# Data Protection

The application should minimise exposure of sensitive information.

The system should:

- Store only necessary information.
- Keep data local where possible.
- Avoid personal information in filenames.
- Maintain audit history.
- Separate user access from employment data.

---

# Repository Separation

The Git repository contains:

- Source code.
- Documentation.
- Tests.

User data remains outside the repository.

This prevents accidental commits of private payroll information.

---

# Development Principle

The architecture should evolve incrementally.

Changes should:

- Preserve existing data.
- Use migrations for database changes.
- Keep business logic separated from technical code.
- Maintain clear documentation.
- Be tested before major changes are committed.

