# DirectPaymentTimesheets Development Guide

## Purpose

This document describes the development workflow and standards used when working on DirectPaymentTimesheets.

The goal is to keep development safe, predictable and maintainable as the project grows.

---

# Development Principles

The project follows these principles:

- Make small, controlled changes.
- Keep features isolated.
- Test after changes.
- Commit working stages.
- Preserve project history through Git.
- Avoid unnecessary complexity.
- Keep user data separate from source code.
- Prefer maintainable solutions over quick fixes.

---

# Technology Stack

Current technologies:

- Rust programming language.
- Cargo build system.
- SQLite database.
- Git version control.

---

# Project Structure

Main source directory:

    src/

Important components:

    main.rs

Application entry point.

    application.rs

Application startup and coordination.

    context.rs

Provides application context.

    config.rs

Handles configuration.

    database.rs

Handles database initialisation.

    repository.rs

Database access layer.

    import_service.rs

Coordinates CSV importing.

    csv_import.rs

CSV validation and parsing.

    archive.rs

Handles archived CSV files.

---

# Development Checks

After making code changes run:

    cargo check

This verifies that the project compiles.

For automated tests run:

    cargo test

Tests should pass before committing changes.

---

# Testing Approach

Tests should be added when new functionality is created.

Current testing areas include:

- CSV parsing.
- Duration conversion.
- Money conversion.
- Invalid input handling.
- Database operations.
- Duplicate detection.
- Import auditing.
- Archive behaviour.

---

# Git Workflow

Git is used as the permanent project history.

Normal workflow:

1. Make a small change.
2. Run cargo check.
3. Run cargo test where appropriate.
4. Review changes.
5. Commit.
6. Push to GitHub.

Commits should describe the change clearly.

Examples:

    Add CSV duplicate detection

    Improve archive filename handling

    Update project documentation

---

# Documentation Updates

Documentation should be updated when architecture or workflow changes.

Important documents:

    docs/ARCHITECTURE.md

Technical architecture and design decisions.

    docs/PROJECT_STATE.md

Current project status and completed milestones.

    docs/DEVELOPMENT.md

Development workflow and standards.

    docs/DOMAIN.md

Business concepts and domain rules.

---

# Database Development

Database changes should be handled carefully.

Principles:

- Preserve existing data.
- Avoid destructive changes.
- Use migrations when the database becomes more complex.
- Keep repository logic separate from database setup.

---

# Adding New Features

New features should normally follow this process:

1. Define the requirement.
2. Update architecture or domain documentation if needed.
3. Add or update data models.
4. Add repository support.
5. Add service logic.
6. Add tests.
7. Update documentation.
8. Commit the completed stage.

---

# Error Handling

Errors should be:

- Clear.
- Useful for debugging.
- Recorded where appropriate.
- Visible to the user when action is required.

Import failures should always create audit records.

---

# Privacy and Security

The project handles sensitive Direct Payment information.

Development rules:

- Never commit user payroll data.
- Keep personal data outside the repository.
- Avoid unnecessary personal information in filenames.
- Preserve audit history.
- Prefer local storage unless a future feature requires otherwise.

---

# Current Development Status

The foundation stage is complete.

Completed:

- Application structure.
- Configuration handling.
- Database foundation.
- Repository layer.
- CSV import pipeline.
- Validation.
- Duplicate detection.
- Archive handling.
- Import auditing.
- Documentation foundation.

Future development will continue by strengthening the foundation before implementing payroll processing features.

