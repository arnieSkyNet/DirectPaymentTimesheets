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

Configuration controls locations such as:

- CSV import folders.
- Generated PDF folders.
- Archives.
- Future email settings.

---

# Application Context

The application uses an application context layer.

The purpose is to provide:

- Configuration.
- Paths.
- Database connection.
- Logging.
- Application information.

Individual modules should request resources from the application context rather than creating their own paths.

---

# Service Architecture

The application will gradually move towards services.

Example:

```
Import Service
    |
     Discover files
     Validate data
     Import records
     Archive files
     Create audit records


Payroll Service
    |
     Calculate hours
     Apply rates
     Handle leave
     Generate payroll records
```

---

# Data Protection

The application should minimise exposure of sensitive information.

Sensitive identifiers should not be placed unnecessarily into filenames.

For example:

Preferred:

```
Cedar Fixture_20260702-20260801.csv
```

Avoid:

```
Cedar_Fixture_NI_NUMBER.csv
```

---

# Repository Separation

The Git repository contains:

- Source code.
- Documentation.
- Tests.

User data remains outside the repository.

This prevents accidental commits of private payroll information.

