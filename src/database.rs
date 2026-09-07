use std::path::Path;

use rusqlite::{Connection, Result};

pub const CURRENT_SCHEMA_VERSION: i64 = 25;

pub fn initialise_database(database_path: &Path) -> Result<()> {
    let connection = Connection::open(database_path)?;

    create_schema(&connection)?;

    println!("Database initialised.");

    Ok(())
}

pub fn create_schema(connection: &Connection) -> Result<()> {
    connection.execute(
        "
        CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER NOT NULL
        )
        ",
        [],
    )?;

    let version_count: i64 =
        connection.query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0))?;

    if version_count == 0 {
        connection.execute("INSERT INTO schema_version (version) VALUES (1)", [])?;
    }

    connection.execute(
        "
        CREATE TABLE IF NOT EXISTS timesheets (
            id INTEGER PRIMARY KEY,
            pa_name TEXT NOT NULL,
            start_time TEXT NOT NULL,
            end_time TEXT NOT NULL,
            break_minutes INTEGER NOT NULL,
            worked_minutes INTEGER NOT NULL,
            hourly_rate REAL NOT NULL,
            amount REAL NOT NULL,
            notes TEXT
        )
        ",
        [],
    )?;

    connection.execute(
        "
        CREATE TABLE IF NOT EXISTS import_audit (
            id INTEGER PRIMARY KEY,
            import_time TEXT NOT NULL,
            original_filename TEXT NOT NULL,
            archive_filename TEXT NOT NULL,
            rows_processed INTEGER NOT NULL,
            rows_imported INTEGER NOT NULL,
            rows_skipped INTEGER NOT NULL,
            status TEXT NOT NULL,
            error_message TEXT
        )
        ",
        [],
    )?;

    repair_unreleased_schema_20(connection)?;
    apply_migrations(connection)?;

    Ok(())
}

fn apply_migrations(connection: &Connection) -> Result<()> {
    let mut current_version: i64 =
        connection.query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
            row.get(0)
        })?;

    if current_version < 2 {
        migrate_to_version_2(connection)?;
        current_version = 2;
    }

    if current_version < 3 {
        migrate_to_version_3(connection)?;
        current_version = 3;
    }

    if current_version < 4 {
        migrate_to_version_4(connection)?;
        current_version = 4;
    }

    if current_version < 5 {
        migrate_to_version_5(connection)?;
        current_version = 5;
    }

    if current_version < 6 {
        migrate_to_version_6(connection)?;
        current_version = 6;
    }

    if current_version < 7 {
        migrate_to_version_7(connection)?;
        current_version = 7;
    }

    if current_version < 8 {
        migrate_to_version_8(connection)?;
        current_version = 8;
    }

    if current_version < 9 {
        migrate_to_version_9(connection)?;
        current_version = 9;
    }

    if current_version < 10 {
        migrate_to_version_10(connection)?;
        current_version = 10;
    }

    if current_version < 11 {
        migrate_to_version_11(connection)?;
        current_version = 11;
    }

    if current_version < 12 {
        migrate_to_version_12(connection)?;
        current_version = 12;
    }

    if current_version < 13 {
        migrate_to_version_13(connection)?;
        current_version = 13;
    }

    if current_version < 14 {
        migrate_to_version_14(connection)?;
        current_version = 14;
    }

    if current_version < 15 {
        migrate_to_version_15(connection)?;
        current_version = 15;
    }

    if current_version < 16 {
        migrate_to_version_16(connection)?;
        current_version = 16;
    }

    if current_version < 17 {
        migrate_to_version_17(connection)?;
        current_version = 17;
    }

    if current_version < 18 {
        migrate_to_version_18(connection)?;
        current_version = 18;
    }

    if current_version < 19 {
        migrate_to_version_19(connection)?;
        current_version = 19;
    }

    if current_version < 20 {
        migrate_to_version_20(connection)?;
        current_version = 20;
    }

    if current_version < 21 {
        migrate_to_version_21(connection)?;
        current_version = 21;
    }

    if current_version < 22 {
        migrate_to_version_22(connection)?;
        current_version = 22;
    }

    if current_version < 23 {
        migrate_to_version_23(connection)?;
        current_version = 23;
    }

    if current_version < 24 {
        migrate_to_version_24(connection)?;
        current_version = 24;
    }
    if current_version < 25 {
        migrate_to_version_25(connection)?;
    }

    Ok(())
}

fn migrate_to_version_2(connection: &Connection) -> Result<()> {
    connection.execute(
        "
        CREATE TABLE IF NOT EXISTS employers (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            address TEXT,
            postcode TEXT,
            telephone TEXT,
            email TEXT,
            payroll_provider TEXT,
            payroll_provider_address TEXT,
            payroll_provider_phone TEXT,
            employer_signature TEXT,
            default_pdf_template TEXT
        )
        ",
        [],
    )?;

    connection.execute(
        "
        CREATE TABLE IF NOT EXISTS personal_assistants (
            id INTEGER PRIMARY KEY,
            first_name TEXT NOT NULL,
            surname TEXT NOT NULL,
            date_of_birth TEXT,
            national_insurance_number TEXT,
            address TEXT,
            postcode TEXT,
            telephone TEXT,
            email TEXT,
            employment_status TEXT
        )
        ",
        [],
    )?;

    connection.execute("UPDATE schema_version SET version = 2", [])?;

    Ok(())
}

fn migrate_to_version_3(connection: &Connection) -> Result<()> {
    connection.execute(
        "
        CREATE TABLE IF NOT EXISTS personal_assistant_pay_rates (
            id INTEGER PRIMARY KEY,
            personal_assistant_id INTEGER NOT NULL,
            effective_date TEXT NOT NULL,
            base_hourly_rate REAL NOT NULL,
            employer_top_up_rate REAL NOT NULL,
            created_at TEXT NOT NULL
        )
        ",
        [],
    )?;

    connection.execute("UPDATE schema_version SET version = 3", [])?;

    Ok(())
}

fn migrate_to_version_4(connection: &Connection) -> Result<()> {
    connection.execute(
        "
        ALTER TABLE timesheets
        ADD COLUMN personal_assistant_id INTEGER
        ",
        [],
    )?;

    connection.execute("UPDATE schema_version SET version = 4", [])?;

    Ok(())
}

fn migrate_to_version_5(connection: &Connection) -> Result<()> {
    connection.execute(
        "
        ALTER TABLE employers
        ADD COLUMN sick_pay_enabled INTEGER NOT NULL DEFAULT 0
        ",
        [],
    )?;

    connection.execute(
        "
        ALTER TABLE employers
        ADD COLUMN mileage_enabled INTEGER NOT NULL DEFAULT 0
        ",
        [],
    )?;

    connection.execute(
        "
        ALTER TABLE personal_assistants
        ADD COLUMN sick_pay_enabled INTEGER NOT NULL DEFAULT 0
        ",
        [],
    )?;

    connection.execute(
        "
        ALTER TABLE personal_assistants
        ADD COLUMN mileage_enabled INTEGER NOT NULL DEFAULT 0
        ",
        [],
    )?;

    connection.execute("UPDATE schema_version SET version = 5", [])?;

    Ok(())
}

