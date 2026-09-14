use rusqlite::{params, Connection, Result};

use crate::models::Employer;

pub struct EmployerRepository {
    connection: Connection,
}

impl EmployerRepository {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    pub fn insert(&self, employer: &Employer) -> Result<()> {
        let previous: Option<String> = None;
        let dob = crate::date_utils::optional_edited(
            employer.date_of_birth.as_deref(),
            previous.as_deref(),
            true,
        )?;
        self.connection.execute(
            "
            INSERT INTO employers (
                name,
                date_of_birth,
                national_insurance_number,
                reference_account_number,
                address,
                telephone,
                email,
                employer_signature,
                email_signature,
                default_pdf_template,
                sick_pay_enabled,
                mileage_enabled
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ",
            params![
                &employer.name,
                &dob,
                &employer.national_insurance_number,
                &employer.reference_account_number,
                &employer.address,
                &employer.telephone,
                &employer.email,
                &employer.employer_signature,
                &employer.email_signature,
                &employer.default_pdf_template,
                employer.sick_pay_enabled,
                employer.mileage_enabled,
            ],
        )?;

        Ok(())
    }

    pub fn update(&self, employer: &Employer) -> Result<()> {
        let previous = self
            .get_all()?
            .into_iter()
            .find(|row| row.id == employer.id)
            .and_then(|row| row.date_of_birth);
        let dob = crate::date_utils::optional_edited(
            employer.date_of_birth.as_deref(),
            previous.as_deref(),
            true,
        )?;
        self.connection.execute(
            "
            UPDATE employers
            SET
                name = ?1,
                date_of_birth = ?2,
                national_insurance_number = ?3,
                reference_account_number = ?4,
                address = ?5,
                telephone = ?6,
                email = ?7,
                employer_signature = ?8,
                email_signature = ?9,
                default_pdf_template = ?10,
                sick_pay_enabled = ?11,
                mileage_enabled = ?12
            WHERE id = ?13
            ",
            params![
                &employer.name,
                &dob,
                &employer.national_insurance_number,
                &employer.reference_account_number,
                &employer.address,
                &employer.telephone,
                &employer.email,
                &employer.employer_signature,
                &employer.email_signature,
                &employer.default_pdf_template,
                employer.sick_pay_enabled,
                employer.mileage_enabled,
                employer.id,
            ],
        )?;

        Ok(())
    }

    pub fn update_email_signature(
        &self,
        employer_id: i64,
        email_signature: Option<&str>,
    ) -> Result<()> {
        self.connection.execute(
            "UPDATE employers SET email_signature = ?1 WHERE id = ?2",
            params![email_signature, employer_id],
        )?;

        Ok(())
    }

    pub fn get_all(&self) -> Result<Vec<Employer>> {
        let mut statement = self.connection.prepare(
            "
            SELECT
                id,
                name,
                date_of_birth,
                national_insurance_number,
                reference_account_number,
                address,
                telephone,
                email,
                employer_signature,
                email_signature,
                default_pdf_template,
                sick_pay_enabled,
                mileage_enabled
            FROM employers
            ",
        )?;

        let employers = statement.query_map([], |row| {
            Ok(Employer {
                id: row.get(0)?,
                name: row.get(1)?,
                date_of_birth: row.get(2)?,
                national_insurance_number: row.get(3)?,
                reference_account_number: row.get(4)?,
                address: row.get(5)?,
                telephone: row.get(6)?,
                email: row.get(7)?,
                employer_signature: row.get(8)?,
                email_signature: row.get(9)?,
                default_pdf_template: row.get(10)?,
                sick_pay_enabled: row.get::<_, i64>(11)? != 0,
                mileage_enabled: row.get::<_, i64>(12)? != 0,
            })
        })?;

        let mut results = Vec::new();

        for employer in employers {
            results.push(employer?);
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::create_schema;

    fn create_test_repository() -> EmployerRepository {
        let connection = Connection::open_in_memory().unwrap();

        create_schema(&connection).unwrap();

        EmployerRepository::new(connection)
    }

    #[test]
    fn insert_and_get_employer() {
        let repository = create_test_repository();

        let employer = Employer {
            id: 0,
            name: "Robin Placeholder".to_string(),
            date_of_birth: None,
            national_insurance_number: None,
            reference_account_number: None,
            address: Some("Test Address".to_string()),
            telephone: None,
            email: None,
            employer_signature: None,
            email_signature: None,
            default_pdf_template: None,
            sick_pay_enabled: false,
            mileage_enabled: false,
        };

        repository.insert(&employer).unwrap();

        let employers = repository.get_all().unwrap();

        assert_eq!(employers.len(), 1);
        assert_eq!(employers[0].name, "Robin Placeholder");
    }
}
