# Direct Payment Timesheets Processor Architecture

## Purpose

This document describes the technical architecture and design decisions of Direct Payment Timesheets Processor.

The application is designed as a cross-platform Rust application for managing UK Direct Payment administration, Personal Assistant records, worked hours, payroll preparation and related documentation.

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
- Configurable.
- Designed to preserve historical information.

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

---

# Configuration

The application must not contain hard-coded user paths.

Configuration controls locations and settings such as:

- CSV import folders.
- Archive folders.
- Generated document folders.
- Email settings.
- Application preferences.
- Payroll settings.

The importer does not depend on a fixed location.

Users can configure their own folders.

---

# Application Structure

The application is separated into layers.

Current structure:

```
main.rs

    |

    v

Application Layer

    |

    +-- Application context
    +-- Configuration loading
    +-- Environment handling
    +-- Database initialisation
    +-- Repository creation
    +-- Service startup


    |

    v

Service Layer

    |

    +-- Import Service


    |

    v

Repository Layer

    |

    +-- Database operations
    +-- Timesheet storage
    +-- Duplicate checking
    +-- Import audit storage
```

---

# Domain Layer

The application separates real-world business concepts from technical implementation.

The domain layer represents:

- Employer records.
- Personal Assistant records.
- Employment history.
- Contracted hours history.
- Pay rate history.
- Worked shifts.
- Annual leave records.
- Public holiday calendar.
- Payroll periods.
- Payroll preparation.

Technical services operate on these domain concepts.

---

# Application Context

The application uses an application context layer.

The purpose is to provide:

- Configuration.
- Paths.
- Environment information.
- Application settings.
- Shared application resources.

Individual modules should request resources from the application context rather than creating their own paths.

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

Import Worked Shift Records

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

The application is moving towards separate services.

Current service:

```
Import Service

    |

    +-- Discover files
    +-- Validate data
    +-- Import records
    +-- Archive files
    +-- Create audit records
```

Future services:

```
Payroll Service

    |

    +-- Group work into payroll periods
    +-- Apply pay rate history
    +-- Apply employer top-ups
    +-- Include annual leave
    +-- Identify public holiday hours
    +-- Generate payroll records


Document Service

    |

    +-- Generate PDF timesheets
    +-- Manage templates
    +-- Prepare payroll documents


Notification Service

    |

    +-- Prepare payroll emails
    +-- Record submission information
```

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


Repository

    - Database inserts
    - Duplicate detection
    - Import audit checking
```

Tests use isolated in-memory databases where appropriate.

---

# Data Protection

The application should minimise exposure of sensitive information.

Sensitive identifiers should not be placed unnecessarily into filenames.

Preferred:

```
timesheet_2026-08-01.csv
```

Avoid:

```
employee_name_NI_NUMBER.csv
```

Personal information should only be stored where required for the Direct Payment payroll process.

---

# Repository Separation

The Git repository contains:

- Source code.
- Documentation.
- Tests.

User data remains outside the repository.

This prevents accidental commits of private payroll information.

---

# Future Architecture Direction

The long-term architecture will support:

```
Direct Payment Timesheets Processor

        |

        v

Domain Management

        |

        +-- Employer
        +-- Personal Assistants
        +-- Employment Records
        +-- Pay Rates
        +-- Leave Records


        |

        v

Payroll Engine

        |

        v

Document Generation

        |

        v

Payroll Communication
```

The architecture should continue to evolve while maintaining:

- Clear separation of responsibilities.
- Historical accuracy.
- Auditability.
- Privacy.
- Simple maintenance.