fn migrate_to_version_6(connection: &Connection) -> Result<()> {
    connection.execute(
        "
        CREATE TABLE IF NOT EXISTS payroll_provider (
            id INTEGER PRIMARY KEY,
            name TEXT,
            email TEXT,
            address TEXT,
            telephone TEXT
        )
        ",
        [],
    )?;

    connection.execute("UPDATE schema_version SET version = 6", [])?;

    Ok(())
}

fn migrate_to_version_7(connection: &Connection) -> Result<()> {
    connection.execute(
        "
        ALTER TABLE employers
        ADD COLUMN date_of_birth TEXT
        ",
        [],
    )?;

    connection.execute(
        "
        ALTER TABLE employers
        ADD COLUMN national_insurance_number TEXT
        ",
        [],
    )?;

    connection.execute(
        "
        ALTER TABLE employers
        ADD COLUMN reference_account_number TEXT
        ",
        [],
    )?;

    connection.execute("UPDATE schema_version SET version = 7", [])?;

    Ok(())
}

fn migrate_to_version_8(connection: &Connection) -> Result<()> {
    connection.execute(
        "
        ALTER TABLE personal_assistants
        ADD COLUMN start_date TEXT
        ",
        [],
    )?;

    connection.execute("UPDATE schema_version SET version = 8", [])?;

    Ok(())
}

fn migrate_to_version_9(connection: &Connection) -> Result<()> {
    connection.execute(
        "
        ALTER TABLE payroll_provider
        ADD COLUMN payroll_department_email TEXT
        ",
        [],
    )?;

    connection.execute("UPDATE schema_version SET version = 9", [])?;

    Ok(())
}

fn migrate_to_version_10(connection: &Connection) -> Result<()> {
    connection.execute(
        "
        CREATE TABLE IF NOT EXISTS personal_assistant_contracted_hours (
            id INTEGER PRIMARY KEY,
            personal_assistant_id INTEGER NOT NULL,
            effective_date TEXT NOT NULL,
            contracted_hours TEXT NOT NULL,
            created_at TEXT NOT NULL
        )
        ",
        [],
    )?;

    connection.execute("UPDATE schema_version SET version = 10", [])?;

    Ok(())
}

fn migrate_to_version_11(connection: &Connection) -> Result<()> {
    connection.execute(
        "
        CREATE TABLE IF NOT EXISTS payroll_schedules (
            id INTEGER PRIMARY KEY,
            payroll_year TEXT NOT NULL,
            cycle_number INTEGER NOT NULL,
            first_week_commencing TEXT NOT NULL,
            latest_posting_date TEXT NOT NULL,
            pay_date TEXT NOT NULL,
            created_at TEXT NOT NULL
        )
        ",
        [],
    )?;

    connection.execute("UPDATE schema_version SET version = 11", [])?;

    Ok(())
}

fn migrate_to_version_12(connection: &Connection) -> Result<()> {
    connection.execute(
        "
        ALTER TABLE personal_assistants
        ADD COLUMN signature TEXT
        ",
        [],
    )?;

    connection.execute("UPDATE schema_version SET version = 12", [])?;

    Ok(())
}

fn migrate_to_version_13(connection: &Connection) -> Result<()> {
    connection.execute(
        "
        CREATE TABLE IF NOT EXISTS payroll_timesheets (
            id INTEGER PRIMARY KEY,
            personal_assistant_id INTEGER NOT NULL,
            payroll_year TEXT NOT NULL,
            cycle_number INTEGER NOT NULL,
            previous_cycle_hours REAL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE (
                personal_assistant_id,
                payroll_year,
                cycle_number
            )
        )
        ",
        [],
    )?;

    connection.execute(
        "
        CREATE TABLE IF NOT EXISTS payroll_timesheet_weeks (
            id INTEGER PRIMARY KEY,
            payroll_timesheet_id INTEGER NOT NULL,
            week_number INTEGER NOT NULL,
            week_commencing TEXT NOT NULL,
            worked_hours REAL NOT NULL DEFAULT 0,
            annual_leave_hours REAL NOT NULL DEFAULT 0,
            sick_leave_hours REAL NOT NULL DEFAULT 0,
            public_holiday_hours REAL NOT NULL DEFAULT 0,
            travel_miles REAL NOT NULL DEFAULT 0,
            UNIQUE (
                payroll_timesheet_id,
                week_number
            )
        )
        ",
        [],
    )?;

    connection.execute("UPDATE schema_version SET version = 13", [])?;

    Ok(())
}

fn migrate_to_version_14(connection: &Connection) -> Result<()> {
    connection.execute(
        "
        CREATE TABLE IF NOT EXISTS payroll_timesheet_public_holidays (
            id INTEGER PRIMARY KEY,
            payroll_timesheet_id INTEGER NOT NULL,
            week_number INTEGER NOT NULL,
            holiday_date TEXT NOT NULL,
            hours REAL NOT NULL DEFAULT 0,
            UNIQUE (
                payroll_timesheet_id,
                week_number,
                holiday_date
            )
        )
        ",
        [],
    )?;

    connection.execute("UPDATE schema_version SET version = 14", [])?;

    Ok(())
}

fn migrate_to_version_15(connection: &Connection) -> Result<()> {
    connection.execute(
        "
        ALTER TABLE employers
        ADD COLUMN email_signature TEXT
        ",
        [],
    )?;

    connection.execute("UPDATE schema_version SET version = 15", [])?;

    Ok(())
}

fn migrate_to_version_16(connection: &Connection) -> Result<()> {
    connection.execute(
        "
        ALTER TABLE payroll_schedules
        ADD COLUMN payslips_sent INTEGER NOT NULL DEFAULT 0
        ",
        [],
    )?;

    connection.execute("UPDATE schema_version SET version = 16", [])?;

    Ok(())
}

fn migrate_to_version_17(connection: &Connection) -> Result<()> {
    connection.execute(
        "
        CREATE TABLE IF NOT EXISTS payroll_timesheet_email_status (
            id INTEGER PRIMARY KEY,
            personal_assistant_id INTEGER NOT NULL,
            payroll_year TEXT NOT NULL,
            cycle_number INTEGER NOT NULL,
            sent_at TEXT,
            UNIQUE (
                personal_assistant_id,
                payroll_year,
                cycle_number
            )
        )
        ",
        [],
    )?;

    connection.execute("UPDATE schema_version SET version = 17", [])?;

    Ok(())
}

fn migrate_to_version_18(connection: &Connection) -> Result<()> {
    let transaction = connection.unchecked_transaction()?;

    transaction.execute_batch(
        "
        CREATE TABLE payroll_timesheet_email_status_new (
            id INTEGER PRIMARY KEY,
            personal_assistant_id INTEGER NOT NULL,
            payroll_year TEXT NOT NULL,
            cycle_number INTEGER NOT NULL,
            email_type TEXT NOT NULL,
            sent_at TEXT,
            UNIQUE (
                personal_assistant_id,
                payroll_year,
                cycle_number,
                email_type
            )
        );

        INSERT INTO payroll_timesheet_email_status_new (
            id,
            personal_assistant_id,
            payroll_year,
            cycle_number,
            email_type,
            sent_at
        )
        SELECT
            id,
            personal_assistant_id,
            payroll_year,
            cycle_number,
            'timesheet',
            sent_at
        FROM payroll_timesheet_email_status;

        DROP TABLE payroll_timesheet_email_status;
        ALTER TABLE payroll_timesheet_email_status_new
        RENAME TO payroll_timesheet_email_status;
        ",
    )?;

    transaction.execute("UPDATE schema_version SET version = 18", [])?;
    transaction.commit()?;

    Ok(())
}

