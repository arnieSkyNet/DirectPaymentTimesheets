# DirectPaymentTimesheets Application Architecture

## Document Purpose

This document describes the planned architecture for DirectPaymentTimesheets.

The purpose is to define the structure of the application before implementation begins, ensuring that future features can be added without redesigning the system.

The application is intended to provide an open-source replacement for Hours Keeping app/software for UK Direct Payment / Personal Budget administration.

---

# 1. Design Goals

DirectPaymentTimesheets will be designed around the following principles:

- Local-first application design.
- Cross-platform compatibility.
- Reliable storage of care and payroll records.
- Clear separation between data storage, business logic and user interface.
- Documentation and code evolving together.
- Small, testable components.
- Avoiding hard-coded assumptions about payroll arrangements.

---

# 2. High-Level Architecture

The application will consist of several layers:

```text
User Interface
      |
      v
Application Logic
      |
      v
Data Storage
      |
      v
External Services
```

---

# 3. Core Components

## User Interface Layer

Responsible for interaction with the user.

Future capabilities:

- Browser-based interface.
- Desktop access.
- Mobile-friendly access.
- Timesheet entry.
- Approval screens.
- Reports.

---

## Application Logic Layer

Responsible for the rules of the system.

Examples:

- Timesheet calculations.
- Approval workflow.
- Payroll calculations.
- Validation.
- Business rules.

This layer should not depend directly on the user interface.

---

## Database Layer

Responsible for permanent storage.

Technology:

- SQLite

Responsibilities:

- Store timesheets.
- Store Personal Assistant information.
- Store settings.
- Store payroll information.
- Maintain historical records.

---

## Import / Export Layer

Responsible for moving data into and out of the application.

Initial feature:

- CSV import.

Future features:

- CSV export.
- Data backup.
- Payroll exports.

---

## Document Generation Layer

Responsible for producing documents.

Future features:

- PDF timesheets.
- Reports.
- Payroll documents.

---

## Communication Layer

Responsible for external communication.

Future features:

- Emailing payroll departments.
- Sending completed documents.

---

# 4. Data Flow

The planned workflow:

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

# 5. Planned Rust Modules

The initial Rust application will grow towards the following structure:

```text
src/

main.rs
    Application entry point.

config.rs
    Application settings and configuration.

database.rs
    SQLite connection and database operations.

models.rs
    Core data structures.

csv_import.rs
    CSV file importing and validation.

payroll.rs
    Payroll calculations.

pdf.rs
    PDF document generation.

email.rs
    Email handling.
```

Modules will be added only when required by a working feature.

---

# 6. Initial Development Strategy

Development will use vertical slices.

Each feature should provide a complete working path through the system.

The first implementation milestone:

## Version 0.0.1

CSV Import:

```text
CSV File
    |
    v
Importer
    |
    v
Validation
    |
    v
SQLite Storage
```

---

# 7. Future Technology Direction

Planned technologies:

- Rust programming language.
- SQLite database.
- Embedded HTTP server.
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
- Be committed to Git.

The goal is a maintainable, reliable replacement for Hours keeping app/software

