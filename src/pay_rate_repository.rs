use rusqlite::{params, Connection, Result};

#[derive(Debug)]
pub struct PayRateRepository {
    connection: Connection,
}

#[derive(Debug)]
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
                &rate.effective_date,
                &rate.base_hourly_rate,
                &rate.employer_top_up_rate,
                &rate.created_at,
            ],
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
            ORDER BY effective_date
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
            effective_date: "2026-04-01".to_string(),
            base_hourly_rate: 12.72,
            employer_top_up_rate: 0.0,
            created_at: "2026-03-01".to_string(),
        };

        repository.insert(&rate).unwrap();

        let rates = repository.get_all_for_personal_assistant(1).unwrap();

        assert_eq!(rates.len(), 1);
        assert_eq!(rates[0].base_hourly_rate, 12.72);
    }
}
