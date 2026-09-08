use rusqlite::{params, Connection, Result};

use crate::models::PersonalAssistant;

pub struct PersonalAssistantRepository {
    connection: Connection,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PersonalAssistantDeleteResult {
    Deleted,
    HasDependentRecords,
    NotFound,
}

impl PersonalAssistantRepository {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    pub fn insert(&self, assistant: &PersonalAssistant) -> Result<()> {
        let leaving_date = validated_leaving_date(assistant)?;
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
                employment_status,
                sick_pay_enabled,
                mileage_enabled,
                start_date,
                signature, leaving_date
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
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
                assistant.sick_pay_enabled,
                assistant.mileage_enabled,
                &assistant.start_date,
                &assistant.signature,
                leaving_date,
            ],
        )?;

        Ok(())
    }

    pub fn update(&self, assistant: &PersonalAssistant) -> Result<()> {
        let leaving_date = validated_leaving_date(assistant)?;
        self.connection.execute(
            "
UPDATE personal_assistants
SET
first_name = ?1,
surname = ?2,
date_of_birth = ?3,
national_insurance_number = ?4,
address = ?5,
postcode = ?6,
telephone = ?7,
email = ?8,
employment_status = ?9,
sick_pay_enabled = ?10,
mileage_enabled = ?11,
start_date = ?12,
signature = ?13, leaving_date = ?15
WHERE id = ?14
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
                assistant.sick_pay_enabled,
                assistant.mileage_enabled,
                &assistant.start_date,
                &assistant.signature,
                assistant.id,
                leaving_date,
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
employment_status,
sick_pay_enabled,
mileage_enabled,
start_date,
signature, leaving_date
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
                sick_pay_enabled: row.get::<_, i64>(10)? != 0,
                mileage_enabled: row.get::<_, i64>(11)? != 0,
                start_date: row.get(12)?,
                leaving_date: row.get(14)?,
                signature: row.get(13)?,
            })
        })?;

        let mut results = Vec::new();

        for assistant in assistants {
            results.push(assistant?);
        }

        Ok(results)
    }

    pub fn get_active(&self) -> Result<Vec<PersonalAssistant>> {
        let assistants = self.get_all()?;

        Ok(assistants
            .into_iter()
            .filter(|assistant| assistant.employment_status.as_deref() == Some("Active"))
            .collect())
    }

    pub fn has_dependent_records(&self, personal_assistant_id: i64) -> Result<bool> {
        self.connection.query_row(
            "
            SELECT EXISTS (
                SELECT 1 FROM timesheets
                WHERE personal_assistant_id = ?1

                UNION ALL

                SELECT 1 FROM personal_assistant_pay_rates
                WHERE personal_assistant_id = ?1

                UNION ALL

                SELECT 1 FROM personal_assistant_contracted_hours
                WHERE personal_assistant_id = ?1

                UNION ALL

                SELECT 1 FROM direct_shifts
                WHERE personal_assistant_id = ?1

                UNION ALL

                SELECT 1 FROM direct_shift_audit
                WHERE before_personal_assistant_id = ?1
                   OR after_personal_assistant_id = ?1

                UNION ALL

                SELECT 1 FROM payroll_timesheets
                WHERE personal_assistant_id = ?1

                UNION ALL

                SELECT 1
                FROM payroll_timesheet_public_holidays AS holiday
                INNER JOIN payroll_timesheets AS timesheet
                    ON timesheet.id = holiday.payroll_timesheet_id
                WHERE timesheet.personal_assistant_id = ?1

                UNION ALL

                SELECT 1 FROM payroll_timesheet_email_status
                WHERE personal_assistant_id = ?1
            )
            ",
            params![personal_assistant_id],
            |row| row.get(0),
        )
    }

    pub fn delete_if_unreferenced(
        &self,
        personal_assistant_id: i64,
    ) -> Result<PersonalAssistantDeleteResult> {
        if self.has_dependent_records(personal_assistant_id)? {
            return Ok(PersonalAssistantDeleteResult::HasDependentRecords);
        }

        let deleted = self.connection.execute(
            "DELETE FROM personal_assistants WHERE id = ?1",
            params![personal_assistant_id],
        )?;

        if deleted == 1 {
            Ok(PersonalAssistantDeleteResult::Deleted)
        } else {
            Ok(PersonalAssistantDeleteResult::NotFound)
        }
    }
}

