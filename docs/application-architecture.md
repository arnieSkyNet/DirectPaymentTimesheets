# DirectPaymentTimesheets Application Architecture

## Document Purpose

This document describes the overall application architecture of DirectPaymentTimesheets.

The purpose is to explain how the major parts of the system fit together and how future features can be added without requiring major redesign.

DirectPaymentTimesheets is designed as an open-source replacement for manual UK Direct Payment / Personal Budget administration workflows.

---

# 1. Design Goals

DirectPaymentTimesheets is designed around the following principles:

- Local-first application design.
- Cross-platform compatibility.
- Reliable storage of care and payroll records.
- Clear separation between data storage, business logic and user interface.
- Documentation and code evolving together.
- Small, testable components.
- Preservation of historical records.
- Avoiding hard-coded assumptions about payroll arrangements.

---

# 2. High-Level Architecture

The application consists of several layers:

```text
User Interface

      |

      v

Application Layer

      |

      v

Services

      |

      v

Repositories

      |

      v

Database
```

Supporting layers provide:

```text
Configuration

Environment

Application Context

Error Handling
```

---

# 3. Core Components

## User Interface Layer

Responsible for interaction with the user.

Current:

- Desktop application foundation.

Future capabilities:

- Browser-based interface.
- Mobile-friendly access.
- Timesheet entry.
- Approval screens.
- Reports.
- Self-service access.

---

## Application Layer

Responsible for application startup and coordination.

Responsibilities:

- Initialise environment.
- Load configuration.
- Create application context.
- Start services.
- Connect repositories.

The application layer should coordinate components without containing business rules.

---

## Service Layer

Responsible for business workflows.

Current service:

```
Import Service
```

Responsibilities:

- Discover files.
- Validate CSV data.
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

## Repository Layer

Responsible for database access.

Current repositories:

```
Timesheet Repository

Employer Repository

Personal Assistant Repository

Pay Rate Repository
```

Repositories handle:

- Saving records.
- Retrieving records.
- Database queries.

Business rules should remain outside repositories.

---

## Database Layer

Responsible for permanent storage.

Technology:

```
SQLite
```

Responsibilities:

- Store timesheets.
- Store employer information.
- Store Personal Assistant information.
- Store pay rate history.
- Store import audit records.
- Maintain historical records.
- Support schema migrations.

---

## Import / Export Layer

Responsible for moving information into and out of the application.

Current:

- CSV import.

Future:

- CSV export.
- Payroll exports.
- Backup functions.
- Data migration tools.

---

## Document Generation Layer

Responsible for producing documents.

Future capabilities:

- PDF timesheets.
- Payroll documents.
- Reports.
- Submission packages.

---

## Communication Layer

Responsible for external communication.

Future capabilities:

- Payroll emails.
- Document delivery.
- Notification systems.

---

# 4. Data Flow

The application workflow is:

```text
CSV Import

    |

    v

Validation

    |

    v

SQLite Storage

    |

    v

Timesheet Processing

    |

    v

Approval Workflow

    |

    v

Payroll Calculation

    |

    v

PDF Generation

    |

    v

Email / Export
```

---

# 5. Current Rust Structure

Current source structure:

```text
src/

main.rs
    Application entry point.

application.rs
    Application startup and coordination.

config.rs
    Configuration handling.

environment.rs
    Environment management.

context.rs
    Shared application context.

database.rs
    SQLite setup and migrations.

models.rs
    Core data structures.

repository.rs
    Timesheet database operations.

employer_repository.rs
    Employer database operations.

personal_assistant_repository.rs
    Personal Assistant database operations.

pay_rate_repository.rs
    Pay rate history operations.

csv_import.rs
    CSV importing and validation.

import_service.rs
    Import workflow.

archive.rs
    File archiving.

gui.rs
    User interface layer.
```

Modules should only be added when required by working features.

---

# 6. Development Strategy

Development uses vertical slices.

Each feature should provide a complete working path through the system.

A feature should normally include:

```text
Documentation

    |

Data Model

    |

Database Changes

    |

Repository

    |

Business Logic

    |

Tests

    |

Working Feature
```

---

# 7. Future Technology Direction

Planned technologies:

- Rust programming language.
- SQLite database.
- Embedded services.
- Browser-based access.
- Cross-platform deployment.

The architecture should allow future expansion without requiring a complete rewrite.

---

# 8. Development Principles

Every development stage should:

- Begin with documentation.
- Add the smallest useful feature.
- Include tests where appropriate.
- Leave the application compiling.
- Use database migrations for structural changes.
- Be committed to Git.

The goal is a maintainable, reliable replacement for manual Direct Payment administration workflows.

