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
        let previous: Option<PersonalAssistant> = None;
        let dob = crate::date_utils::optional_edited(
            assistant.date_of_birth.as_deref(),
            previous.as_ref().and_then(|pa| pa.date_of_birth.as_deref()),
            true,
        )?;
        let start_date = crate::date_utils::optional_edited(
            assistant.start_date.as_deref(),
            previous.as_ref().and_then(|pa| pa.start_date.as_deref()),
            true,
        )?;
        let leaving_date = crate::date_utils::optional_edited(
            assistant.leaving_date.as_deref(),
            previous.as_ref().and_then(|pa| pa.leaving_date.as_deref()),
            false,
        )?;
        if previous.as_ref().is_none_or(|pa| {
            pa.start_date != assistant.start_date || pa.leaving_date != assistant.leaving_date
        }) {
            validated_leaving_date(assistant)?;
        }
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
                mileage_enabled,
                start_date,
                signature, leaving_date
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ",
            params![
                &assistant.first_name,
                &assistant.surname,
                &dob,
                &assistant.national_insurance_number,
                &assistant.address,
                &assistant.postcode,
                &assistant.telephone,
                &assistant.email,
                &assistant.employment_status,
                assistant.mileage_enabled,
                &start_date,
                &assistant.signature,
                leaving_date,
            ],
        )?;

        Ok(())
    }

    pub fn update(&self, assistant: &PersonalAssistant) -> Result<()> {
        let previous = self.get_all()?.into_iter().find(|pa| pa.id == assistant.id);
        let dob = crate::date_utils::optional_edited(
            assistant.date_of_birth.as_deref(),
            previous.as_ref().and_then(|pa| pa.date_of_birth.as_deref()),
            true,
        )?;
        let start_date = crate::date_utils::optional_edited(
            assistant.start_date.as_deref(),
            previous.as_ref().and_then(|pa| pa.start_date.as_deref()),
            true,
        )?;
        let leaving_date = crate::date_utils::optional_edited(
            assistant.leaving_date.as_deref(),
            previous.as_ref().and_then(|pa| pa.leaving_date.as_deref()),
            false,
        )?;
        if previous.as_ref().is_none_or(|pa| {
            pa.start_date != assistant.start_date || pa.leaving_date != assistant.leaving_date
        }) {
            validated_leaving_date(assistant)?;
        }
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
mileage_enabled = ?10,
start_date = ?11,
signature = ?12, leaving_date = ?14
WHERE id = ?13
",
            params![
                &assistant.first_name,
                &assistant.surname,
                &dob,
                &assistant.national_insurance_number,
                &assistant.address,
                &assistant.postcode,
                &assistant.telephone,
                &assistant.email,
                &assistant.employment_status,
                assistant.mileage_enabled,
                &start_date,
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
                mileage_enabled: row.get::<_, i64>(10)? != 0,
                start_date: row.get(11)?,
                leaving_date: row.get(13)?,
                signature: row.get(12)?,
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

    #[test]
    fn malformed_legacy_dates_survive_unrelated_edits_but_changed_fields_require_correction() {
        let repo = test_repository();
        let id = insert_test_assistant(&repo);
        repo.connection.execute("UPDATE personal_assistants SET date_of_birth='unknown', start_date='bad legacy', leaving_date='?' WHERE id=?1", [id]).unwrap();
        let mut pa = repo.get_all().unwrap().remove(0);
        pa.surname = "Changed".into();
        repo.update(&pa).unwrap();
        assert_eq!(
            repo.get_all().unwrap()[0].start_date.as_deref(),
            Some("bad legacy")
        );
        assert!(pa
            .eligible_for_period(
                crate::date_utils::parse_input("1 Apr 2026").unwrap(),
                crate::date_utils::parse_input("28 Apr 2026").unwrap(),
                true
            )
            .unwrap());
        pa.date_of_birth = Some("bad new".into());
        assert!(repo.update(&pa).is_err());
        pa.date_of_birth = Some("4th April 1990".into());
        repo.update(&pa).unwrap();
        assert_eq!(
            repo.get_all().unwrap()[0].date_of_birth.as_deref(),
            Some("1990-04-04")
        );
    }

    use super::*;
    use crate::database::create_schema;

    fn test_repository() -> PersonalAssistantRepository {
        let connection = Connection::open_in_memory().unwrap();
        create_schema(&connection).unwrap();
        PersonalAssistantRepository::new(connection)
    }

    #[test]
    fn pa_updates_ignore_and_preserve_legacy_sickness_flag() {
        let repository = test_repository();
        repository.insert(&test_assistant()).unwrap();
        let mut pa = repository.get_all().unwrap().remove(0);
        assert_eq!(
            repository
                .connection
                .query_row::<i64, _, _>(
                    "SELECT sick_pay_enabled FROM personal_assistants WHERE id=?1",
                    [pa.id],
                    |row| row.get(0),
                )
                .unwrap(),
            0
        );
        for legacy in [0, 1] {
            repository
                .connection
                .execute(
                    "UPDATE personal_assistants SET sick_pay_enabled=?1 WHERE id=?2",
                    params![legacy, pa.id],
                )
                .unwrap();
            pa.surname = format!("Updated {legacy}");
            repository.update(&pa).unwrap();
            let loaded = repository.get_all().unwrap().remove(0);
            assert_eq!(loaded.surname, pa.surname);
            assert_eq!(loaded.mileage_enabled, pa.mileage_enabled);
            assert_eq!(loaded.signature, pa.signature);
            assert_eq!(loaded.leaving_date, pa.leaving_date);
            assert_eq!(
                repository
                    .connection
                    .query_row::<i64, _, _>(
                        "SELECT sick_pay_enabled FROM personal_assistants WHERE id=?1",
                        [pa.id],
                        |row| row.get(0),
                    )
                    .unwrap(),
                legacy
            );
        }
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
            assert_eq!(reloaded.start_date.as_deref(), Some("2024-05-17"));
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
        assert!(repository.insert(&assistant).is_err());
        assistant.leaving_date = Some("18/9/2030".into());
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

    macro_rules! period_overlap_case {
        ($name:ident, $start:expr, $leave:expr, $expected:expr) => {
            #[test]
            fn $name() {
                let date = |s| crate::models::parse_employment_date(s).unwrap();
                for status in [Some("Active"), Some("Inactive"), None] {
                    let mut pa = test_assistant();
                    pa.employment_status = status.map(str::to_string);
                    pa.start_date = Some($start.into());
                    pa.leaving_date = $leave.map(str::to_string);
                    assert_eq!(
                        pa.employment_overlaps(date("10/08/2026"), date("06/09/2026"))
                            .unwrap(),
                        $expected
                    );
                    assert_eq!(
                        pa.eligible_for_period(date("10/08/2026"), date("06/09/2026"), false)
                            .unwrap(),
                        $expected
                    );
                    assert!(pa
                        .eligible_for_period(date("10/08/2026"), date("06/09/2026"), true)
                        .unwrap());
                }
            }
        };
    }
    period_overlap_case!(
        future_start_excluded_regardless_of_status,
        "07/09/2026",
        None::<&str>,
        false
    );
    period_overlap_case!(
        mid_period_start_included_regardless_of_status,
        "20/08/2026",
        None::<&str>,
        true
    );
    period_overlap_case!(
        start_on_period_end_included_regardless_of_status,
        "06/09/2026",
        None::<&str>,
        true
    );
    period_overlap_case!(
        leaving_before_period_excludes_new_preparations,
        "01/01/2026",
        Some("09/08/2026"),
        false
    );
    period_overlap_case!(
        leaving_during_period_included_regardless_of_status,
        "01/01/2026",
        Some("20/08/2026"),
        true
    );
    period_overlap_case!(
        leaving_on_period_start_included_regardless_of_status,
        "01/01/2026",
        Some("10/08/2026"),
        true
    );
    period_overlap_case!(
        future_inactive_leaver_excluded_from_earlier_period,
        "01/01/2027",
        Some("01/02/2027"),
        false
    );

    #[test]
    fn period_overlap_parses_existing_formats_without_rewriting_dates() {
        let date = |s| crate::models::parse_employment_date(s).unwrap();
        for start in ["20 August 2026", "2026-08-20", "20/8/26", "20-08-2026"] {
            let mut pa = test_assistant();
            pa.start_date = Some(start.into());
            pa.leaving_date = Some("06 Sep 2026".into());
            pa.employment_status = Some("Inactive".into());
            assert!(pa
                .employment_overlaps(date("10/08/2026"), date("06/09/2026"))
                .unwrap());
            assert_eq!(pa.start_date.as_deref(), Some(start));
            assert_eq!(pa.leaving_date.as_deref(), Some("06 Sep 2026"));
        }
    }

    #[test]
    fn legacy_missing_dates_remain_unbounded_and_invalid_dates_do_not_hide_stored_payroll() {
        let date = |s| crate::models::parse_employment_date(s).unwrap();
        let mut pa = test_assistant();
        pa.employment_status = Some("Inactive".into());
        pa.start_date = None;
        pa.leaving_date = Some("  ".into());
        assert!(pa
            .employment_overlaps(date("10/08/2026"), date("06/09/2026"))
            .unwrap());
        for (start, leave) in [("invalid", None), ("01/01/2026", Some("31/02/2026"))] {
            pa.start_date = Some(start.into());
            pa.leaving_date = leave.map(str::to_string);
            assert!(pa
                .eligible_for_period(date("10/08/2026"), date("06/09/2026"), false)
                .is_err());
            assert!(pa
                .eligible_for_period(date("10/08/2026"), date("06/09/2026"), true)
                .unwrap());
        }
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
        assert!(assistant
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
        assert_eq!(saved.date_of_birth.as_deref(), Some("1990-02-01"));
        assert_eq!(
            saved.national_insurance_number,
            assistant.national_insurance_number
        );
        assert_eq!(saved.address, assistant.address);
        assert_eq!(saved.postcode, assistant.postcode);
        assert_eq!(saved.telephone, assistant.telephone);
        assert_eq!(saved.email, assistant.email);
        assert_eq!(saved.employment_status, assistant.employment_status);
        assert_eq!(saved.mileage_enabled, assistant.mileage_enabled);
        assert_eq!(saved.start_date.as_deref(), Some("2025-04-03"));
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
