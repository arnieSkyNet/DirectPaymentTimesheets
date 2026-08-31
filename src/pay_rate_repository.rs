use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension, Result};
use std::collections::HashSet;

#[derive(Debug)]
pub struct PayRateRepository {
    connection: Connection,
}

#[derive(Debug, Clone)]
pub struct PersonalAssistantPayRate {
    pub id: i64,
    pub personal_assistant_id: i64,
    pub effective_date: String,
    pub base_hourly_rate: f64,
    pub employer_top_up_rate: f64,
    pub created_at: String,
}

impl PayRateRepository {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    pub fn insert(&self, rate: &PersonalAssistantPayRate) -> Result<()> {
        let effective_date = normalise_date(&rate.effective_date);

        self.connection.execute(
            "
            INSERT INTO personal_assistant_pay_rates (
                personal_assistant_id,
                effective_date,
                base_hourly_rate,
                employer_top_up_rate,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5)
            ",
            params![
                &rate.personal_assistant_id,
                &effective_date,
                &rate.base_hourly_rate,
                &rate.employer_top_up_rate,
                &rate.created_at,
            ],
        )?;

        Ok(())
    }

    pub fn update(&self, rate: &PersonalAssistantPayRate) -> Result<()> {
        let effective_date = normalise_date(&rate.effective_date);

        self.connection.execute(
            "
            UPDATE personal_assistant_pay_rates
            SET
                effective_date = ?1,
                base_hourly_rate = ?2,
                employer_top_up_rate = ?3
            WHERE id = ?4
            ",
            params![
                &effective_date,
                &rate.base_hourly_rate,
                &rate.employer_top_up_rate,
                &rate.id,
            ],
        )?;

        Ok(())
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        self.connection.execute(
            "
            DELETE FROM personal_assistant_pay_rates
            WHERE id = ?1
            ",
            params![id],
        )?;

        Ok(())
    }

