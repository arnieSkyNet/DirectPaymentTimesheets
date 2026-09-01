use std::path::Path;

use rusqlite::{Connection, Result};

pub const CURRENT_SCHEMA_VERSION: i64 = 20;

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

    apply_migrations(connection)?;
    repair_unreleased_schema_20(connection)?;

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

    #[test]
    fn migration_to_version_18_preserves_legacy_statuses_as_timesheets() {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();

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
        assert_eq!(version, 20);
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
        assert_eq!(version, 20);
    }

    #[test]
    fn final_schema_19_migrates_directly_to_final_schema_20() {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        connection
            .execute_batch(
                "DROP TABLE direct_shift_audit;
                 DROP TABLE direct_shifts;
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
            20
        );
    }

    #[test]
    fn earlier_development_schema_20_is_repaired_idempotently_without_data_loss() {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        connection
            .execute_batch(
                "DROP TABLE direct_shift_audit;
                 DROP TABLE direct_shifts;
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
                 );",
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
}
