//! Date-only sickness storage. Dates are inclusive and independent of payroll weeks.
//! Payroll is responsible for all SSP calculations.
// Public storage API is wired into sickness entry in a later stage.
#![allow(dead_code)]

use rusqlite::{params, Connection, OptionalExtension, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SicknessPeriod {
    pub id: i64,
    pub personal_assistant_id: i64,
    /// Canonical YYYY-MM-DD, inclusive.
    pub start_date: String,
    /// Canonical YYYY-MM-DD, inclusive.
    pub end_date: String,
}

pub struct SicknessPeriodRepository {
    connection: Connection,
}

impl SicknessPeriodRepository {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    /// Accept project calendar inputs, validating and normalizing them before storage.
    pub fn insert(&self, pa_id: i64, start_date: &str, end_date: &str) -> Result<i64> {
        let (start, end) = date_range(start_date, end_date)?;
        // Connections in this project do not uniformly enable foreign keys.
        let changed = self.connection.execute(
            "INSERT INTO personal_assistant_sickness_periods
                (personal_assistant_id, start_date, end_date)
             SELECT id, ?2, ?3 FROM personal_assistants WHERE id = ?1",
            params![pa_id, start, end],
        )?;
        if changed == 0 {
            return Err(crate::date_utils::sql_error(
                "Personal Assistant not found".into(),
            ));
        }
        Ok(self.connection.last_insert_rowid())
    }

    pub fn get_by_id(&self, id: i64) -> Result<Option<SicknessPeriod>> {
        self.connection
            .query_row(
                "SELECT id, personal_assistant_id, start_date, end_date
             FROM personal_assistant_sickness_periods WHERE id = ?1",
                [id],
                from_row,
            )
            .optional()
    }

    pub fn get_for_pa(&self, pa_id: i64) -> Result<Vec<SicknessPeriod>> {
        let mut statement = self.connection.prepare(
            "SELECT id, personal_assistant_id, start_date, end_date
             FROM personal_assistant_sickness_periods WHERE personal_assistant_id = ?1
             ORDER BY start_date, end_date, id",
        )?;
        let rows = statement.query_map([pa_id], from_row)?;
        rows.collect()
    }

    /// Return full periods overlapping either boundary or any part of the inclusive
    /// query range. Periods are not clipped or split into payroll weeks.
    pub fn get_overlapping_for_pa(
        &self,
        pa_id: i64,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<SicknessPeriod>> {
        let (start, end) = date_range(start_date, end_date)?;
        let mut statement = self.connection.prepare(
            "SELECT id, personal_assistant_id, start_date, end_date
             FROM personal_assistant_sickness_periods
             WHERE personal_assistant_id = ?1 AND start_date <= ?3 AND end_date >= ?2
             ORDER BY start_date, end_date, id",
        )?;
        let rows = statement.query_map(params![pa_id, start, end], from_row)?;
        rows.collect()
    }

    /// Update dates without changing the PA owner; false means the record was absent.
    pub fn update(&self, id: i64, start_date: &str, end_date: &str) -> Result<bool> {
        let (start, end) = date_range(start_date, end_date)?;
        Ok(self.connection.execute(
            "UPDATE personal_assistant_sickness_periods SET start_date = ?2, end_date = ?3
             WHERE id = ?1",
            params![id, start, end],
        )? == 1)
    }

    /// False means the record was already absent.
    pub fn delete(&self, id: i64) -> Result<bool> {
        Ok(self.connection.execute(
            "DELETE FROM personal_assistant_sickness_periods WHERE id = ?1",
            [id],
        )? == 1)
    }
}

fn date_range(start: &str, end: &str) -> Result<(String, String)> {
    let start = crate::date_utils::parse_input(start).map_err(crate::date_utils::sql_error)?;
    let end = crate::date_utils::parse_input(end).map_err(crate::date_utils::sql_error)?;
    if end < start {
        return Err(crate::date_utils::sql_error(
            "End date cannot precede start date".into(),
        ));
    }
    Ok((crate::date_utils::iso(start), crate::date_utils::iso(end)))
}

fn from_row(row: &rusqlite::Row<'_>) -> Result<SicknessPeriod> {
    Ok(SicknessPeriod {
        id: row.get(0)?,
        personal_assistant_id: row.get(1)?,
        start_date: row.get(2)?,
        end_date: row.get(3)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository() -> SicknessPeriodRepository {
        let connection = Connection::open_in_memory().unwrap();
        crate::database::create_schema(&connection).unwrap();
        connection.execute_batch("INSERT INTO personal_assistants (id, first_name, surname) VALUES (1, 'One', 'PA'), (2, 'Two', 'PA');").unwrap();
        SicknessPeriodRepository::new(connection)
    }

    #[test]
    fn crud_normalizes_dates_and_supports_single_day_and_multiple_periods() {
        let r = repository();
        let id = r.insert(1, "03/09/2026", "5 Sep 2026").unwrap();
        let second = r.insert(1, "2026-09-10", "2026-09-10").unwrap();
        let stored = r.get_by_id(id).unwrap().unwrap();
        assert_eq!(stored.start_date, "2026-09-03");
        assert_eq!(stored.end_date, "2026-09-05");
        assert_eq!(stored.personal_assistant_id, 1);
        assert_eq!(r.get_for_pa(1).unwrap().len(), 2);
        assert!(r.update(id, "2026-09-02", "2026-10-01").unwrap());
        assert_eq!(r.get_by_id(id).unwrap().unwrap().end_date, "2026-10-01");
        assert!(r.delete(id).unwrap());
        assert!(!r.delete(id).unwrap());
        assert!(!r.update(id, "2026-09-02", "2026-09-03").unwrap());
        assert!(r.get_by_id(id).unwrap().is_none());
        assert_eq!(r.get_for_pa(1).unwrap()[0].id, second);
    }

    #[test]
    fn overlap_is_inclusive_pa_scoped_and_returns_complete_periods_in_order() {
        let r = repository();
        let right = r.insert(1, "2026-09-28", "2026-10-10").unwrap();
        let inside = r.insert(1, "2026-09-09", "2026-09-09").unwrap();
        let left = r.insert(1, "2026-08-25", "2026-09-01").unwrap();
        let enclosing = r.insert(1, "2026-08-01", "2026-11-01").unwrap();
        r.insert(1, "2026-08-30", "2026-08-31").unwrap();
        r.insert(1, "2026-09-29", "2026-10-01").unwrap();
        r.insert(2, "2026-09-01", "2026-09-28").unwrap();
        let periods = r
            .get_overlapping_for_pa(1, "2026-09-01", "2026-09-28")
            .unwrap();
        assert_eq!(
            periods.iter().map(|p| p.id).collect::<Vec<_>>(),
            vec![enclosing, left, inside, right]
        );
        assert_eq!(periods[0].start_date, "2026-08-01");
        assert_eq!(periods[0].end_date, "2026-11-01");
        assert!(r
            .get_overlapping_for_pa(3, "2026-09-01", "2026-09-28")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn invalid_dates_ranges_and_missing_pa_do_not_write() {
        let r = repository();
        let id = r.insert(1, "2024-02-29", "2024-02-29").unwrap();
        let before = r.get_by_id(id).unwrap();
        for (start, end) in [
            ("2026-02-29", "2026-03-01"),
            ("", "2026-09-01"),
            ("2026-09-02", "2026-09-01"),
            ("2026-09-01", "bad"),
            ("(03/09/2026 to 05/09/2026)", "2026-09-05"),
        ] {
            assert!(r.insert(1, start, end).is_err());
            assert!(r.update(id, start, end).is_err());
            assert!(r.get_overlapping_for_pa(1, start, end).is_err());
        }
        assert!(r.insert(999, "2026-09-01", "2026-09-02").is_err());
        assert_eq!(r.get_by_id(id).unwrap(), before);
        assert_eq!(r.get_for_pa(1).unwrap().len(), 1);
    }

    #[test]
    fn migration_from_29_is_atomic_repeatable_and_records_survive_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sickness.sqlite");
        let connection = Connection::open(&path).unwrap();
        crate::database::create_schema(&connection).unwrap();
        connection.execute_batch("DROP TABLE personal_assistant_sickness_periods; UPDATE schema_version SET version = 29;
            INSERT INTO personal_assistants (id, first_name, surname, sick_pay_enabled) VALUES (1, 'Existing', 'PA', 0);
            CREATE TRIGGER refuse_version BEFORE UPDATE ON schema_version BEGIN SELECT RAISE(ABORT, 'blocked'); END;").unwrap();
        assert!(crate::database::create_schema(&connection).is_err());
        assert_eq!(
            connection
                .query_row::<i64, _, _>("SELECT version FROM schema_version", [], |r| r.get(0))
                .unwrap(),
            29
        );
        assert_eq!(connection.query_row::<i64, _, _>("SELECT count(*) FROM sqlite_master WHERE name = 'personal_assistant_sickness_periods'", [], |r| r.get(0)).unwrap(), 0);
        connection
            .execute_batch("DROP TRIGGER refuse_version;")
            .unwrap();
        crate::database::create_schema(&connection).unwrap();
        assert_eq!(
            connection
                .query_row::<i64, _, _>("SELECT version FROM schema_version", [], |r| r.get(0))
                .unwrap(),
            30
        );
        assert_eq!(
            connection
                .query_row::<i64, _, _>(
                    "SELECT sick_pay_enabled FROM personal_assistants WHERE id = 1",
                    [],
                    |r| r.get(0)
                )
                .unwrap(),
            0
        );
        let r = SicknessPeriodRepository::new(connection);
        let id = r.insert(1, "2026-09-01", "2026-10-10").unwrap();
        drop(r);
        let connection = Connection::open(path).unwrap();
        crate::database::create_schema(&connection).unwrap();
        let r = SicknessPeriodRepository::new(connection);
        assert_eq!(r.get_by_id(id).unwrap().unwrap().end_date, "2026-10-10");
    }
}