    pub fn get_all_for_personal_assistant(
        &self,
        personal_assistant_id: i64,
    ) -> Result<Vec<PersonalAssistantPayRate>> {
        let mut statement = self.connection.prepare(
            "
            SELECT
                id,
                personal_assistant_id,
                effective_date,
                base_hourly_rate,
                employer_top_up_rate,
                created_at
            FROM personal_assistant_pay_rates
            WHERE personal_assistant_id = ?1
            ORDER BY
                substr(effective_date, 7, 4) DESC,
                substr(effective_date, 4, 2) DESC,
                substr(effective_date, 1, 2) DESC,
                id DESC
            ",
        )?;

        let rates = statement.query_map(params![personal_assistant_id], |row| {
            Ok(PersonalAssistantPayRate {
                id: row.get(0)?,
                personal_assistant_id: row.get(1)?,
                effective_date: row.get(2)?,
                base_hourly_rate: row.get(3)?,
                employer_top_up_rate: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;

        let mut results = Vec::new();

        for rate in rates {
            results.push(rate?);
        }

        Ok(results)
    }

    #[allow(dead_code)]
    pub fn get_current_for_personal_assistant(
        &self,
        personal_assistant_id: i64,
    ) -> Result<Option<PersonalAssistantPayRate>> {
        let mut rates = self.get_all_for_personal_assistant(personal_assistant_id)?;

        if rates.is_empty() {
            Ok(None)
        } else {
            Ok(Some(rates.remove(0)))
        }
    }

    pub fn get_for_personal_assistant_as_of(
        &self,
        personal_assistant_id: i64,
        as_of_date: NaiveDate,
    ) -> Result<Option<PersonalAssistantPayRate>> {
        self.connection
            .query_row(
                "
                SELECT
                    id,
                    personal_assistant_id,
                    effective_date,
                    base_hourly_rate,
                    employer_top_up_rate,
                    created_at
                FROM personal_assistant_pay_rates
                WHERE personal_assistant_id = ?1
                  AND printf(
                        '%04d-%02d-%02d',
                        CAST(substr(effective_date, 7, 4) AS INTEGER),
                        CAST(substr(effective_date, 4, 2) AS INTEGER),
                        CAST(substr(effective_date, 1, 2) AS INTEGER)
                      ) <= ?2
                ORDER BY
                    substr(effective_date, 7, 4) DESC,
                    substr(effective_date, 4, 2) DESC,
                    substr(effective_date, 1, 2) DESC,
                    id DESC
                LIMIT 1
                ",
                params![
                    personal_assistant_id,
                    as_of_date.format("%Y-%m-%d").to_string()
                ],
                |row| {
                    Ok(PersonalAssistantPayRate {
                        id: row.get(0)?,
                        personal_assistant_id: row.get(1)?,
                        effective_date: row.get(2)?,
                        base_hourly_rate: row.get(3)?,
                        employer_top_up_rate: row.get(4)?,
                        created_at: row.get(5)?,
                    })
                },
            )
            .optional()
    }

    pub fn get_applicable_during_period(
        &self,
        personal_assistant_id: i64,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<PersonalAssistantPayRate>> {
        let mut rates = self.get_all_for_personal_assistant(personal_assistant_id)?;
        rates.retain(|rate| {
            NaiveDate::parse_from_str(&rate.effective_date, "%d/%m/%Y")
                .is_ok_and(|date| date <= end_date)
        });

        let active_at_start = self
            .get_for_personal_assistant_as_of(personal_assistant_id, start_date)?
            .map(|rate| rate.id);
        let mut effective_dates_seen = HashSet::new();
        rates.retain(|rate| {
            let is_authoritative_for_effective_date =
                effective_dates_seen.insert(rate.effective_date.clone());
            is_authoritative_for_effective_date
                && (active_at_start == Some(rate.id)
                    || NaiveDate::parse_from_str(&rate.effective_date, "%d/%m/%Y")
                        .is_ok_and(|date| date >= start_date && date <= end_date))
        });
        rates.sort_by_key(|rate| {
            (
                NaiveDate::parse_from_str(&rate.effective_date, "%d/%m/%Y").ok(),
                rate.id,
            )
        });
        Ok(rates)
    }
}

fn normalise_date(date: &str) -> String {
    let parts: Vec<&str> = date.split('/').collect();

    if parts.len() == 3 {
        let day = parts[0].parse::<u32>().unwrap_or(0);
        let month = parts[1].parse::<u32>().unwrap_or(0);
        let year = parts[2].parse::<u32>().unwrap_or(0);

        if day > 0 && month > 0 && year > 0 {
            return format!("{:02}/{:02}/{:04}", day, month, year);
        }
    }

    date.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::create_schema;

    fn create_test_repository() -> PayRateRepository {
        let connection = Connection::open_in_memory().unwrap();

        create_schema(&connection).unwrap();

        PayRateRepository::new(connection)
    }

    #[test]
    fn insert_and_get_pay_rate_history() {
        let repository = create_test_repository();

        let rate = PersonalAssistantPayRate {
            id: 0,
            personal_assistant_id: 1,
            effective_date: "1/4/2026".to_string(),
            base_hourly_rate: 12.72,
            employer_top_up_rate: 0.0,
            created_at: "2026-03-01".to_string(),
        };

        repository.insert(&rate).unwrap();

        let rates = repository.get_all_for_personal_assistant(1).unwrap();

        assert_eq!(rates.len(), 1);
        assert_eq!(rates[0].effective_date, "01/04/2026");
        assert_eq!(rates[0].base_hourly_rate, 12.72);
    }

    fn insert_rate(repository: &PayRateRepository, effective_date: &str, base_hourly_rate: f64) {
        repository
            .insert(&PersonalAssistantPayRate {
                id: 0,
                personal_assistant_id: 1,
                effective_date: effective_date.to_string(),
                base_hourly_rate,
                employer_top_up_rate: 0.25,
                created_at: "2026-01-01".to_string(),
            })
            .unwrap();
    }

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn as_of_lookup_selects_history_and_excludes_future_rate() {
        let repository = create_test_repository();
        insert_rate(&repository, "01/04/2025", 11.0);
        insert_rate(&repository, "01/04/2026", 12.0);
        insert_rate(&repository, "01/04/2027", 13.0);

        let selected = repository
            .get_for_personal_assistant_as_of(1, date("2026-08-01"))
            .unwrap()
            .unwrap();

        assert_eq!(selected.effective_date, "01/04/2026");
        assert_eq!(selected.base_hourly_rate, 12.0);
    }

    #[test]
    fn as_of_lookup_handles_exact_boundary_day_before_day_after_and_before_first_rate() {
        let repository = create_test_repository();
        insert_rate(&repository, "01/04/2025", 11.0);
        insert_rate(&repository, "31/03/2027", 12.0);

        assert_eq!(
            repository
                .get_for_personal_assistant_as_of(1, date("2027-03-30"))
                .unwrap()
                .unwrap()
                .base_hourly_rate,
            11.0
        );
        assert_eq!(
            repository
                .get_for_personal_assistant_as_of(1, date("2027-03-31"))
                .unwrap()
                .unwrap()
                .base_hourly_rate,
            12.0
        );
        assert_eq!(
            repository
                .get_for_personal_assistant_as_of(1, date("2027-04-01"))
                .unwrap()
                .unwrap()
                .base_hourly_rate,
            12.0
        );
        assert!(repository
            .get_for_personal_assistant_as_of(1, date("2025-03-31"))
            .unwrap()
            .is_none());
    }

    #[test]
    fn equal_effective_dates_deterministically_select_highest_id() {
        let repository = create_test_repository();
        insert_rate(&repository, "01/04/2026", 11.0);
        insert_rate(&repository, "01/04/2026", 12.0);

        let selected = repository
            .get_for_personal_assistant_as_of(1, date("2026-04-01"))
            .unwrap()
            .unwrap();

        assert_eq!(selected.base_hourly_rate, 12.0);
    }
}
