use chrono::NaiveDate;
use rusqlite::{params, Connection, Result};
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone)]
pub struct PayrollSchedule {
    pub id: i64,
    pub payroll_year: String,
    pub cycle_number: i64,
    pub first_week_commencing: String,
    pub latest_posting_date: String,
    pub pay_date: String,
    pub created_at: String,
    pub payslips_sent: bool,
}

pub struct PayrollScheduleRepository {
    connection: Connection,
}

#[derive(Debug)]
pub enum PayrollScheduleResolutionError {
    Database(rusqlite::Error),
    InvalidStoredDate {
        schedule_id: i64,
        value: String,
    },
    NoCurrentCycle(NaiveDate),
    AmbiguousCurrentCycle(NaiveDate, Vec<(String, i64)>),
    AmbiguousPreviousCycle {
        payroll_year: String,
        cycle_number: i64,
        matches: Vec<(String, i64)>,
    },
}

impl fmt::Display for PayrollScheduleResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "Failed reading payroll schedules: {error}"),
            Self::InvalidStoredDate { schedule_id, value } => write!(
                formatter,
                "Payroll schedule {schedule_id} has an invalid first week commencing date: '{value}'."
            ),
            Self::NoCurrentCycle(date) => write!(
                formatter,
                "No imported payroll cycle contains {}.",
                date.format("%d/%m/%Y")
            ),
            Self::AmbiguousCurrentCycle(date, matches) => {
                let descriptions = matches
                    .iter()
                    .map(|(year, cycle)| format!("{year} cycle {cycle}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    formatter,
                    "More than one imported payroll cycle contains {}: {descriptions}.",
                    date.format("%d/%m/%Y")
                )
            }
            Self::AmbiguousPreviousCycle {
                payroll_year,
                cycle_number,
                matches,
            } => {
                let descriptions = matches
                    .iter()
                    .map(|(year, cycle)| format!("{year} cycle {cycle}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    formatter,
                    "More than one imported payroll cycle immediately precedes {payroll_year} cycle {cycle_number}: {descriptions}."
                )
            }
        }
    }
}

impl Error for PayrollScheduleResolutionError {}

impl From<rusqlite::Error> for PayrollScheduleResolutionError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

impl PayrollScheduleRepository {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    pub fn get_all_for_year(&self, payroll_year: &str) -> Result<Vec<PayrollSchedule>> {
        let mut statement = self.connection.prepare(
            "
            SELECT
                id,
                payroll_year,
                cycle_number,
                first_week_commencing,
                latest_posting_date,
                pay_date,
                created_at,
                payslips_sent
            FROM payroll_schedules
            WHERE payroll_year = ?1
            ORDER BY cycle_number
            ",
        )?;

        let entries = statement.query_map(params![payroll_year], |row| {
            Ok(PayrollSchedule {
                id: row.get(0)?,
                payroll_year: row.get(1)?,
                cycle_number: row.get(2)?,
                first_week_commencing: row.get(3)?,
                latest_posting_date: row.get(4)?,
                pay_date: row.get(5)?,
                created_at: row.get(6)?,
                payslips_sent: row.get::<_, i64>(7)? != 0,
            })
        })?;

        entries.collect()
    }

    pub fn get_all(&self) -> Result<Vec<PayrollSchedule>> {
        let mut statement = self.connection.prepare(
            "
            SELECT
                id,
                payroll_year,
                cycle_number,
                first_week_commencing,
                latest_posting_date,
                pay_date,
                created_at,
                payslips_sent
            FROM payroll_schedules
            ORDER BY payroll_year, cycle_number
            ",
        )?;

        let entries = statement.query_map([], |row| {
            Ok(PayrollSchedule {
                id: row.get(0)?,
                payroll_year: row.get(1)?,
                cycle_number: row.get(2)?,
                first_week_commencing: row.get(3)?,
                latest_posting_date: row.get(4)?,
                pay_date: row.get(5)?,
                created_at: row.get(6)?,
                payslips_sent: row.get::<_, i64>(7)? != 0,
            })
        })?;

        entries.collect()
    }