fn migrate_to_version_19(connection: &Connection) -> Result<()> {
    let transaction = connection.unchecked_transaction()?;
    transaction.execute_batch(
        "
        CREATE TABLE payroll_timesheet_manual_adjustments (
            id INTEGER PRIMARY KEY,
            payroll_timesheet_id INTEGER NOT NULL,
            week_number INTEGER NOT NULL,
            adjustment_minutes INTEGER NOT NULL,
            reason TEXT,
            updated_at TEXT NOT NULL,
            UNIQUE (payroll_timesheet_id, week_number)
        );

        CREATE TABLE payroll_timesheet_worked_item_snapshots (
            id INTEGER PRIMARY KEY,
            payroll_timesheet_id INTEGER NOT NULL,
            week_number INTEGER NOT NULL,
            source_type TEXT NOT NULL,
            timesheet_id INTEGER,
            work_date TEXT,
            worked_minutes INTEGER NOT NULL,
            pay_rate_id INTEGER,
            pay_rate_effective_date TEXT,
            total_hourly_rate REAL,
            reason TEXT,
            captured_at TEXT NOT NULL,
            CHECK (
                (source_type = 'legacy_previous_cycle_adjustment'
                 AND timesheet_id IS NULL
                 AND work_date IS NULL
                 AND pay_rate_id IS NULL
                 AND pay_rate_effective_date IS NULL
                 AND total_hourly_rate IS NULL)
                OR
                (source_type <> 'legacy_previous_cycle_adjustment'
                 AND pay_rate_id IS NOT NULL
                 AND pay_rate_effective_date IS NOT NULL
                 AND total_hourly_rate IS NOT NULL)
            )
        );

        CREATE TABLE payroll_timesheet_snapshot_states (
            payroll_timesheet_id INTEGER PRIMARY KEY,
            state TEXT NOT NULL CHECK (state IN ('candidate', 'submitted', 'indeterminate')),
            pdf_path TEXT NOT NULL,
            pdf_sha256 TEXT NOT NULL,
            generated_at TEXT NOT NULL,
            submitted_at TEXT,
            indeterminate_at TEXT
        );

        CREATE UNIQUE INDEX payroll_snapshot_raw_shift
        ON payroll_timesheet_worked_item_snapshots (
            payroll_timesheet_id,
            timesheet_id
        )
        WHERE timesheet_id IS NOT NULL;

        CREATE INDEX payroll_snapshot_timesheet_id
        ON payroll_timesheet_worked_item_snapshots (timesheet_id);
        ",
    )?;
    transaction.execute("UPDATE schema_version SET version = 19", [])?;
    transaction.commit()?;
    Ok(())
}

fn migrate_to_version_20(connection: &Connection) -> Result<()> {
    let transaction = connection.unchecked_transaction()?;
    create_final_schema_20_tables(&transaction)?;
    transaction.execute("UPDATE schema_version SET version = 20", [])?;
    transaction.commit()?;
    Ok(())
}

fn migrate_to_version_21(connection: &Connection) -> Result<()> {
    let transaction = connection.unchecked_transaction()?;
    transaction.execute_batch(
        "
        CREATE TABLE timesheet_correction_events (
            id INTEGER PRIMARY KEY,
            timesheet_id INTEGER NOT NULL,
            actor_id TEXT NOT NULL CHECK (length(trim(actor_id)) > 0),
            action_type TEXT NOT NULL CHECK (action_type IN ('edit', 'revert')),
            action_at TEXT NOT NULL CHECK (length(trim(action_at)) > 0),
            reason TEXT,
            before_start_time TEXT NOT NULL,
            before_end_time TEXT NOT NULL,
            before_break_minutes INTEGER NOT NULL CHECK (before_break_minutes >= 0),
            before_worked_minutes INTEGER NOT NULL CHECK (before_worked_minutes >= 0),
            before_notes TEXT,
            after_start_time TEXT NOT NULL,
            after_end_time TEXT NOT NULL,
            after_break_minutes INTEGER NOT NULL CHECK (after_break_minutes >= 0),
            after_worked_minutes INTEGER NOT NULL CHECK (after_worked_minutes >= 0),
            after_notes TEXT
        );

        CREATE INDEX timesheet_correction_events_timesheet_id
        ON timesheet_correction_events (timesheet_id, id);
        ",
    )?;
    transaction.execute("UPDATE schema_version SET version = 21", [])?;
    transaction.commit()?;
    Ok(())
}

