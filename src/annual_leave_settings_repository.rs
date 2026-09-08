use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension, Result};

#[derive(Clone, Debug, PartialEq)]
pub struct AnnualLeaveSettings {
    pub contracted_effective_from: String,
    pub statutory_weeks: f64,
    pub variable_effective_from: String,
    pub accrual_percentage: f64,
}
impl Default for AnnualLeaveSettings {
    fn default() -> Self {
        Self {
            contracted_effective_from: "01/04".into(),
            statutory_weeks: 5.6,
            variable_effective_from: "01/04".into(),
            accrual_percentage: 12.07,
        }
    }
}
fn invalid(message: &str) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(message.into())
}

// Recurring boundaries must exist in every year. No year is stored or accepted.
pub fn normalise_boundary(input: &str) -> Result<String> {
    let parts: Vec<_> = input.trim().split('/').collect();
    if parts.len() != 2 {
        return Err(invalid(
            "Effective from must be a recurring day/month (DD/MM), without a year.",
        ));
    }
    parts[0]
        .parse()
        .ok()
        .zip(parts[1].parse().ok())
        .and_then(|(day, month)| NaiveDate::from_ymd_opt(2025, month, day))
        .map(|date| date.format("%d/%m").to_string())
        .ok_or_else(|| {
            invalid("Effective from must exist every year (DD/MM); 29/02 is not supported.")
        })
}
impl AnnualLeaveSettings {
    pub fn validated(&self) -> Result<Self> {
        let mut settings = self.clone();
        settings.contracted_effective_from = normalise_boundary(&self.contracted_effective_from)?;
        settings.variable_effective_from = normalise_boundary(&self.variable_effective_from)?;
        if !self.statutory_weeks.is_finite() || !(0.0..=52.0).contains(&self.statutory_weeks) {
            return Err(invalid(
                "Statutory annual-leave weeks must be finite and between 0 and 52.",
            ));
        }
        if !self.accrual_percentage.is_finite() || !(0.0..=100.0).contains(&self.accrual_percentage)
        {
            return Err(invalid(
                "Accrual percentage must be finite and between 0 and 100.",
            ));
        }
        Ok(settings)
    }
}

pub struct AnnualLeaveSettingsRepository {
    connection: Connection,
}
impl AnnualLeaveSettingsRepository {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }
    pub fn get(&self) -> Result<AnnualLeaveSettings> {
        self.connection.query_row("SELECT contracted_effective_from, statutory_weeks, variable_effective_from, accrual_percentage FROM annual_leave_settings WHERE id=1", [], |row| Ok(AnnualLeaveSettings {
            contracted_effective_from: row.get(0)?, statutory_weeks: row.get(1)?, variable_effective_from: row.get(2)?, accrual_percentage: row.get(3)?,
        })).optional().map(|settings| settings.unwrap_or_default())
    }
    pub fn save(&self, settings: &AnnualLeaveSettings) -> Result<AnnualLeaveSettings> {
        let settings = settings.validated()?;
        self.connection.execute("INSERT INTO annual_leave_settings (id, contracted_effective_from, statutory_weeks, variable_effective_from, accrual_percentage)
            VALUES (1,?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET contracted_effective_from=excluded.contracted_effective_from,
            statutory_weeks=excluded.statutory_weeks, variable_effective_from=excluded.variable_effective_from, accrual_percentage=excluded.accrual_percentage",
            params![settings.contracted_effective_from, settings.statutory_weeks, settings.variable_effective_from, settings.accrual_percentage])?;
        Ok(settings)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recurring_boundaries_accept_other_days_without_years() {
        for (input, expected) in [
            ("1/4", "01/04"),
            ("15/9", "15/09"),
            ("31/12", "31/12"),
            ("28/2", "28/02"),
        ] {
            assert_eq!(normalise_boundary(input).unwrap(), expected);
        }
        for input in [
            "01/04/2026",
            "2026-04-01",
            "29/02",
            "31/04",
            "00/01",
            "1/13",
            "bad",
        ] {
            assert!(normalise_boundary(input).is_err());
        }
    }
    #[test]
    fn absent_row_returns_operational_defaults_without_ui_or_save() {
        for upgrading in [false, true] {
            let connection = Connection::open_in_memory().unwrap();
            crate::database::create_schema(&connection).unwrap();
            if upgrading {
                connection
                    .execute_batch(
                        "ALTER TABLE personal_assistants DROP COLUMN leaving_date; DROP TABLE annual_leave_settings; UPDATE schema_version SET version = 25;",
                    )
                    .unwrap();
                crate::database::create_schema(&connection).unwrap();
            }
            let repository = AnnualLeaveSettingsRepository::new(connection);
            // Read directly through the application-facing API, without any UI
            // construction or save. Pin the actual values, not Default itself.
            for _ in 0..2 {
                let settings = repository.get().unwrap();
                assert_eq!(settings.contracted_effective_from, "01/04");
                assert_eq!(settings.statutory_weeks, 5.6);
                assert_eq!(settings.variable_effective_from, "01/04");
                assert_eq!(settings.accrual_percentage, 12.07);
            }
            assert_eq!(
                repository
                    .connection
                    .query_row("SELECT COUNT(*) FROM annual_leave_settings", [], |row| row
                        .get::<_, i64>(
                        0
                    ))
                    .unwrap(),
                0
            );
        }
    }

    #[test]
    fn defaults_and_atomic_four_value_save() {
        let connection = Connection::open_in_memory().unwrap();
        crate::database::create_schema(&connection).unwrap();
        let repository = AnnualLeaveSettingsRepository::new(connection);
        assert_eq!(repository.get().unwrap(), AnnualLeaveSettings::default());
        assert_eq!(
            repository
                .connection
                .query_row("SELECT COUNT(*) FROM annual_leave_settings", [], |row| row
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            0
        );
        let settings = AnnualLeaveSettings {
            contracted_effective_from: "1/1".into(),
            statutory_weeks: 6.0,
            variable_effective_from: "15/9".into(),
            accrual_percentage: 13.0,
        };
        let saved = repository.save(&settings).unwrap();
        assert_eq!(saved.contracted_effective_from, "01/01");
        assert_eq!(saved.variable_effective_from, "15/09");
        assert_eq!(repository.get().unwrap(), saved);
        for value in [-1.0, f64::NAN, f64::INFINITY, 101.0] {
            let mut bad = saved.clone();
            bad.statutory_weeks = value;
            assert!(repository.save(&bad).is_err());
            bad = saved.clone();
            bad.accrual_percentage = value;
            assert!(repository.save(&bad).is_err());
            assert_eq!(repository.get().unwrap(), saved);
        }
        repository.connection.execute_batch("CREATE TRIGGER refuse_settings BEFORE UPDATE ON annual_leave_settings BEGIN SELECT RAISE(ABORT,'blocked'); END;").unwrap();
        assert!(repository.save(&AnnualLeaveSettings::default()).is_err());
        assert_eq!(repository.get().unwrap(), saved);
    }
}