    pub fn get_for_year_and_cycle(
        &self,
        payroll_year: &str,
        cycle_number: i64,
    ) -> Result<Option<PayrollSchedule>> {
        use rusqlite::OptionalExtension;

        self.connection
            .query_row(
                "SELECT id, payroll_year, cycle_number, first_week_commencing,
                        latest_posting_date, pay_date, created_at, payslips_sent
                 FROM payroll_schedules
                 WHERE payroll_year = ?1 AND cycle_number = ?2",
                params![payroll_year, cycle_number],
                |row| {
                    Ok(PayrollSchedule {
                        id: row.get(0)?,
                        payroll_year: row.get(1)?,
                        cycle_number: row.get(2)?,
                        first_week_commencing: row.get(3)?,
                        latest_posting_date: row.get(4)?,
                        pay_date: row.get(5)?,
                        created_at: row.get(6)?,
                        payslips_sent: row.get::<_, i64>(7)? != 0,
                    })
                },
            )
            .optional()
    }

    pub fn resolve_for_date(
        &self,
        date: NaiveDate,
    ) -> std::result::Result<PayrollSchedule, PayrollScheduleResolutionError> {
        let mut statement = self.connection.prepare(
            "SELECT id, payroll_year, cycle_number, first_week_commencing,
                    latest_posting_date, pay_date, created_at, payslips_sent
             FROM payroll_schedules
             ORDER BY payroll_year, cycle_number",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(PayrollSchedule {
                id: row.get(0)?,
                payroll_year: row.get(1)?,
                cycle_number: row.get(2)?,
                first_week_commencing: row.get(3)?,
                latest_posting_date: row.get(4)?,
                pay_date: row.get(5)?,
                created_at: row.get(6)?,
                payslips_sent: row.get::<_, i64>(7)? != 0,
            })
        })?;

        let mut matches = Vec::new();
        for row in rows {
            let schedule = row?;
            let first_week = crate::date_utils::parse_legacy(&schedule.first_week_commencing)
                .map_err(|_| PayrollScheduleResolutionError::InvalidStoredDate {
                    schedule_id: schedule.id,
                    value: schedule.first_week_commencing.clone(),
                })?;
            if first_week <= date && date <= first_week + chrono::Duration::days(27) {
                matches.push(schedule);
            }
        }

        match matches.len() {
            0 => Err(PayrollScheduleResolutionError::NoCurrentCycle(date)),
            1 => Ok(matches.remove(0)),
            _ => Err(PayrollScheduleResolutionError::AmbiguousCurrentCycle(
                date,
                matches
                    .iter()
                    .map(|schedule| (schedule.payroll_year.clone(), schedule.cycle_number))
                    .collect(),
            )),
        }
    }

    pub fn resolve_previous(
        &self,
        current: &PayrollSchedule,
    ) -> std::result::Result<Option<PayrollSchedule>, PayrollScheduleResolutionError> {
        let current_first_week = crate::date_utils::parse_legacy(&current.first_week_commencing)
            .map_err(|_| PayrollScheduleResolutionError::InvalidStoredDate {
                schedule_id: current.id,
                value: current.first_week_commencing.clone(),
            })?;

        let mut statement = self.connection.prepare(
            "SELECT id, payroll_year, cycle_number, first_week_commencing,
                    latest_posting_date, pay_date, created_at, payslips_sent
             FROM payroll_schedules
             ORDER BY payroll_year, cycle_number",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(PayrollSchedule {
                id: row.get(0)?,
                payroll_year: row.get(1)?,
                cycle_number: row.get(2)?,
                first_week_commencing: row.get(3)?,
                latest_posting_date: row.get(4)?,
                pay_date: row.get(5)?,
                created_at: row.get(6)?,
                payslips_sent: row.get::<_, i64>(7)? != 0,
            })
        })?;

        let mut matches = Vec::new();
        for row in rows {
            let schedule = row?;
            let first_week = crate::date_utils::parse_legacy(&schedule.first_week_commencing)
                .map_err(|_| PayrollScheduleResolutionError::InvalidStoredDate {
                    schedule_id: schedule.id,
                    value: schedule.first_week_commencing.clone(),
                })?;
            if first_week + chrono::Duration::days(28) == current_first_week {
                matches.push(schedule);
            }
        }

        match matches.len() {
            0 => Ok(None),
            1 => Ok(matches.pop()),
            _ => Err(PayrollScheduleResolutionError::AmbiguousPreviousCycle {
                payroll_year: current.payroll_year.clone(),
                cycle_number: current.cycle_number,
                matches: matches
                    .iter()
                    .map(|schedule| (schedule.payroll_year.clone(), schedule.cycle_number))
                    .collect(),
            }),
        }
    }

    pub fn replace_year_atomically(
        &self,
        payroll_year: &str,
        schedules: &[PayrollSchedule],
    ) -> std::result::Result<usize, Box<dyn Error>> {
        let transaction = self.connection.unchecked_transaction()?;
        let existing = {
            let mut statement = transaction.prepare(
                "SELECT id, payroll_year, cycle_number, first_week_commencing,
                        latest_posting_date, pay_date, created_at, payslips_sent
                 FROM payroll_schedules
                 WHERE payroll_year = ?1
                 ORDER BY cycle_number",
            )?;
            let rows = statement.query_map(params![payroll_year], |row| {
                Ok(PayrollSchedule {
                    id: row.get(0)?,
                    payroll_year: row.get(1)?,
                    cycle_number: row.get(2)?,
                    first_week_commencing: row.get(3)?,
                    latest_posting_date: row.get(4)?,
                    pay_date: row.get(5)?,
                    created_at: row.get(6)?,
                    payslips_sent: row.get::<_, i64>(7)? != 0,
                })
            })?;
            rows.collect::<Result<Vec<_>>>()?
        };

        let dates_changed = existing.len() != schedules.len()
            || schedules.iter().any(|schedule| {
                existing
                    .iter()
                    .find(|current| current.cycle_number == schedule.cycle_number)
                    .is_none_or(|current| !same_schedule_dates(current, schedule))
            });

        if dates_changed {
            let prepared_count: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM payroll_timesheets WHERE payroll_year = ?1",
                params![payroll_year],
                |row| row.get(0),
            )?;
            if prepared_count > 0 {
                return Err(format!(
                    "Cannot replace the {payroll_year} payroll schedule because its dates differ and {prepared_count} prepared payroll record(s) already exist."
                )
                .into());
            }
        }

        transaction.execute(
            "DELETE FROM payroll_schedules WHERE payroll_year = ?1",
            params![payroll_year],
        )?;

        for schedule in schedules {
            let unchanged = existing.iter().find(|current| {
                current.cycle_number == schedule.cycle_number
                    && same_schedule_dates(current, schedule)
            });
            let payslips_sent = unchanged
                .map(|current| current.payslips_sent)
                .unwrap_or(false);
            let created_at = unchanged
                .map(|current| current.created_at.as_str())
                .unwrap_or(&schedule.created_at);

            transaction.execute(
                "INSERT INTO payroll_schedules (
                    payroll_year, cycle_number, first_week_commencing,
                    latest_posting_date, pay_date, created_at, payslips_sent
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    schedule.payroll_year,
                    schedule.cycle_number,
                    unchanged
                        .map(|s| &s.first_week_commencing)
                        .unwrap_or(&schedule.first_week_commencing),
                    unchanged
                        .map(|s| &s.latest_posting_date)
                        .unwrap_or(&schedule.latest_posting_date),
                    unchanged.map(|s| &s.pay_date).unwrap_or(&schedule.pay_date),
                    created_at,
                    payslips_sent,
                ],
            )?;
        }

        transaction.commit()?;
        Ok(schedules.len())
    }

    pub fn mark_payslips_sent(&self, id: i64) -> Result<bool> {
        Ok(self.connection.execute(
            "
            UPDATE payroll_schedules
            SET payslips_sent = 1
            WHERE id = ?1
            ",
            params![id],
        )? == 1)
    }
}