fn migrate_to_version_22(connection: &Connection) -> Result<()> {
    let transaction = connection.unchecked_transaction()?;
    transaction.execute_batch(
        "
        CREATE TABLE payroll_timesheet_revisions (
            id INTEGER PRIMARY KEY,
            payroll_timesheet_id INTEGER NOT NULL,
            revision_number INTEGER NOT NULL CHECK (revision_number > 0),
            state TEXT NOT NULL CHECK (state IN ('candidate', 'submitted', 'indeterminate')),
            pdf_path TEXT NOT NULL CHECK (length(trim(pdf_path)) > 0),
            pdf_sha256 TEXT NOT NULL CHECK (length(trim(pdf_sha256)) > 0),
            generated_at TEXT NOT NULL CHECK (length(trim(generated_at)) > 0),
            send_attempted_at TEXT,
            submitted_at TEXT,
            indeterminate_at TEXT,
            legacy_backfilled INTEGER NOT NULL DEFAULT 0 CHECK (legacy_backfilled IN (0, 1)),
            UNIQUE (payroll_timesheet_id, revision_number),
            UNIQUE (payroll_timesheet_id, pdf_path)
        );

        CREATE UNIQUE INDEX new_payroll_timesheet_revision_pdf_path
        ON payroll_timesheet_revisions (pdf_path)
        WHERE legacy_backfilled = 0;

        CREATE UNIQUE INDEX one_candidate_payroll_timesheet_revision
        ON payroll_timesheet_revisions (payroll_timesheet_id)
        WHERE state = 'candidate';

        CREATE INDEX payroll_timesheet_revisions_history
        ON payroll_timesheet_revisions (payroll_timesheet_id, revision_number DESC);

        CREATE TABLE payroll_timesheet_revision_worked_items (
            id INTEGER PRIMARY KEY,
            payroll_timesheet_revision_id INTEGER NOT NULL,
            week_number INTEGER NOT NULL,
            source_type TEXT NOT NULL CHECK (length(trim(source_type)) > 0),
            timesheet_id INTEGER,
            timesheet_correction_event_id INTEGER,
            direct_shift_id INTEGER,
            direct_shift_audit_id INTEGER,
            effective_start_time TEXT,
            effective_end_time TEXT,
            effective_break_minutes INTEGER CHECK (
                effective_break_minutes IS NULL OR effective_break_minutes >= 0
            ),
            effective_notes TEXT,
            work_date TEXT,
            worked_minutes INTEGER NOT NULL,
            pay_rate_id INTEGER,
            pay_rate_effective_date TEXT,
            total_hourly_rate REAL,
            reason TEXT,
            captured_at TEXT NOT NULL CHECK (length(trim(captured_at)) > 0),
            CHECK (timesheet_correction_event_id IS NULL OR timesheet_id IS NOT NULL),
            CHECK (direct_shift_audit_id IS NULL OR direct_shift_id IS NOT NULL),
            CHECK (timesheet_id IS NULL OR direct_shift_id IS NULL)
        );

        CREATE UNIQUE INDEX payroll_revision_imported_source
        ON payroll_timesheet_revision_worked_items (
            payroll_timesheet_revision_id, timesheet_id
        ) WHERE timesheet_id IS NOT NULL;

        CREATE UNIQUE INDEX payroll_revision_direct_source
        ON payroll_timesheet_revision_worked_items (
            payroll_timesheet_revision_id, direct_shift_id
        ) WHERE direct_shift_id IS NOT NULL;

        CREATE INDEX payroll_revision_worked_items_revision
        ON payroll_timesheet_revision_worked_items (payroll_timesheet_revision_id, id);

        CREATE INDEX payroll_revision_worked_items_import_version
        ON payroll_timesheet_revision_worked_items (timesheet_id, timesheet_correction_event_id);

        CREATE INDEX payroll_revision_worked_items_direct_version
        ON payroll_timesheet_revision_worked_items (direct_shift_id, direct_shift_audit_id);

        CREATE TABLE payroll_timesheet_revision_weeks (
            id INTEGER PRIMARY KEY,
            payroll_timesheet_revision_id INTEGER NOT NULL,
            week_number INTEGER NOT NULL,
            week_commencing TEXT NOT NULL,
            worked_hours REAL NOT NULL,
            annual_leave_hours REAL NOT NULL,
            sick_leave_hours REAL NOT NULL,
            public_holiday_hours REAL NOT NULL,
            travel_miles REAL NOT NULL,
            UNIQUE (payroll_timesheet_revision_id, week_number)
        );

        CREATE TABLE payroll_timesheet_revision_public_holidays (
            id INTEGER PRIMARY KEY,
            payroll_timesheet_revision_id INTEGER NOT NULL,
            week_number INTEGER NOT NULL,
            holiday_date TEXT NOT NULL,
            hours REAL NOT NULL,
            UNIQUE (payroll_timesheet_revision_id, week_number, holiday_date)
        );

        CREATE TABLE payroll_timesheet_revision_delivery_attempts (
            id INTEGER PRIMARY KEY,
            payroll_timesheet_revision_id INTEGER NOT NULL,
            outcome TEXT NOT NULL CHECK (
                outcome IN ('protected', 'failed', 'submitted', 'indeterminate',
                            'reconciled_sent', 'reconciled_unsent')
            ),
            attempted_at TEXT NOT NULL CHECK (length(trim(attempted_at)) > 0),
            completed_at TEXT,
            recipient_to TEXT,
            recipient_cc TEXT,
            recipient_bcc TEXT,
            subject TEXT,
            attachment_path TEXT NOT NULL CHECK (length(trim(attachment_path)) > 0),
            attachment_sha256 TEXT NOT NULL CHECK (length(trim(attachment_sha256)) > 0),
            transport_error TEXT
        );

        CREATE INDEX payroll_revision_delivery_attempts_history
        ON payroll_timesheet_revision_delivery_attempts (
            payroll_timesheet_revision_id, id
        );

        INSERT INTO payroll_timesheet_revisions (
            payroll_timesheet_id, revision_number, state, pdf_path, pdf_sha256,
            generated_at, send_attempted_at, submitted_at, indeterminate_at,
            legacy_backfilled
        )
        SELECT payroll_timesheet_id, 1, state, pdf_path, pdf_sha256,
               generated_at, indeterminate_at, submitted_at, indeterminate_at, 1
        FROM payroll_timesheet_snapshot_states;

        INSERT INTO payroll_timesheet_revision_worked_items (
            payroll_timesheet_revision_id, week_number, source_type, timesheet_id,
            work_date, worked_minutes, pay_rate_id, pay_rate_effective_date,
            total_hourly_rate, reason, captured_at
        )
        SELECT revision.id, item.week_number, item.source_type, item.timesheet_id,
               item.work_date, item.worked_minutes, item.pay_rate_id,
               item.pay_rate_effective_date, item.total_hourly_rate, item.reason,
               item.captured_at
        FROM payroll_timesheet_worked_item_snapshots AS item
        INNER JOIN payroll_timesheet_revisions AS revision
            ON revision.payroll_timesheet_id = item.payroll_timesheet_id
           AND revision.revision_number = 1;

        INSERT INTO payroll_timesheet_revision_weeks (
            payroll_timesheet_revision_id, week_number, week_commencing,
            worked_hours, annual_leave_hours, sick_leave_hours,
            public_holiday_hours, travel_miles
        )
        SELECT revision.id, week.week_number, week.week_commencing,
               week.worked_hours, week.annual_leave_hours, week.sick_leave_hours,
               week.public_holiday_hours, week.travel_miles
        FROM payroll_timesheet_weeks AS week
        INNER JOIN payroll_timesheet_revisions AS revision
            ON revision.payroll_timesheet_id = week.payroll_timesheet_id
           AND revision.revision_number = 1;

        INSERT INTO payroll_timesheet_revision_public_holidays (
            payroll_timesheet_revision_id, week_number, holiday_date, hours
        )
        SELECT revision.id, holiday.week_number, holiday.holiday_date, holiday.hours
        FROM payroll_timesheet_public_holidays AS holiday
        INNER JOIN payroll_timesheet_revisions AS revision
            ON revision.payroll_timesheet_id = holiday.payroll_timesheet_id
           AND revision.revision_number = 1;
        ",
    )?;
    transaction.execute("UPDATE schema_version SET version = 22", [])?;
    transaction.commit()?;
    Ok(())
}

fn migrate_to_version_23(connection: &Connection) -> Result<()> {
    let transaction = connection.unchecked_transaction()?;
    transaction.execute_batch(
        "DROP TABLE IF EXISTS payroll_timesheet_revision_delivery_attempts;
         DROP TABLE IF EXISTS payroll_timesheet_revision_public_holidays;
         DROP TABLE IF EXISTS payroll_timesheet_revision_weeks;
         DROP TABLE IF EXISTS payroll_timesheet_revision_worked_items;
         DROP TABLE IF EXISTS payroll_timesheet_revisions;",
    )?;
    transaction.execute("UPDATE schema_version SET version = 23", [])?;
    transaction.commit()?;
    Ok(())
}