fn validated_leaving_date(assistant: &PersonalAssistant) -> Result<Option<String>> {
    let Some(value) = assistant
        .leaving_date
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(None);
    };
    let leaving = crate::models::parse_employment_date(value).ok_or_else(|| {
        rusqlite::Error::InvalidParameterName(
            "Leaving date must be a real date (DD/MM/YYYY).".into(),
        )
    })?;
    if let Some(start) = assistant
        .start_date
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        let start = crate::models::parse_employment_date(start).ok_or_else(|| {
            rusqlite::Error::InvalidParameterName(
                "Start date must be valid before setting a Leaving date.".into(),
            )
        })?;
        if leaving < start {
            return Err(rusqlite::Error::InvalidParameterName(
                "Leaving date cannot be before Start date.".into(),
            ));
        }
    }
    Ok(Some(leaving.format("%d/%m/%Y").to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::create_schema;

    fn test_repository() -> PersonalAssistantRepository {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        PersonalAssistantRepository::new(connection)
    }

    fn test_assistant() -> PersonalAssistant {
        PersonalAssistant {
            id: 0,
            first_name: "Alex".to_string(),
            surname: "Smith".to_string(),
            date_of_birth: Some("01/02/1990".to_string()),
            national_insurance_number: Some("AB123456C".to_string()),
            address: Some("1 Example Street\nExample Town".to_string()),
            postcode: Some("AB1 2CD".to_string()),
            telephone: Some("01234 567890".to_string()),
            email: Some("alex@example.test".to_string()),
            employment_status: Some("Active".to_string()),
            sick_pay_enabled: true,
            mileage_enabled: true,
            start_date: Some("03/04/2025".to_string()),
            leaving_date: None,
            signature: Some("/example/signatures/alex.png".to_string()),
        }
    }

    fn insert_test_assistant(repository: &PersonalAssistantRepository) -> i64 {
        repository.insert(&test_assistant()).unwrap();
        repository.get_all().unwrap()[0].id
    }

    #[test]
    fn leaving_date_saves_with_numeric_and_human_readable_start_dates() {
        for start in ["17/05/2024", "17 May 2024"] {
            let repository = test_repository();
            let mut assistant = test_assistant();
            assistant.start_date = Some(start.into());
            repository.insert(&assistant).unwrap();
            let mut saved = repository.get_all().unwrap().remove(0);
            saved.leaving_date = Some("30/09/2026".into());
            repository.update(&saved).unwrap();
            let mut reloaded = repository.get_all().unwrap().remove(0);
            assert_eq!(reloaded.start_date.as_deref(), Some(start));
            assert_eq!(reloaded.leaving_date.as_deref(), Some("30/09/2026"));
            reloaded.leaving_date = Some("16 May 2024".into());
            assert!(repository.update(&reloaded).is_err());
            for blank in [Some("   ".into()), None] {
                reloaded.leaving_date = blank;
                repository.update(&reloaded).unwrap();
                assert!(repository.get_all().unwrap()[0].leaving_date.is_none());
            }
        }
    }

    #[test]
    fn leaving_date_round_trip_validation_and_status_preservation() {
        let connection = Connection::open_in_memory().unwrap();
        crate::database::create_schema(&connection).unwrap();
        let repository = PersonalAssistantRepository::new(connection);
        let mut assistant = test_assistant();
        assistant.leaving_date = Some("18/9/30".into());
        repository.insert(&assistant).unwrap();
        let mut saved = repository.get_all().unwrap().remove(0);
        assert_eq!(saved.leaving_date.as_deref(), Some("18/09/2030"));
        assert_eq!(saved.employment_status.as_deref(), Some("Active"));
        for value in ["31/02/2030", "02/04/2025"] {
            saved.leaving_date = Some(value.into());
            assert!(repository.update(&saved).is_err());
        }
        saved.leaving_date = Some("03/04/2025".into());
        repository.update(&saved).unwrap();
        assert_eq!(
            repository.get_all().unwrap()[0]
                .employment_status
                .as_deref(),
            Some("Active")
        );
        saved.leaving_date = Some("".into());
        repository.update(&saved).unwrap();
        assert!(repository.get_all().unwrap()[0].leaving_date.is_none());
    }

    #[test]
    fn employment_overlap_is_inclusive_and_historical_records_remain_eligible() {
        let date = |value| crate::models::parse_employment_date(value).unwrap();
        let mut assistant = test_assistant();
        assistant.start_date = Some("14/09/2026".into());
        assistant.leaving_date = Some("30/09/2026".into());
        for (start, end, expected) in [
            ("01/09/2026", "13/09/2026", false),
            ("01/09/2026", "14/09/2026", true),
            ("30/09/2026", "27/10/2026", true),
            ("01/10/2026", "28/10/2026", false),
        ] {
            assert_eq!(
                assistant
                    .eligible_for_period(date(start), date(end), false)
                    .unwrap(),
                expected
            );
            assert!(assistant
                .eligible_for_period(date(start), date(end), true)
                .unwrap());
        }
        assistant.employment_status = Some("Inactive".into());
        assert!(!assistant
            .eligible_for_period(date("14/09/2026"), date("30/09/2026"), false)
            .unwrap());
        assert!(assistant
            .eligible_for_period(date("01/10/2026"), date("28/10/2026"), true)
            .unwrap());
    }

    #[test]
    fn insert_and_read_back_preserves_all_personal_assistant_fields() {
        let repository = test_repository();
        let assistant = test_assistant();

        repository.insert(&assistant).unwrap();
        let saved = repository.get_all().unwrap();

        assert_eq!(saved.len(), 1);
        let saved = &saved[0];
        assert_eq!(saved.first_name, assistant.first_name);
        assert_eq!(saved.surname, assistant.surname);
        assert_eq!(saved.date_of_birth, assistant.date_of_birth);
        assert_eq!(
            saved.national_insurance_number,
            assistant.national_insurance_number
        );
        assert_eq!(saved.address, assistant.address);
        assert_eq!(saved.postcode, assistant.postcode);
        assert_eq!(saved.telephone, assistant.telephone);
        assert_eq!(saved.email, assistant.email);
        assert_eq!(saved.employment_status, assistant.employment_status);
        assert_eq!(saved.sick_pay_enabled, assistant.sick_pay_enabled);
        assert_eq!(saved.mileage_enabled, assistant.mileage_enabled);
        assert_eq!(saved.start_date, assistant.start_date);
        assert_eq!(saved.signature, assistant.signature);
    }

    #[test]
    fn deletes_an_unreferenced_personal_assistant() {
        let repository = test_repository();
        let personal_assistant_id = insert_test_assistant(&repository);

        let result = repository
            .delete_if_unreferenced(personal_assistant_id)
            .unwrap();

        assert_eq!(result, PersonalAssistantDeleteResult::Deleted);
        assert!(repository.get_all().unwrap().is_empty());
    }

    #[test]
    fn refuses_to_delete_a_personal_assistant_with_historical_timesheets() {
        let repository = test_repository();
        let personal_assistant_id = insert_test_assistant(&repository);
        repository
            .connection
            .execute(
                "
                INSERT INTO timesheets (
                    pa_name,
                    personal_assistant_id,
                    start_time,
                    end_time,
                    break_minutes,
                    worked_minutes,
                    hourly_rate,
                    amount,
                    notes
                )
                VALUES ('Alex Smith', ?1, '09:00', '17:00', 30, 450, 12.50, 93.75, NULL)
                ",
                params![personal_assistant_id],
            )
            .unwrap();

        let result = repository
            .delete_if_unreferenced(personal_assistant_id)
            .unwrap();

        assert_eq!(result, PersonalAssistantDeleteResult::HasDependentRecords);
        assert_eq!(repository.get_all().unwrap().len(), 1);
    }

    #[test]
    fn refuses_to_delete_a_personal_assistant_with_direct_shift_evidence() {
        let repository = test_repository();
        let personal_assistant_id = insert_test_assistant(&repository);
        repository
            .connection
            .execute(
                "INSERT INTO direct_shifts (
                    personal_assistant_id, start_time, end_time, break_minutes,
                    notes, source_type, created_at, updated_at
                 ) VALUES (?1, '2026-09-01T09:00', NULL, 0, NULL, 'direct', 'created', 'updated')",
                params![personal_assistant_id],
            )
            .unwrap();

        assert_eq!(
            repository
                .delete_if_unreferenced(personal_assistant_id)
                .unwrap(),
            PersonalAssistantDeleteResult::HasDependentRecords
        );
    }
}
