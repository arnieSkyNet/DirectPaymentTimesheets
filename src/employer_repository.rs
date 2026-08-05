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
        self.connection.execute(
            "
            INSERT INTO employers (
                name,
                address,
                postcode,
                telephone,
                email,
                payroll_provider,
                payroll_provider_address,
                payroll_provider_phone,
                employer_signature,
                default_pdf_template,
                sick_pay_enabled,
                mileage_enabled
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ",
            params![
                &employer.name,
                &employer.address,
                &employer.postcode,
                &employer.telephone,
                &employer.email,
                &employer.payroll_provider,
                &employer.payroll_provider_address,
                &employer.payroll_provider_phone,
                &employer.employer_signature,
                &employer.default_pdf_template,
                employer.sick_pay_enabled,
                employer.mileage_enabled,
            ],
        )?;

        Ok(())
    }

    pub fn update(&self, employer: &Employer) -> Result<()> {
        self.connection.execute(
                        "
                                    UPDATE employers
                                                SET
                                                                name = ?1,
                                                                                address = ?2,
                                                                                                postcode = ?3,
                                                                                                                telephone = ?4,
                                                                                                                                email = ?5,
                                                                                                                                                payroll_provider = ?6,
                                                                                                                                                                payroll_provider_address = ?7,
                                                                                                                                                                                payroll_provider_phone = ?8,
                                                                                                                                                                                                employer_signature = ?9,
                                                                                                                                                                                                                default_pdf_template = ?10,
                                                                                                                                                                                                                                sick_pay_enabled = ?11,
                                                                                                                                                                                                                                                mileage_enabled = ?12
                                                                                                                                                                                                                                                            WHERE id = ?13
                                                                                                                                                                                                                                                                        ",
                                                                                                                                                                                                                                                                                    params![
                                                                                                                                                                                                                                                                                                    &employer.name,
                                                                                                                                                                                                                                                                                                                    &employer.address,
                                                                                                                                                                                                                                                                                                                                    &employer.postcode,
                                                                                                                                                                                                                                                                                                                                                    &employer.telephone,
                                                                                                                                                                                                                                                                                                                                                                    &employer.email,
                                                                                                                                                                                                                                                                                                                                                                                    &employer.payroll_provider,
                                                                                                                                                                                                                                                                                                                                                                                                    &employer.payroll_provider_address,
                                                                                                                                                                                                                                                                                                                                                                                                                    &employer.payroll_provider_phone,
                                                                                                                                                                                                                                                                                                                                                                                                                                    &employer.employer_signature,
                                                                                                                                                                                                                                                                                                                                                                                                                                                    &employer.default_pdf_template,
                                                                                                                                                                                                                                                                                                                                                                                                                                                                    employer.sick_pay_enabled,
                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    employer.mileage_enabled,
                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    employer.id,
                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                ],
                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        )?;

        Ok(())
    }

    pub fn get_all(&self) -> Result<Vec<Employer>> {
        let mut statement = self.connection.prepare(
            "
            SELECT
                id,
                name,
                address,
                postcode,
                telephone,
                email,
                payroll_provider,
                payroll_provider_address,
                payroll_provider_phone,
                employer_signature,
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
                address: row.get(2)?,
                postcode: row.get(3)?,
                telephone: row.get(4)?,
                email: row.get(5)?,
                payroll_provider: row.get(6)?,
                payroll_provider_address: row.get(7)?,
                payroll_provider_phone: row.get(8)?,
                employer_signature: row.get(9)?,
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
            name: "Morgan".to_string(),
            address: Some("Test Address".to_string()),
            postcode: Some("AB12 3CD".to_string()),
            telephone: None,
            email: None,
            payroll_provider: Some("Payroll Department".to_string()),
            payroll_provider_address: None,
            payroll_provider_phone: None,
            employer_signature: None,
            default_pdf_template: None,
            sick_pay_enabled: false,
            mileage_enabled: false,
        };

        repository.insert(&employer).unwrap();

        let employers = repository.get_all().unwrap();

        assert_eq!(employers.len(), 1);
        assert_eq!(employers[0].name, "Morgan");
    }
}
