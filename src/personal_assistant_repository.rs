use rusqlite::{params, Connection, Result};

use crate::models::PersonalAssistant;

pub struct PersonalAssistantRepository {
    connection: Connection,
}

impl PersonalAssistantRepository {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    pub fn insert(&self, assistant: &PersonalAssistant) -> Result<()> {
        self.connection.execute(
            "
            INSERT INTO personal_assistants (
                first_name,
                surname,
                date_of_birth,
                national_insurance_number,
                address,
                postcode,
                telephone,
                email,
                employment_status
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ",
            params![
                &assistant.first_name,
                &assistant.surname,
                &assistant.date_of_birth,
                &assistant.national_insurance_number,
                &assistant.address,
                &assistant.postcode,
                &assistant.telephone,
                &assistant.email,
                &assistant.employment_status,
            ],
        )?;

        Ok(())
    }

    pub fn get_all(&self) -> Result<Vec<PersonalAssistant>> {
        let mut statement = self.connection.prepare(
            "
            SELECT
                id,
                first_name,
                surname,
                date_of_birth,
                national_insurance_number,
                address,
                postcode,
                telephone,
                email,
                employment_status
            FROM personal_assistants
            ",
        )?;

        let assistants = statement.query_map([], |row| {
            Ok(PersonalAssistant {
                id: row.get(0)?,
                first_name: row.get(1)?,
                surname: row.get(2)?,
                date_of_birth: row.get(3)?,
                national_insurance_number: row.get(4)?,
                address: row.get(5)?,
                postcode: row.get(6)?,
                telephone: row.get(7)?,
                email: row.get(8)?,
                employment_status: row.get(9)?,
            })
        })?;

        let mut results = Vec::new();

        for assistant in assistants {
            results.push(assistant?);
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::create_schema;

    fn create_test_repository() -> PersonalAssistantRepository {
        let connection = Connection::open_in_memory().unwrap();

        create_schema(&connection).unwrap();

        PersonalAssistantRepository::new(connection)
    }

    #[test]
    fn insert_and_get_personal_assistant() {
        let repository = create_test_repository();

        let assistant = PersonalAssistant {
            id: 0,
            first_name: "Andy".to_string(),
            surname: "Pandy".to_string(),
            date_of_birth: Some("01/01/1990".to_string()),
            national_insurance_number: Some("AB123456C".to_string()),
            address: None,
            postcode: None,
            telephone: None,
            email: None,
            employment_status: Some("Active".to_string()),
        };

        repository.insert(&assistant).unwrap();

        let assistants = repository.get_all().unwrap();

        assert_eq!(assistants.len(), 1);
        assert_eq!(assistants[0].first_name, "Andy");
        assert_eq!(assistants[0].surname, "Pandy");
    }
}
