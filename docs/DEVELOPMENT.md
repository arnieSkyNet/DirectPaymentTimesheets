# DirectPaymentTimesheets Development Guide

## Development Principles

The project follows these principles:

- Make small controlled changes.
- Keep Git history meaningful.
- Test after changes.
- Document important decisions.

---

# Working Method

Because development may involve physical accessibility limitations:

- Batch related changes together.
- Avoid unnecessary editing sessions.
- Prefer complete file replacements.
- Minimise repetitive commands.

---

# Before Coding

Review:

- Existing files.
- Current architecture.
- Database design.
- Previous commits.

Avoid assumptions about current code.

---

# Git Workflow

Preferred workflow:

```
Make change



cargo check



cargo run/test



git commit



continue development
```

---

# Code Quality

Priorities:

- Clear code.
- Maintainability.
- Avoid unnecessary complexity.
- Cross-platform compatibility.

---

# Documentation

Documentation should be updated when:

- Architecture changes.
- New features are completed.
- Design decisions are made.

---

# Current Development Stage

Foundation complete.

Implemented:

- Configuration.
- Application environment.
- SQLite database.
- Repository layer.
- CSV import.
- Validation.
- Duplicate detection.
- Archive system.
- Import audit trail.

Next:

Improve import workflow and continue towards payroll processing.