fn migrate_to_version_24(connection: &Connection) -> Result<()> {
    let transaction = connection.unchecked_transaction()?;
    transaction.execute_batch(
        "ALTER TABLE personal_assistant_contracted_hours
         ADD COLUMN hours_basis TEXT NOT NULL DEFAULT 'contracted'
         CHECK (hours_basis IN ('contracted', 'variable'));
         UPDATE schema_version SET version = 24;",
    )?;
    transaction.commit()
}

fn migrate_to_version_25(connection: &Connection) -> Result<()> {
    let transaction = connection.unchecked_transaction()?;
    transaction.execute_batch(
        "CREATE TABLE payroll_timesheet_annual_leave (
            id INTEGER PRIMARY KEY,
            payroll_timesheet_id INTEGER NOT NULL,
            week_number INTEGER NOT NULL CHECK (week_number BETWEEN 1 AND 4),
            leave_date TEXT NOT NULL,
            hours REAL NOT NULL CHECK (hours >= 0 AND hours <= 1.7976931348623157e308),
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(payroll_timesheet_id, week_number, leave_date)
        );
        UPDATE schema_version SET version = 25;",
    )?;
    transaction.commit()
}

fn repair_unreleased_schema_20(connection: &Connection) -> Result<()> {
    let version: i64 =
        connection.query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
            row.get(0)
        })?;
    if version != 20 {
        return Ok(());
    }

    let transaction = connection.unchecked_transaction()?;
    if !table_exists(&transaction, "direct_shifts")? {
        create_final_schema_20_tables(&transaction)?;
        transaction.commit()?;
        return Ok(());
    }

    let has_deleted_at = table_has_column(&transaction, "direct_shifts", "deleted_at")?;
    let has_deleted_by = table_has_column(&transaction, "direct_shifts", "deleted_by")?;
    if !has_deleted_at || !has_deleted_by {
        if !has_deleted_at {
            transaction.execute("ALTER TABLE direct_shifts ADD COLUMN deleted_at TEXT", [])?;
        }
        if !has_deleted_by {
            transaction.execute("ALTER TABLE direct_shifts ADD COLUMN deleted_by TEXT", [])?;
        }
        transaction.execute_batch(
            "
            DROP INDEX IF EXISTS one_running_direct_shift_per_pa;
            DROP INDEX IF EXISTS direct_shifts_pa_start;
            ALTER TABLE direct_shifts RENAME TO direct_shifts_development_20;
            ",
        )?;
        create_final_schema_20_tables(&transaction)?;
        transaction.execute_batch(
            "
            INSERT INTO direct_shifts (
                id, personal_assistant_id, start_time, end_time, break_minutes,
                notes, source_type, created_at, updated_at, deleted_at, deleted_by
            )
            SELECT
                id, personal_assistant_id, start_time, end_time, break_minutes,
                notes, source_type, created_at, updated_at,
                CASE WHEN deleted_at IS NOT NULL AND deleted_by IS NOT NULL THEN deleted_at END,
                CASE WHEN deleted_at IS NOT NULL AND deleted_by IS NOT NULL THEN deleted_by END
            FROM direct_shifts_development_20;
            DROP TABLE direct_shifts_development_20;
            ",
        )?;
    } else {
        create_final_schema_20_tables(&transaction)?;
    }
    transaction.commit()?;
    Ok(())
}

fn create_final_schema_20_tables(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS direct_shifts (
            id INTEGER PRIMARY KEY,
            personal_assistant_id INTEGER NOT NULL,
            start_time TEXT NOT NULL,
            end_time TEXT,
            break_minutes INTEGER NOT NULL DEFAULT 0 CHECK (break_minutes >= 0),
            notes TEXT,
            source_type TEXT NOT NULL DEFAULT 'direct' CHECK (source_type = 'direct'),
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            deleted_at TEXT,
            deleted_by TEXT,
            CHECK (
                (deleted_at IS NULL AND deleted_by IS NULL)
                OR (deleted_at IS NOT NULL AND deleted_by IS NOT NULL)
            ),
            CHECK (end_time IS NULL OR end_time >= start_time)
        );

        CREATE TABLE IF NOT EXISTS direct_shift_audit (
            id INTEGER PRIMARY KEY,
            direct_shift_id INTEGER NOT NULL,
            actor_id TEXT NOT NULL,
            action_type TEXT NOT NULL CHECK (
                action_type IN ('clock_in', 'clock_out', 'edit', 'delete', 'cancel_clock_in')
            ),
            action_at TEXT NOT NULL,
            before_personal_assistant_id INTEGER,
            before_start_time TEXT,
            before_end_time TEXT,
            before_break_minutes INTEGER,
            before_notes TEXT,
            before_updated_at TEXT,
            before_deleted_at TEXT,
            before_deleted_by TEXT,
            after_personal_assistant_id INTEGER,
            after_start_time TEXT,
            after_end_time TEXT,
            after_break_minutes INTEGER,
            after_notes TEXT,
            after_updated_at TEXT,
            after_deleted_at TEXT,
            after_deleted_by TEXT
        );

        CREATE UNIQUE INDEX IF NOT EXISTS one_running_direct_shift_per_pa
        ON direct_shifts (personal_assistant_id)
        WHERE end_time IS NULL AND deleted_at IS NULL;

        CREATE INDEX IF NOT EXISTS direct_shifts_pa_start
        ON direct_shifts (personal_assistant_id, start_time DESC, id DESC);

        CREATE INDEX IF NOT EXISTS direct_shift_audit_shift_action
        ON direct_shift_audit (direct_shift_id, id);
        ",
    )?;
    Ok(())
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool> {
    connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [table],
        |row| row.get(0),
    )
}