fn same_schedule_dates(left: &PayrollSchedule, right: &PayrollSchedule) -> bool {
    crate::date_utils::same(&left.first_week_commencing, &right.first_week_commencing)
        && crate::date_utils::same(&left.latest_posting_date, &right.latest_posting_date)
        && crate::date_utils::same(&left.pay_date, &right.pay_date)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(value: &str) -> NaiveDate {
        crate::date_utils::parse_legacy(value).unwrap()
    }

    fn repository_with_schedules(schedules: &[(&str, i64, &str)]) -> PayrollScheduleRepository {
        let connection = Connection::open_in_memory().unwrap();
        crate::database::create_schema(&connection).unwrap();
        for (year, cycle, first_week) in schedules {
            let first = date(first_week);
            connection
                .execute(
                    "INSERT INTO payroll_schedules (
                        payroll_year, cycle_number, first_week_commencing,
                        latest_posting_date, pay_date, created_at, payslips_sent
                     ) VALUES (?1, ?2, ?3, ?4, ?5, 'created', 0)",
                    params![
                        year,
                        cycle,
                        first_week,
                        (first + chrono::Duration::days(18))
                            .format("%d/%m/%Y")
                            .to_string(),
                        (first + chrono::Duration::days(25))
                            .format("%d/%m/%Y")
                            .to_string(),
                    ],
                )
                .unwrap();
        }
        PayrollScheduleRepository::new(connection)
    }

    #[test]
    fn resolves_exact_schedule_across_two_payroll_years() {
        let repository = repository_with_schedules(&[
            ("2026/27", 10, "30/11/2026"),
            ("2027/28", 1, "22/03/2027"),
        ]);

        let old = repository.resolve_for_date(date("10/12/2026")).unwrap();
        let new = repository.resolve_for_date(date("30/03/2027")).unwrap();

        assert_eq!(
            (old.payroll_year.as_str(), old.cycle_number),
            ("2026/27", 10)
        );
        assert_eq!(
            (new.payroll_year.as_str(), new.cycle_number),
            ("2027/28", 1)
        );
        assert_eq!(new.latest_posting_date, "09/04/2027");
        assert_eq!(new.pay_date, "16/04/2027");
    }

    #[test]
    fn gets_complete_schedule_by_stable_year_and_cycle_identity() {
        let repository = repository_with_schedules(&[
            ("2026/27", 6, "10/08/2026"),
            ("2027/28", 6, "09/08/2027"),
        ]);

        let selected = repository
            .get_for_year_and_cycle("2026/27", 6)
            .unwrap()
            .unwrap();

        assert_eq!(selected.payroll_year, "2026/27");
        assert_eq!(selected.cycle_number, 6);
        assert_eq!(selected.first_week_commencing, "10/08/2026");
        assert_eq!(selected.latest_posting_date, "28/08/2026");
        assert_eq!(selected.pay_date, "04/09/2026");
        assert!(repository
            .get_for_year_and_cycle("2025/26", 6)
            .unwrap()
            .is_none());
    }

    #[test]
    fn future_year_import_does_not_change_december_resolution() {
        let repository = repository_with_schedules(&[
            ("2026/27", 10, "30/11/2026"),
            ("2027/28", 1, "22/03/2027"),
        ]);

        let resolved = repository.resolve_for_date(date("15/12/2026")).unwrap();

        assert_eq!(resolved.payroll_year, "2026/27");
        assert_eq!(resolved.cycle_number, 10);
    }

    #[test]
    fn schedule_driven_rollover_can_occur_before_first_of_april() {
        let repository = repository_with_schedules(&[
            ("2026/27", 13, "22/02/2027"),
            ("2027/28", 1, "22/03/2027"),
        ]);

        let resolved = repository.resolve_for_date(date("25/03/2027")).unwrap();

        assert_eq!(resolved.payroll_year, "2027/28");
    }

    #[test]
    fn schedule_driven_rollover_can_occur_after_first_of_april() {
        let repository = repository_with_schedules(&[
            ("2026/27", 13, "08/03/2027"),
            ("2027/28", 1, "05/04/2027"),
        ]);

        assert_eq!(
            repository
                .resolve_for_date(date("03/04/2027"))
                .unwrap()
                .payroll_year,
            "2026/27"
        );
        assert_eq!(
            repository
                .resolve_for_date(date("05/04/2027"))
                .unwrap()
                .payroll_year,
            "2027/28"
        );
    }

    #[test]
    fn reports_when_no_schedule_contains_date() {
        let repository = repository_with_schedules(&[("2027/28", 1, "05/04/2027")]);

        assert!(matches!(
            repository.resolve_for_date(date("01/04/2027")),
            Err(PayrollScheduleResolutionError::NoCurrentCycle(_))
        ));
    }

    #[test]
    fn refuses_overlapping_ambiguous_schedules() {
        let repository = repository_with_schedules(&[
            ("2026/27", 13, "08/03/2027"),
            ("2027/28", 1, "22/03/2027"),
        ]);

        assert!(matches!(
            repository.resolve_for_date(date("25/03/2027")),
            Err(PayrollScheduleResolutionError::AmbiguousCurrentCycle(_, matches))
                if matches == vec![("2026/27".to_string(), 13), ("2027/28".to_string(), 1)]
        ));
    }

    #[test]
    fn resolves_same_year_predecessor_by_first_week_chronology() {
        let repository = repository_with_schedules(&[
            ("2026/27", 5, "13/07/2026"),
            ("2026/27", 6, "10/08/2026"),
        ]);
        let current = repository.get_all_for_year("2026/27").unwrap().remove(1);

        let previous = repository.resolve_previous(&current).unwrap().unwrap();

        assert_eq!(
            (previous.payroll_year.as_str(), previous.cycle_number),
            ("2026/27", 5)
        );
    }

    #[test]
    fn resolves_cycle_thirteen_across_payroll_year_boundary() {
        let repository = repository_with_schedules(&[
            ("2026/27", 13, "22/02/2027"),
            ("2027/28", 1, "22/03/2027"),
        ]);
        let current = repository.get_all_for_year("2027/28").unwrap().remove(0);

        let previous = repository.resolve_previous(&current).unwrap().unwrap();

        assert_eq!(
            (previous.payroll_year.as_str(), previous.cycle_number),
            ("2026/27", 13)
        );
    }

    #[test]
    fn first_known_schedule_has_no_predecessor() {
        let repository = repository_with_schedules(&[("2027/28", 1, "22/03/2027")]);
        let current = repository.get_all_for_year("2027/28").unwrap().remove(0);

        assert!(repository.resolve_previous(&current).unwrap().is_none());
    }

    #[test]
    fn gap_does_not_falsely_resolve_a_predecessor() {
        let repository = repository_with_schedules(&[
            ("2026/27", 13, "15/02/2027"),
            ("2027/28", 1, "22/03/2027"),
        ]);
        let current = repository.get_all_for_year("2027/28").unwrap().remove(0);

        assert!(repository.resolve_previous(&current).unwrap().is_none());
    }

    #[test]
    fn ambiguous_predecessors_are_rejected() {
        let repository = repository_with_schedules(&[
            ("2026/27", 13, "22/02/2027"),
            ("legacy", 99, "22/02/2027"),
            ("2027/28", 1, "22/03/2027"),
        ]);
        let current = repository.get_all_for_year("2027/28").unwrap().remove(0);

        assert!(matches!(
            repository.resolve_previous(&current),
            Err(PayrollScheduleResolutionError::AmbiguousPreviousCycle { matches, .. })
                if matches == vec![("2026/27".to_string(), 13), ("legacy".to_string(), 99)]
        ));
    }

    #[test]
    fn cross_year_predecessor_identity_finds_existing_payroll_record_not_cycle_zero() {
        let repository = repository_with_schedules(&[
            ("2026/27", 13, "22/02/2027"),
            ("2027/28", 1, "22/03/2027"),
        ]);
        repository
            .connection
            .execute(
                "INSERT INTO payroll_timesheets (
                    personal_assistant_id, payroll_year, cycle_number,
                    previous_cycle_hours, created_at, updated_at
                 ) VALUES (1, '2026/27', 13, NULL, 'created', 'updated')",
                [],
            )
            .unwrap();
        let current = repository.get_all_for_year("2027/28").unwrap().remove(0);
        let previous = repository.resolve_previous(&current).unwrap().unwrap();

        let stored_identity: (String, i64) = repository
            .connection
            .query_row(
                "SELECT payroll_year, cycle_number
                 FROM payroll_timesheets
                 WHERE payroll_year = ?1 AND cycle_number = ?2",
                params![previous.payroll_year, previous.cycle_number],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(stored_identity, ("2026/27".to_string(), 13));
    }

    #[test]
    fn invalid_stored_predecessor_date_is_rejected() {
        let repository = repository_with_schedules(&[("2027/28", 1, "22/03/2027")]);
        repository
            .connection
            .execute(
                "INSERT INTO payroll_schedules (
                    payroll_year, cycle_number, first_week_commencing,
                    latest_posting_date, pay_date, created_at, payslips_sent
                 ) VALUES ('broken', 1, 'not-a-date', '', '', 'created', 0)",
                [],
            )
            .unwrap();
        let current = repository.get_all_for_year("2027/28").unwrap().remove(0);

        assert!(matches!(
            repository.resolve_previous(&current),
            Err(PayrollScheduleResolutionError::InvalidStoredDate { value, .. })
                if value == "not-a-date"
        ));
    }
}
