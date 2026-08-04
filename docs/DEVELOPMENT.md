# DirectPaymentTimesheets Development Guide

## Purpose

This document describes the development practices used for DirectPaymentTimesheets.

The purpose is to keep development consistent, maintainable and safe while the application grows.

---

# Development Principles

The project follows these principles:

- Small incremental changes.
- Test before committing.
- Keep documentation aligned with code.
- Preserve existing data.
- Avoid unnecessary complexity.
- Use Git as permanent project history.
- Build the foundation before adding advanced features.

---

# Technology Stack

Current technology:

```
Rust

SQLite

Cargo

Git
```

The application is designed to remain cross-platform.

---

# Development Environment

The project is developed using a Rust toolchain.

Required tools:

```
cargo

rustc

git
```

The Rust version should be kept current enough to support required dependencies.

---

# Common Development Commands

## Check Code

Use:

```
cargo check
```

Purpose:

- Quickly verify compilation.
- Detect code errors.

---

## Run Tests

Use:

```
cargo test
```

Purpose:

- Run automated tests.
- Confirm existing functionality still works.

All tests should pass before committing changes.

---

## Format Code

Use:

```
cargo fmt
```

Purpose:

- Keep Rust formatting consistent.
- Reduce unnecessary code differences.

Formatting should be run before committing Rust changes.

---

# Project Structure

Current source layout:

```
src/

    main.rs

    application.rs
    context.rs
    environment.rs

    database.rs

    models.rs

    repository.rs
    employer_repository.rs
    personal_assistant_repository.rs
    pay_rate_repository.rs

    csv_import.rs
    import_service.rs
    archive.rs

    config.rs
    paths.rs
    error.rs

    gui.rs
```

---

# Adding New Features

New features should normally follow this pattern:

## 1. Define the Domain Requirement

Before writing code:

- Describe the real-world problem.
- Update documentation if required.
- Define the data needed.

---

## 2. Update the Data Model

Add or modify:

```
docs/DATA-MODEL.md
```

Consider:

- Historical accuracy.
- Future expansion.
- Relationships between records.

---

## 3. Update Database Schema

Database changes should use migrations.

Do not manually edit existing databases.

The process should be:

```
Add migration

        |

Update schema version

        |

Test upgrade path

        |

Commit change
```

---

## 4. Add Repository Layer

Database access should be contained inside repositories.

Repositories handle:

- Inserts.
- Queries.
- Updates.
- Data retrieval.

Business rules should not be placed inside repositories.

---

## 5. Add Tests

Every new feature should include tests where practical.

Tests should cover:

- Normal operation.
- Invalid data.
- Edge cases.
- Duplicate handling where relevant.

---

# Database Development

The application uses SQLite.

Database responsibilities:

```
database.rs
```

Handles:

- Creating tables.
- Checking schema version.
- Running migrations.

Repositories handle database operations.

---

# Data Storage Rules

Application data is stored outside the Git repository.

Default location:

```
~/.directpaymenttimesheets/
```

The repository must not contain:

- Real personal information.
- Payroll records.
- Private signatures.
- User databases.

---

# Import Development

CSV imports must:

- Validate incoming data.
- Prevent duplicate imports.
- Preserve original files.
- Record audit information.

Import changes should include tests.

---

# Testing Database Code

Database tests should use isolated databases.

Preferred approach:

```
SQLite in-memory database
```

This prevents tests from modifying real user data.

---

# Git Workflow

Git is used as project history.

Each significant change should:

- Have a clear commit message.
- Represent one logical change.
- Pass tests before committing.

Examples of good commit messages:

```
Add employer repository

Add pay rate history model

Improve database migration handling

Update project documentation
```

---

# Documentation Rules

Documentation should be updated when:

- A major feature is added.
- Database structure changes.
- Architecture changes.
- Business rules change.

Important documents:

```
docs/DATA-MODEL.md

docs/DOMAIN.md

docs/ARCHITECTURE.md

docs/PROJECT_STATE.md
```

---

# Current Development Stage

The current focus is completing the application foundation.

Completed:

- Application structure.
- Configuration handling.
- Environment management.
- SQLite database.
- Database migrations.
- Core repositories.
- CSV import pipeline.
- Import audit system.

---

# Current Development Priorities

Next areas:

## Business Records

Continue building:

- Leave records.
- Public holiday records.
- Payroll periods.

---

## Payroll Engine Preparation

Prepare:

- Pay calculations.
- Historical rates.
- Payroll rules.

---

## Document Generation

Prepare:

- Timesheet PDFs.
- Payroll outputs.
- Email workflow.

---

# Future Development

Future features may include:

- Employer login.
- Personal Assistant self-service.
- Browser/mobile access.
- Advanced payroll automation.

These should only be added after the underlying data model and business rules are stable.

