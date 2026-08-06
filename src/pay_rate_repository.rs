use rusqlite::{params, Connection, Result};

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
                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            ORDER BY substr(effective_date, 7, 4) DESC,
                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 substr(effective_date, 4, 2) DESC,
                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      substr(effective_date, 1, 2) DESC
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
}