fn table_has_column(connection: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    for existing in columns {
        if existing? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drop_schema_22(connection: &Connection) {
        connection
            .execute("DROP TABLE payroll_timesheet_annual_leave", [])
            .unwrap();
        connection
            .execute(
                "ALTER TABLE personal_assistant_contracted_hours DROP COLUMN hours_basis",
                [],
            )
            .unwrap();
        connection
            .execute_batch(
                "DROP TABLE IF EXISTS payroll_timesheet_revision_delivery_attempts;
                 DROP TABLE IF EXISTS payroll_timesheet_revision_public_holidays;
                 DROP TABLE IF EXISTS payroll_timesheet_revision_weeks;
                 DROP TABLE IF EXISTS payroll_timesheet_revision_worked_items;
                 DROP TABLE IF EXISTS payroll_timesheet_revisions;",
            )
            .unwrap();
    }

    #[test]
    fn migration_24_to_25_preserves_undated_totals_without_backfill() {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        connection
            .execute_batch(
                "DROP TABLE payroll_timesheet_annual_leave;
             UPDATE schema_version SET version = 24;
             INSERT INTO payroll_timesheet_weeks
             (payroll_timesheet_id, week_number, week_commencing, annual_leave_hours)
             VALUES (1, 1, '10/08/2026', 7.25), (1, 2, '17/08/2026', 0);",
            )
            .unwrap();
        create_schema(&connection).unwrap();
        create_schema(&connection).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT version FROM schema_version", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            25
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT annual_leave_hours FROM payroll_timesheet_weeks WHERE week_number = 1",
                    [],
                    |row| row.get::<_, f64>(0)
                )
                .unwrap(),
            7.25
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM payroll_timesheet_annual_leave",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
    }

    #[test]
    fn fresh_schema_25_annual_leave_constraints() {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        let sql = "INSERT INTO payroll_timesheet_annual_leave
            (payroll_timesheet_id, week_number, leave_date, hours, created_at, updated_at)
            VALUES (1, ?1, '10/08/2026', ?2, 'created', 'updated')";
        connection.execute(sql, rusqlite::params![1, 2.0]).unwrap();
        assert!(connection.execute(sql, rusqlite::params![1, 3.0]).is_err());
        for (week, hours) in [
            (0, 1.0),
            (5, 1.0),
            (2, -1.0),
            (2, f64::NAN),
            (2, f64::INFINITY),
        ] {
            assert!(connection
                .execute(sql, rusqlite::params![week, hours])
                .is_err());
        }
        assert!(connection
            .execute(
                "UPDATE payroll_timesheet_annual_leave SET leave_date = NULL",
                []
            )
            .is_err());
    }

    #[test]
    fn migration_23_to_24_preserves_history_and_defaults_to_contracted() {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        connection
            .execute_batch(
                "DROP TABLE payroll_timesheet_annual_leave;
             ALTER TABLE personal_assistant_contracted_hours DROP COLUMN hours_basis;
             UPDATE schema_version SET version = 23;
             INSERT INTO personal_assistant_contracted_hours
                 (id, personal_assistant_id, effective_date, contracted_hours, created_at)
             VALUES (7, 1, '01/04/2026', '16.50', 'original'),
                    (9, 1, '01/04/2026', 'legacy text', 'later'),
                    (12, 2, '01/05/2026', '', 'empty');",
            )
            .unwrap();
        create_schema(&connection).unwrap();
        create_schema(&connection).unwrap(); // Already-current schema is unchanged.
        let mut statement = connection.prepare(
            "SELECT id, personal_assistant_id, effective_date, contracted_hours, created_at, hours_basis
             FROM personal_assistant_contracted_hours ORDER BY id"
        ).unwrap();
        let rows: Vec<(i64, i64, String, String, String, String)> = statement
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })
            .unwrap()
            .collect::<Result<_>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![
                (
                    7,
                    1,
                    "01/04/2026".into(),
                    "16.50".into(),
                    "original".into(),
                    "contracted".into()
                ),
                (
                    9,
                    1,
                    "01/04/2026".into(),
                    "legacy text".into(),
                    "later".into(),
                    "contracted".into()
                ),
                (
                    12,
                    2,
                    "01/05/2026".into(),
                    "".into(),
                    "empty".into(),
                    "contracted".into()
                ),
            ]
        );
        assert_eq!(
            connection
                .query_row("SELECT version FROM schema_version", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            25
        );
    }

    #[test]
    fn fresh_schema_24_accepts_only_explicit_supported_bases() {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        for basis in ["contracted", "variable"] {
            connection
                .execute(
                    "INSERT INTO personal_assistant_contracted_hours
                (personal_assistant_id, effective_date, contracted_hours, created_at, hours_basis)
                VALUES (1, '01/04/2026', '', 'created', ?1)",
                    [basis],
                )
                .unwrap();
        }
        assert!(connection
            .execute(
                "UPDATE personal_assistant_contracted_hours SET hours_basis = 'unknown'",
                []
            )
            .is_err());
        assert!(connection
            .execute(
                "UPDATE personal_assistant_contracted_hours SET hours_basis = NULL",
                []
            )
            .is_err());
        assert_eq!(
            connection
                .query_row("SELECT version FROM schema_version", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            25
        );
    }

    #[test]
    fn migration_to_version_18_preserves_legacy_statuses_as_timesheets() {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        drop_schema_22(&connection);

        connection
            .execute("DROP TABLE payroll_timesheet_email_status", [])
            .unwrap();
        connection
            .execute_batch(
                "
                CREATE TABLE payroll_timesheet_email_status (
                    id INTEGER PRIMARY KEY,
                    personal_assistant_id INTEGER NOT NULL,
                    payroll_year TEXT NOT NULL,
                    cycle_number INTEGER NOT NULL,
                    sent_at TEXT,
                    UNIQUE (personal_assistant_id, payroll_year, cycle_number)
                );
                INSERT INTO payroll_timesheet_email_status (
                    personal_assistant_id,
                    payroll_year,
                    cycle_number,
                    sent_at
                ) VALUES (1, '2026/27', 1, '2026-04-01T10:00:00Z');
                DROP TABLE payroll_timesheet_manual_adjustments;
                DROP TABLE payroll_timesheet_worked_item_snapshots;
                DROP TABLE payroll_timesheet_snapshot_states;
                DROP TABLE direct_shifts;
                DROP TABLE direct_shift_audit;
                DROP TABLE timesheet_correction_events;
                UPDATE schema_version SET version = 17;
                ",
            )
            .unwrap();

        create_schema(&connection).unwrap();

        let email_type: String = connection
            .query_row(
                "SELECT email_type FROM payroll_timesheet_email_status WHERE personal_assistant_id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let version: i64 = connection
            .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .unwrap();

        assert_eq!(email_type, "timesheet");
        assert_eq!(version, 25);
    }

    #[test]
    fn migration_to_version_20_adds_direct_shift_evidence_storage() {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();

        connection
            .execute(
                "INSERT INTO direct_shifts (
                    personal_assistant_id, start_time, end_time, break_minutes,
                    notes, source_type, created_at, updated_at
                 ) VALUES (1, '2026-09-01T09:07', NULL, 0, NULL, 'direct', 'created', 'updated')",
                [],
            )
            .unwrap();
        let duplicate = connection.execute(
            "INSERT INTO direct_shifts (
                personal_assistant_id, start_time, end_time, break_minutes,
                notes, source_type, created_at, updated_at
             ) VALUES (1, '2026-09-01T10:08', NULL, 0, NULL, 'direct', 'created', 'updated')",
            [],
        );
        let version: i64 = connection
            .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .unwrap();

        assert!(duplicate.is_err());
        assert_eq!(version, 25);
    }

    #[test]
    fn migration_to_version_21_adds_correction_history_without_changing_imported_rows() {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        drop_schema_22(&connection);
        connection
            .execute(
                "INSERT INTO timesheets (
                    id, pa_name, personal_assistant_id, start_time, end_time,
                    break_minutes, worked_minutes, hourly_rate, amount, notes
                 ) VALUES (
                    41, 'Alex Smith', 7, '1 September 2026 at 09:00:00',
                    '1 September 2026 at 10:00:00', 0, 60, 12.0, 12.0, 'source'
                 )",
                [],
            )
            .unwrap();
        connection
            .execute_batch(
                "DROP TABLE timesheet_correction_events;
                 UPDATE schema_version SET version = 20;",
            )
            .unwrap();

        create_schema(&connection).unwrap();

        let preserved: (i64, String, i64, String) = connection
            .query_row(
                "SELECT id, start_time, worked_minutes, notes FROM timesheets WHERE id = 41",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            preserved,
            (
                41,
                "1 September 2026 at 09:00:00".to_string(),
                60,
                "source".to_string()
            )
        );
        assert!(table_exists(&connection, "timesheet_correction_events").unwrap());
        assert_eq!(
            connection
                .query_row("SELECT version FROM schema_version", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            25
        );
    }

    #[test]
    fn final_schema_19_migrates_directly_to_final_schema_20() {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        drop_schema_22(&connection);
        connection
            .execute_batch(
                "DROP TABLE direct_shift_audit;
                 DROP TABLE direct_shifts;
                 DROP TABLE timesheet_correction_events;
                 UPDATE schema_version SET version = 19;",
            )
            .unwrap();

        create_schema(&connection).unwrap();

        assert!(table_has_column(&connection, "direct_shifts", "deleted_at").unwrap());
        assert!(table_has_column(&connection, "direct_shifts", "deleted_by").unwrap());
        assert!(table_exists(&connection, "direct_shift_audit").unwrap());
        assert_eq!(
            connection
                .query_row("SELECT version FROM schema_version", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            25
        );
    }

    #[test]
    fn earlier_development_schema_20_is_repaired_idempotently_without_data_loss() {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        drop_schema_22(&connection);
        connection
            .execute_batch(
                "DROP TABLE direct_shift_audit;
                 DROP TABLE direct_shifts;
                 DROP TABLE timesheet_correction_events;
                 CREATE TABLE direct_shifts (
                    id INTEGER PRIMARY KEY,
                    personal_assistant_id INTEGER NOT NULL,
                    start_time TEXT NOT NULL,
                    end_time TEXT,
                    break_minutes INTEGER NOT NULL DEFAULT 0 CHECK (break_minutes >= 0),
                    notes TEXT,
                    source_type TEXT NOT NULL DEFAULT 'direct' CHECK (source_type = 'direct'),
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    CHECK (end_time IS NULL OR end_time >= start_time)
                 );
                 CREATE UNIQUE INDEX one_running_direct_shift_per_pa
                 ON direct_shifts (personal_assistant_id) WHERE end_time IS NULL;
                 INSERT INTO direct_shifts (
                    id, personal_assistant_id, start_time, end_time, break_minutes,
                    notes, source_type, created_at, updated_at
                 ) VALUES (
                    42, 7, '2026-09-01T09:07', '2026-09-01T10:19', 12,
                    'existing test shift', 'direct', 'created', 'updated'
                 );
                 UPDATE schema_version SET version = 20;",
            )
            .unwrap();

        create_schema(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO direct_shift_audit (
                    direct_shift_id, actor_id, action_type, action_at
                 ) VALUES (42, 'local_employer', 'edit', 'existing-audit')",
                [],
            )
            .unwrap();
        create_schema(&connection).unwrap();

        let preserved: (i64, i64, String, String, i64, String) = connection
            .query_row(
                "SELECT id, personal_assistant_id, start_time, end_time,
                        break_minutes, notes
                 FROM direct_shifts WHERE id = 42",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
        let audit_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM direct_shift_audit", [], |row| {
                row.get(0)
            })
            .unwrap();

        assert_eq!(
            preserved,
            (
                42,
                7,
                "2026-09-01T09:07".to_string(),
                "2026-09-01T10:19".to_string(),
                12,
                "existing test shift".to_string()
            )
        );
        assert!(table_has_column(&connection, "direct_shifts", "deleted_at").unwrap());
        assert!(table_has_column(&connection, "direct_shifts", "deleted_by").unwrap());
        assert_eq!(audit_count, 1);
    }

    #[test]
    fn migration_to_version_22_backfills_only_legacy_snapshots_with_exact_evidence() {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        drop_schema_22(&connection);
        connection
            .execute_batch(
                "INSERT INTO payroll_timesheets (
                    id, personal_assistant_id, payroll_year, cycle_number,
                    created_at, updated_at
                 ) VALUES
                    (101, 1, '2026/27', 6, 'created-1', 'updated-1'),
                    (102, 2, '2026/27', 6, 'created-2', 'updated-2'),
                    (103, 3, '2026/27', 6, 'created-3', 'updated-3');

                 INSERT INTO payroll_timesheet_weeks (
                    payroll_timesheet_id, week_number, week_commencing, worked_hours,
                    annual_leave_hours, sick_leave_hours, public_holiday_hours, travel_miles
                 ) VALUES
                    (101, 1, '10/08/2026', 7.25, 1.5, 2.5, 3.5, 4.5),
                    (102, 2, '17/08/2026', 8.25, 0.5, 1.5, 2.5, 5.5),
                    (103, 3, '24/08/2026', 9.25, 0.0, 0.0, 0.0, 6.5);

                 INSERT INTO payroll_timesheet_public_holidays (
                    payroll_timesheet_id, week_number, holiday_date, hours
                 ) VALUES
                    (101, 1, '10/08/2026', 3.5),
                    (102, 2, '17/08/2026', 2.5),
                    (103, 3, '24/08/2026', 1.5);

                 INSERT INTO payroll_timesheet_snapshot_states (
                    payroll_timesheet_id, state, pdf_path, pdf_sha256, generated_at,
                    submitted_at, indeterminate_at
                 ) VALUES
                    (101, 'submitted', '/legacy/shared.pdf', 'submitted-digest',
                     'generated-submitted', 'submitted-at', NULL),
                    (102, 'indeterminate', '/legacy/shared.pdf', 'indeterminate-digest',
                     'generated-indeterminate', NULL, 'indeterminate-at');

                 INSERT INTO payroll_timesheet_email_status (
                    personal_assistant_id, payroll_year, cycle_number, email_type, sent_at
                 ) VALUES (1, '2026/27', 6, 'timesheet', 'email-sent-at');

                 INSERT INTO payroll_timesheet_worked_item_snapshots (
                    payroll_timesheet_id, week_number, source_type, timesheet_id,
                    work_date, worked_minutes, pay_rate_id, pay_rate_effective_date,
                    total_hourly_rate, reason, captured_at
                 ) VALUES
                    (101, 1, 'imported_shift', 501, '2026-08-10', 435,
                     31, '01/04/2026', 14.5, NULL, 'captured-1'),
                    (102, 2, 'manual_adjustment', NULL, '2026-08-17', -30,
                     32, '01/04/2026', 15.5, 'legacy reason', 'captured-2');

                 UPDATE schema_version SET version = 21;",
            )
            .unwrap();

        migrate_to_version_22(&connection).unwrap();

        let revisions = connection
            .prepare(
                "SELECT payroll_timesheet_id, revision_number, state, pdf_path,
                        pdf_sha256, generated_at, send_attempted_at, submitted_at,
                        indeterminate_at, legacy_backfilled
                 FROM payroll_timesheet_revisions ORDER BY payroll_timesheet_id",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, i64>(9)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>>>()
            .unwrap();
        assert_eq!(revisions.len(), 2);
        assert_eq!(
            revisions[0],
            (
                101,
                1,
                "submitted".to_string(),
                "/legacy/shared.pdf".to_string(),
                "submitted-digest".to_string(),
                "generated-submitted".to_string(),
                None,
                Some("submitted-at".to_string()),
                None,
                1,
            )
        );
        assert_eq!(revisions[1].0, 102);
        assert_eq!(revisions[1].2, "indeterminate");
        assert_eq!(revisions[1].3, "/legacy/shared.pdf");
        assert_eq!(revisions[1].4, "indeterminate-digest");
        assert_eq!(revisions[1].5, "generated-indeterminate");
        assert_eq!(revisions[1].6.as_deref(), Some("indeterminate-at"));
        assert_eq!(revisions[1].7, None);
        assert_eq!(revisions[1].8.as_deref(), Some("indeterminate-at"));
        assert_eq!(revisions[1].9, 1);

        let unsnapshotted_revisions: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM payroll_timesheet_revisions
                 WHERE payroll_timesheet_id = 103",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(unsnapshotted_revisions, 0);

        let mapped_items: Vec<(i64, String, Option<i64>, i64, Option<String>)> = connection
            .prepare(
                "SELECT revision.payroll_timesheet_id, item.source_type,
                        item.timesheet_id, item.worked_minutes, item.reason
                 FROM payroll_timesheet_revision_worked_items AS item
                 INNER JOIN payroll_timesheet_revisions AS revision
                    ON revision.id = item.payroll_timesheet_revision_id
                 ORDER BY revision.payroll_timesheet_id",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .unwrap()
            .collect::<Result<_>>()
            .unwrap();
        assert_eq!(mapped_items.len(), 2);
        assert_eq!(
            mapped_items[0],
            (101, "imported_shift".to_string(), Some(501), 435, None)
        );
        assert_eq!(mapped_items[1].0, 102);
        assert_eq!(mapped_items[1].4.as_deref(), Some("legacy reason"));

        let frozen_weeks: Vec<(i64, i64, f64, f64, f64, f64, f64)> = connection
            .prepare(
                "SELECT revision.payroll_timesheet_id, week.week_number,
                        week.worked_hours, week.annual_leave_hours, week.sick_leave_hours,
                        week.public_holiday_hours, week.travel_miles
                 FROM payroll_timesheet_revision_weeks AS week
                 INNER JOIN payroll_timesheet_revisions AS revision
                    ON revision.id = week.payroll_timesheet_revision_id
                 ORDER BY revision.payroll_timesheet_id",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            })
            .unwrap()
            .collect::<Result<_>>()
            .unwrap();
        assert_eq!(frozen_weeks.len(), 2);
        assert_eq!(frozen_weeks[0], (101, 1, 7.25, 1.5, 2.5, 3.5, 4.5));
        assert_eq!(frozen_weeks[1], (102, 2, 8.25, 0.5, 1.5, 2.5, 5.5));

        let frozen_holidays: Vec<(i64, String, f64)> = connection
            .prepare(
                "SELECT revision.payroll_timesheet_id, holiday.holiday_date, holiday.hours
                 FROM payroll_timesheet_revision_public_holidays AS holiday
                 INNER JOIN payroll_timesheet_revisions AS revision
                    ON revision.id = holiday.payroll_timesheet_revision_id
                 ORDER BY revision.payroll_timesheet_id",
            )
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<Result<_>>()
            .unwrap();
        assert_eq!(
            frozen_holidays,
            vec![
                (101, "10/08/2026".to_string(), 3.5),
                (102, "17/08/2026".to_string(), 2.5)
            ]
        );

        let legacy_state_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM payroll_timesheet_snapshot_states",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let email_status: (String, String) = connection
            .query_row("SELECT email_type, sent_at FROM payroll_timesheet_email_status WHERE personal_assistant_id = 1 AND payroll_year = '2026/27' AND cycle_number = 6", [], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap();
        assert_eq!(legacy_state_count, 2);
        assert_eq!(
            email_status,
            ("timesheet".to_string(), "email-sent-at".to_string())
        );
        assert_eq!(
            connection
                .query_row("SELECT version FROM schema_version", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            22
        );
    }

    #[test]
    fn migration_to_version_22_rolls_back_all_new_tables_on_failure() {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        drop_schema_22(&connection);
        connection
            .execute_batch(
                "CREATE TABLE payroll_timesheet_revision_weeks (injected INTEGER);
                 UPDATE schema_version SET version = 21;",
            )
            .unwrap();

        assert!(migrate_to_version_22(&connection).is_err());
        assert!(!table_exists(&connection, "payroll_timesheet_revisions").unwrap());
        assert!(!table_exists(&connection, "payroll_timesheet_revision_worked_items").unwrap());
        assert!(table_exists(&connection, "payroll_timesheet_revision_weeks").unwrap());
        assert_eq!(
            connection
                .query_row("SELECT version FROM schema_version", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            21
        );
    }

    #[test]
    fn migration_to_version_23_removes_only_revision_infrastructure() {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO payroll_timesheets
             (id, personal_assistant_id, payroll_year, cycle_number, created_at, updated_at)
             VALUES (77, 7, '2026/27', 6, 'created', 'updated')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO payroll_timesheet_snapshot_states
             (payroll_timesheet_id, state, pdf_path, pdf_sha256, generated_at)
             VALUES (77, 'candidate', '/legacy.pdf', 'digest', 'generated')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "ALTER TABLE personal_assistant_contracted_hours DROP COLUMN hours_basis",
                [],
            )
            .unwrap();
        connection
            .execute("DROP TABLE payroll_timesheet_annual_leave", [])
            .unwrap();
        migrate_to_version_22(&connection).unwrap();
        assert!(table_exists(&connection, "payroll_timesheet_revisions").unwrap());

        create_schema(&connection).unwrap();

        for table in [
            "payroll_timesheet_revisions",
            "payroll_timesheet_revision_worked_items",
            "payroll_timesheet_revision_weeks",
            "payroll_timesheet_revision_public_holidays",
            "payroll_timesheet_revision_delivery_attempts",
        ] {
            assert!(
                !table_exists(&connection, table).unwrap(),
                "{table} remains"
            );
        }
        assert!(table_exists(&connection, "timesheet_correction_events").unwrap());
        assert!(table_exists(&connection, "direct_shifts").unwrap());
        assert!(table_exists(&connection, "direct_shift_audit").unwrap());
        assert!(table_exists(&connection, "payroll_timesheet_worked_item_snapshots").unwrap());
        assert!(table_exists(&connection, "payroll_timesheet_snapshot_states").unwrap());
        let legacy: (String, String) = connection
            .query_row(
                "SELECT pdf_path, pdf_sha256 FROM payroll_timesheet_snapshot_states
             WHERE payroll_timesheet_id = 77",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(legacy, ("/legacy.pdf".to_string(), "digest".to_string()));
        assert_eq!(
            connection
                .query_row("SELECT version FROM schema_version", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            25
        );
    }

    #[test]
    fn fresh_schema_24_never_retains_revision_tables() {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT version FROM schema_version", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            25
        );
        assert!(!table_exists(&connection, "payroll_timesheet_revisions").unwrap());
        assert!(table_exists(&connection, "timesheet_correction_events").unwrap());
    }
}
