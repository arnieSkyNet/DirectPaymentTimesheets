use chrono::NaiveDate;
use rusqlite::{params, Connection, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HoursBasis {
    #[default]
    Contracted,
    Variable,
}

impl HoursBasis {
    pub fn label(self) -> &'static str {
        match self {
            Self::Contracted => "Contracted",
            Self::Variable => "Variable",
        }
    }

    fn stored(self) -> &'static str {
        match self {
            Self::Contracted => "contracted",
            Self::Variable => "variable",
        }
    }
}

impl rusqlite::types::FromSql for HoursBasis {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match value.as_str()? {
            "contracted" => Ok(Self::Contracted),
            "variable" => Ok(Self::Variable),
            _ => Err(rusqlite::types::FromSqlError::Other(Box::new(
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Unknown contracted-hours basis",
                ),
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContractedHoursEntry {
    pub id: i64,
    pub personal_assistant_id: i64,
    pub effective_date: String,
    pub contracted_hours: String,
    pub hours_basis: HoursBasis,
    pub created_at: String,
}

impl ContractedHoursEntry {
    pub fn summary_value(&self) -> &str {
        match self.hours_basis {
            HoursBasis::Contracted => &self.contracted_hours,
            HoursBasis::Variable => "Variable",
        }
    }

    fn hours_to_store(&self) -> Result<&str> {
        match self.hours_basis {
            HoursBasis::Variable => Ok(""),
            HoursBasis::Contracted if self.contracted_hours.trim().is_empty() => Err(
                rusqlite::Error::InvalidParameterName("Weekly contracted hours is required".into()),
            ),
            HoursBasis::Contracted => Ok(&self.contracted_hours),
        }
    }
}

pub struct ContractedHoursRepository {
    connection: Connection,
}

impl ContractedHoursRepository {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    pub fn insert(&self, entry: &ContractedHoursEntry) -> Result<()> {
        let effective_date = crate::date_utils::edited(&entry.effective_date, None, false)?;

        self.connection.execute(
            "
            INSERT INTO personal_assistant_contracted_hours (
                personal_assistant_id,
                effective_date,
                contracted_hours,
                created_at,
                hours_basis
            )
            VALUES (?1, ?2, ?3, ?4, ?5)
            ",
            params![
                entry.personal_assistant_id,
                effective_date,
                entry.hours_to_store()?,
                entry.created_at,
                entry.hours_basis.stored(),
            ],
        )?;

        Ok(())
    }

    pub fn update(&self, entry: &ContractedHoursEntry) -> Result<()> {
        let previous: String = self.connection.query_row(
            "SELECT effective_date FROM personal_assistant_contracted_hours WHERE id=?1",
            [entry.id],
            |row| row.get(0),
        )?;
        let effective_date =
            crate::date_utils::edited(&entry.effective_date, Some(&previous), false)?;

        self.connection.execute(
            "
            UPDATE personal_assistant_contracted_hours
            SET
                effective_date = ?1,
                contracted_hours = ?2,
                hours_basis = ?4
            WHERE id = ?3
            ",
            params![
                effective_date,
                entry.hours_to_store()?,
                entry.id,
                entry.hours_basis.stored()
            ],
        )?;

        Ok(())
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        self.connection.execute(
            "
            DELETE FROM personal_assistant_contracted_hours
            WHERE id = ?1
            ",
            params![id],
        )?;

        Ok(())
    }

    pub fn get_all_for_personal_assistant(
        &self,
        personal_assistant_id: i64,
    ) -> Result<Vec<ContractedHoursEntry>> {
        let mut statement = self.connection.prepare(
            "
            SELECT
                id,
                personal_assistant_id,
                effective_date,
                contracted_hours,
                created_at,
                hours_basis
            FROM personal_assistant_contracted_hours
            WHERE personal_assistant_id = ?1
            ORDER BY id DESC",
        )?;

        let entries = statement.query_map(params![personal_assistant_id], |row| {
            Ok(ContractedHoursEntry {
                id: row.get(0)?,
                personal_assistant_id: row.get(1)?,
                effective_date: row.get(2)?,
                contracted_hours: row.get(3)?,
                created_at: row.get(4)?,
                hours_basis: row.get(5)?,
            })
        })?;

        let mut results = Vec::new();

        for entry in entries {
            results.push(entry?);
        }

        results.sort_by_key(|row| {
            std::cmp::Reverse((
                crate::date_utils::parse_legacy(&row.effective_date).ok(),
                row.id,
            ))
        });
        Ok(results)
    }
    #[allow(dead_code)]
    pub fn get_current_for_personal_assistant(
        &self,
        personal_assistant_id: i64,
    ) -> Result<Option<ContractedHoursEntry>> {
        let mut hours = self.get_all_for_personal_assistant(personal_assistant_id)?;

        if hours.is_empty() {
            Ok(None)
        } else {
            Ok(Some(hours.remove(0)))
        }
    }

    pub fn get_for_personal_assistant_as_of(
        &self,
        personal_assistant_id: i64,
        as_of_date: NaiveDate,
    ) -> Result<Option<ContractedHoursEntry>> {
        let rows = self.get_all_for_personal_assistant(personal_assistant_id)?;
        // Invalid legacy history is visible in maintenance, never guessed in payroll.
        for row in &rows {
            crate::date_utils::parse_legacy(&row.effective_date)
                .map_err(crate::date_utils::sql_error)?;
        }
        Ok(rows.into_iter().find(|row| {
            crate::date_utils::parse_legacy(&row.effective_date)
                .is_ok_and(|date| date <= as_of_date)
        }))
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn mixed_calendar_history_uses_typed_dates_and_preserves_legacy_bytes() {
        let repo = repository();
        for date in ["31/12/2025", "2026-01-02", "02/01/2026", "1 February 2026"] {
            repo.connection.execute("INSERT INTO personal_assistant_contracted_hours(personal_assistant_id,effective_date,contracted_hours,created_at,hours_basis) VALUES(1,?1,'10','legacy','contracted')", [date]).unwrap();
        }
        assert_eq!(
            repo.get_all_for_personal_assistant(1)
                .unwrap()
                .iter()
                .map(|r| r.id)
                .collect::<Vec<_>>(),
            vec![4, 3, 2, 1]
        );
        assert_eq!(
            repo.get_for_personal_assistant_as_of(1, date("2026-01-15"))
                .unwrap()
                .unwrap()
                .id,
            3
        );
        let mut row = repo.get_all_for_personal_assistant(1).unwrap().remove(0);
        row.effective_date = "2026-02-01".into();
        repo.update(&row).unwrap();
        assert_eq!(
            repo.get_all_for_personal_assistant(1).unwrap()[0].effective_date,
            "1 February 2026"
        );
        row.effective_date = "31 February 2026".into();
        assert!(repo.update(&row).is_err());
    }

    use super::*;
    use crate::database::create_schema;

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn repository() -> ContractedHoursRepository {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        ContractedHoursRepository::new(connection)
    }

    fn insert(repository: &ContractedHoursRepository, effective_date: &str, hours: &str) {
        repository
            .insert(&ContractedHoursEntry {
                id: 0,
                personal_assistant_id: 1,
                effective_date: effective_date.to_string(),
                contracted_hours: hours.to_string(),
                hours_basis: HoursBasis::Contracted,
                created_at: "2026-01-01".to_string(),
            })
            .unwrap();
    }

    #[test]
    fn both_bases_round_trip_and_update_without_magic_hours_text() {
        let repository = repository();
        insert(&repository, "01/04/2026", "16.50");
        let mut entry = repository
            .get_all_for_personal_assistant(1)
            .unwrap()
            .remove(0);
        assert_eq!(entry.hours_basis, HoursBasis::Contracted);
        assert_eq!(entry.summary_value(), "16.50");
        entry.hours_basis = HoursBasis::Variable;
        repository.update(&entry).unwrap();
        let mut entry = repository
            .get_for_personal_assistant_as_of(1, date("2026-04-01"))
            .unwrap()
            .unwrap();
        assert_eq!(entry.hours_basis, HoursBasis::Variable);
        assert_eq!(entry.contracted_hours, "");
        assert_eq!(entry.summary_value(), "Variable");
        entry.hours_basis = HoursBasis::Contracted;
        assert!(repository.update(&entry).is_err());
        assert!(repository.insert(&entry).is_err());
        entry.contracted_hours = "20.00".into();
        repository.update(&entry).unwrap();
        let stored = repository
            .get_all_for_personal_assistant(1)
            .unwrap()
            .remove(0);
        assert_eq!(stored.summary_value(), "20.00");
        assert_eq!(stored.created_at, "2026-01-01");
        entry.effective_date = "08/04/2026".into();
        entry.hours_basis = HoursBasis::Variable;
        repository.insert(&entry).unwrap();
        let stored = repository
            .get_for_personal_assistant_as_of(1, date("2026-04-08"))
            .unwrap()
            .unwrap();
        assert_eq!(stored.hours_basis, HoursBasis::Variable);
        assert_eq!(stored.contracted_hours, "");
        assert_eq!(
            repository
                .get_for_personal_assistant_as_of(1, date("2026-04-07"))
                .unwrap()
                .unwrap()
                .summary_value(),
            "20.00"
        );
    }

    #[test]
    fn variable_history_uses_highest_id_and_weekly_summary_preserves_unavailable() {
        let repository = repository();
        insert(&repository, "08/04/2026", "16");
        insert(&repository, "15/04/2026", "20");
        repository
            .insert(&ContractedHoursEntry {
                id: 0,
                personal_assistant_id: 1,
                effective_date: "15/04/2026".into(),
                contracted_hours: "ignored numeric text".into(),
                hours_basis: HoursBasis::Variable,
                created_at: "created".into(),
            })
            .unwrap();
        let dates = ["01/04/2026", "08/04/2026", "15/04/2026", "22/04/2026"].map(String::from);
        let values = dates.clone().map(|date| {
            repository
                .get_for_personal_assistant_as_of(
                    1,
                    NaiveDate::parse_from_str(&date, "%d/%m/%Y").unwrap(),
                )
                .unwrap()
                .map(|entry| entry.summary_value().to_string())
                .unwrap_or_else(|| "Unavailable".into())
        });
        assert_eq!(values, ["Unavailable", "16", "Variable", "Variable"]);
        assert_eq!(
            crate::pdf_generator::contracted_hours_summary(&values, &dates),
            "Unavailable (w/c 01/04)  16 (w/c 08/04)  Variable (w/c 15/04, 22/04)"
        );
        assert_eq!(
            crate::pdf_generator::contracted_hours_summary(
                &std::array::from_fn(|_| "Variable".into()),
                &dates
            ),
            "Variable"
        );
        insert(&repository, "15/04/2026", "24");
        assert_eq!(
            repository
                .get_for_personal_assistant_as_of(1, date("2026-04-15"))
                .unwrap()
                .unwrap()
                .summary_value(),
            "24"
        );
    }

    #[test]
    fn as_of_lookup_handles_before_first_exact_boundary_change_and_future_exclusion() {
        let repository = repository();
        insert(&repository, "01/04/2026", "16");
        insert(&repository, "24/08/2026", "20");
        insert(&repository, "01/10/2026", "30");

        assert!(repository
            .get_for_personal_assistant_as_of(1, date("2026-03-31"))
            .unwrap()
            .is_none());
        assert_eq!(
            repository
                .get_for_personal_assistant_as_of(1, date("2026-04-01"))
                .unwrap()
                .unwrap()
                .contracted_hours,
            "16"
        );
        assert_eq!(
            repository
                .get_for_personal_assistant_as_of(1, date("2026-08-23"))
                .unwrap()
                .unwrap()
                .contracted_hours,
            "16"
        );
        assert_eq!(
            repository
                .get_for_personal_assistant_as_of(1, date("2026-08-24"))
                .unwrap()
                .unwrap()
                .contracted_hours,
            "20"
        );
        assert_eq!(
            repository
                .get_for_personal_assistant_as_of(1, date("2026-08-31"))
                .unwrap()
                .unwrap()
                .contracted_hours,
            "20"
        );
    }

    #[test]
    fn equal_effective_dates_deterministically_select_highest_id() {
        let repository = repository();
        insert(&repository, "24/08/2026", "16");
        insert(&repository, "24/08/2026", "20");

        assert_eq!(
            repository
                .get_for_personal_assistant_as_of(1, date("2026-08-24"))
                .unwrap()
                .unwrap()
                .contracted_hours,
            "20"
        );
    }
}
