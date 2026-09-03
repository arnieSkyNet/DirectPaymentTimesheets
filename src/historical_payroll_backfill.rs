use std::path::Path;

use chrono::{Duration, NaiveDate};
use rusqlite::{params, Connection, Result};

const PAYROLL_YEAR: &str = "2026/27";
const REASON: &str = "Verified historical payroll backfill (2026-09-04)";

pub(crate) fn is_backfill_reason(reason: Option<&str>) -> bool {
    reason == Some(REASON)
}

struct HistoricalPayroll {
    name: &'static str,
    cycle: i64,
    first_week: &'static str,
    worked_minutes: [i64; 4],
    previous_cycle_minutes: i64,
    annual_leave: Option<(i64, i64)>,
    public_holidays: &'static [(i64, &'static str, i64)],
}

const FIGURES: &[HistoricalPayroll] = &[
    row(
        "Alder Example",
        1,
        "23/03/2026",
        [165, 225, 0, 315],
        0,
        None,
        &[],
    ),
    row(
        "Alder Example",
        2,
        "20/04/2026",
        [75, 255, 0, 0],
        0,
        None,
        &[],
    ),
    row(
        "Alder Example",
        3,
        "18/05/2026",
        [0, 135, 285, 345],
        0,
        None,
        &[],
    ),
    row(
        "Alder Example",
        4,
        "15/06/2026",
        [315, 270, 0, 345],
        60,
        Some((3, 240)),
        &[],
    ),
    row(
        "Birch Sample",
        1,
        "23/03/2026",
        [615, 735, 525, 645],
        150,
        None,
        &[(2, "03/04/2026", 240), (3, "06/04/2026", 360)],
    ),
    row(
        "Birch Sample",
        2,
        "20/04/2026",
        [705, 555, 825, 465],
        90,
        None,
        &[(3, "04/05/2026", 180)],
    ),
    row(
        "Birch Sample",
        3,
        "18/05/2026",
        [585, 765, 675, 435],
        120,
        None,
        &[(2, "25/05/2026", 240)],
    ),
    row(
        "Birch Sample",
        4,
        "15/06/2026",
        [675, 555, 0, 615],
        120,
        Some((3, 420)),
        &[],
    ),
    row(
        "Cedar Fixture",
        1,
        "23/03/2026",
        [195, 330, 270, 210],
        75,
        None,
        &[],
    ),
    row(
        "Cedar Fixture",
        2,
        "20/04/2026",
        [360, 180, 240, 300],
        45,
        None,
        &[],
    ),
    row(
        "Cedar Fixture",
        3,
        "18/05/2026",
        [405, 150, 390, 120],
        90,
        None,
        &[],
    ),
    row(
        "Cedar Fixture",
        4,
        "15/06/2026",
        [300, 240, 630, 270],
        0,
        None,
        &[],
    ),
];

const fn row(
    name: &'static str,
    cycle: i64,
    first_week: &'static str,
    worked_minutes: [i64; 4],
    previous_cycle_minutes: i64,
    annual_leave: Option<(i64, i64)>,
    public_holidays: &'static [(i64, &'static str, i64)],
) -> HistoricalPayroll {
    HistoricalPayroll {
        name,
        cycle,
        first_week,
        worked_minutes,
        previous_cycle_minutes,
        annual_leave,
        public_holidays,
    }
}

pub fn apply(database_path: &Path) -> Result<()> {
    let mut connection = Connection::open(database_path)?;
    apply_to_connection(&mut connection)
}

fn apply_to_connection(connection: &mut Connection) -> Result<()> {
    let transaction = connection.transaction()?;
    let completed: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM payroll_timesheet_manual_adjustments WHERE reason = ?1",
        [REASON],
        |row| row.get(0),
    )?;
    if completed == (FIGURES.len() * 4) as i64 {
        synchronise_known_public_holidays(&transaction)?;
        transaction.commit()?;
        return Ok(());
    }
    if completed != 0 {
        return Err(rusqlite::Error::InvalidParameterName(
            "The historical payroll backfill is only partially present; no data was changed."
                .to_string(),
        ));
    }

    let schedules: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM payroll_schedules
         WHERE payroll_year = ?1 AND cycle_number BETWEEN 1 AND 4",
        [PAYROLL_YEAR],
        |row| row.get(0),
    )?;
    let assistants: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM personal_assistants
         WHERE (first_name = 'Alder' AND surname = 'Example')
            OR (first_name = 'Birch' AND surname = 'Sample')
            OR (first_name = 'Cedar' AND surname = 'Fixture')",
        [],
        |row| row.get(0),
    )?;
    if schedules == 0 && assistants == 0 {
        transaction.commit()?;
        return Ok(());
    }
    if schedules != 4 || assistants != 3 {
        return Err(rusqlite::Error::InvalidParameterName(
            "The historical payroll source schedules or assistants are incomplete; no data was changed."
                .to_string(),
        ));
    }

    for figure in FIGURES {
        apply_figure(&transaction, figure)?;
    }
    synchronise_known_public_holidays(&transaction)?;
    transaction.commit()
}

fn synchronise_known_public_holidays(connection: &Connection) -> Result<()> {
    for figure in FIGURES
        .iter()
        .filter(|figure| !figure.public_holidays.is_empty())
    {
        let (first_name, surname) = figure.name.split_once(' ').expect("fixed full name");
        let payroll_timesheet_id: i64 = connection.query_row(
            "SELECT pt.id
             FROM payroll_timesheets AS pt
             JOIN personal_assistants AS pa ON pa.id = pt.personal_assistant_id
             WHERE pa.first_name = ?1 AND pa.surname = ?2
               AND pt.payroll_year = ?3 AND pt.cycle_number = ?4",
            params![first_name, surname, PAYROLL_YEAR, figure.cycle],
            |row| row.get(0),
        )?;
        for (week_number, date, minutes) in figure.public_holidays {
            connection.execute(
                "INSERT INTO payroll_timesheet_public_holidays
                    (payroll_timesheet_id, week_number, holiday_date, hours)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(payroll_timesheet_id, week_number, holiday_date)
                 DO UPDATE SET hours = excluded.hours",
                params![
                    payroll_timesheet_id,
                    week_number,
                    date,
                    *minutes as f64 / 60.0
                ],
            )?;
            let changed = connection.execute(
                "UPDATE payroll_timesheet_weeks
                 SET public_holiday_hours = (
                     SELECT COALESCE(SUM(hours), 0)
                     FROM payroll_timesheet_public_holidays
                     WHERE payroll_timesheet_id = ?1 AND week_number = ?2
                 )
                 WHERE payroll_timesheet_id = ?1 AND week_number = ?2",
                params![payroll_timesheet_id, week_number],
            )?;
            if changed != 1 {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
        }
    }
    Ok(())
}

fn apply_figure(connection: &Connection, figure: &HistoricalPayroll) -> Result<()> {
    let (first_name, surname) = figure.name.split_once(' ').expect("fixed full name");
    let assistant_ids = connection
        .prepare(
            "SELECT id FROM personal_assistants WHERE first_name = ?1 AND surname = ?2 ORDER BY id",
        )?
        .query_map(params![first_name, surname], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>>>()?;
    if assistant_ids.len() != 1 {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "Historical payroll requires exactly one personal assistant named {}.",
            figure.name
        )));
    }
    let personal_assistant_id = assistant_ids[0];
    let stored_first_week: String = connection.query_row(
        "SELECT first_week_commencing FROM payroll_schedules WHERE payroll_year = ?1 AND cycle_number = ?2",
        params![PAYROLL_YEAR, figure.cycle],
        |row| row.get(0),
    )?;
    if stored_first_week != figure.first_week {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "Historical payroll cycle {} has unexpected first week {}.",
            figure.cycle, stored_first_week
        )));
    }

    let now = "2026-09-04 00:00:00";
    connection.execute(
        "INSERT INTO payroll_timesheets (personal_assistant_id, payroll_year, cycle_number, previous_cycle_hours, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)
         ON CONFLICT(personal_assistant_id, payroll_year, cycle_number) DO UPDATE SET
             previous_cycle_hours = excluded.previous_cycle_hours, updated_at = excluded.updated_at",
        params![personal_assistant_id, PAYROLL_YEAR, figure.cycle,
            (figure.previous_cycle_minutes > 0).then(|| figure.previous_cycle_minutes as f64 / 60.0), now],
    )?;
    let payroll_timesheet_id: i64 = connection.query_row(
        "SELECT id FROM payroll_timesheets WHERE personal_assistant_id = ?1 AND payroll_year = ?2 AND cycle_number = ?3",
        params![personal_assistant_id, PAYROLL_YEAR, figure.cycle], |row| row.get(0),
    )?;
    let first_week = NaiveDate::parse_from_str(figure.first_week, "%d/%m/%Y").map_err(|_| {
        rusqlite::Error::InvalidParameterName("Invalid fixed historical date".to_string())
    })?;

    for (index, target_minutes) in figure.worked_minutes.iter().enumerate() {
        let week_number = index as i64 + 1;
        let week_start = first_week + Duration::days(index as i64 * 7);
        let raw_minutes = raw_minutes_for_week(connection, personal_assistant_id, week_start)?;
        connection.execute(
            "INSERT INTO payroll_timesheet_weeks (payroll_timesheet_id, week_number, week_commencing, worked_hours, annual_leave_hours, sick_leave_hours, public_holiday_hours, travel_miles)
             VALUES (?1, ?2, ?3, ?4, 0, 0, 0, 0)
             ON CONFLICT(payroll_timesheet_id, week_number) DO UPDATE SET worked_hours = excluded.worked_hours",
            params![payroll_timesheet_id, week_number, week_start.format("%d/%m/%Y").to_string(), *target_minutes as f64 / 60.0],
        )?;
        connection.execute(
            "INSERT INTO payroll_timesheet_manual_adjustments (payroll_timesheet_id, week_number, adjustment_minutes, reason, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![payroll_timesheet_id, week_number, target_minutes - raw_minutes, REASON, now],
        )?;
    }

    if let Some((week_number, minutes)) = figure.annual_leave {
        connection.execute(
            "UPDATE payroll_timesheet_weeks SET annual_leave_hours = ?1 WHERE payroll_timesheet_id = ?2 AND week_number = ?3",
            params![minutes as f64 / 60.0, payroll_timesheet_id, week_number],
        )?;
    }
    Ok(())
}

fn raw_minutes_for_week(
    connection: &Connection,
    personal_assistant_id: i64,
    start: NaiveDate,
) -> Result<i64> {
    let mut statement = connection.prepare(
        "SELECT start_time, worked_minutes FROM timesheets WHERE personal_assistant_id = ?1 AND worked_minutes > 0",
    )?;
    let entries = statement.query_map([personal_assistant_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut total = 0;
    for entry in entries {
        let (start_time, minutes) = entry?;
        if crate::pay_rate_allocation::parse_timesheet_date(&start_time)
            .is_some_and(|date| date >= start && date <= start + Duration::days(6))
        {
            total += minutes;
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backfills_every_supplied_value_idempotently_without_changing_later_cycles_or_raw_shifts() {
        let mut connection = Connection::open_in_memory().unwrap();
        crate::database::create_schema(&connection).unwrap();
        for (first, surname) in [
            ("Alder", "Example"),
            ("Birch", "Sample"),
            ("Cedar", "Fixture"),
        ] {
            connection
                .execute(
                    "INSERT INTO personal_assistants (first_name, surname) VALUES (?1, ?2)",
                    params![first, surname],
                )
                .unwrap();
        }
        for (cycle, first_week) in [
            (1, "23/03/2026"),
            (2, "20/04/2026"),
            (3, "18/05/2026"),
            (4, "15/06/2026"),
            (5, "13/07/2026"),
            (6, "10/08/2026"),
        ] {
            connection.execute("INSERT INTO payroll_schedules (payroll_year, cycle_number, first_week_commencing, latest_posting_date, pay_date, created_at) VALUES (?1, ?2, ?3, 'x', 'x', 'x')", params![PAYROLL_YEAR, cycle, first_week]).unwrap();
        }
        connection.execute("INSERT INTO timesheets (pa_name, personal_assistant_id, start_time, end_time, break_minutes, worked_minutes, hourly_rate, amount) VALUES ('Alder Example', 1, '6 July 2026 at 09:00:00', 'x', 0, 360, 1, 6)", []).unwrap();
        connection.execute("INSERT INTO payroll_timesheets (personal_assistant_id, payroll_year, cycle_number, previous_cycle_hours, created_at, updated_at) VALUES (1, ?1, 5, NULL, 'x', 'x')", [PAYROLL_YEAR]).unwrap();
        let later_id = connection.last_insert_rowid();
        connection.execute("INSERT INTO payroll_timesheet_weeks (payroll_timesheet_id, week_number, week_commencing, worked_hours) VALUES (?1, 1, '13/07/2026', 99)", [later_id]).unwrap();
        connection.execute("INSERT INTO payroll_timesheets (personal_assistant_id, payroll_year, cycle_number, previous_cycle_hours, created_at, updated_at) VALUES (1, ?1, 6, 7.5, 'x', 'x')", [PAYROLL_YEAR]).unwrap();
        let week_22_id = connection.last_insert_rowid();
        connection.execute("INSERT INTO payroll_timesheet_weeks (payroll_timesheet_id, week_number, week_commencing, worked_hours) VALUES (?1, 1, '10/08/2026', 88)", [week_22_id]).unwrap();

        apply_to_connection(&mut connection).unwrap();
        connection
            .execute(
                "UPDATE payroll_timesheet_weeks
                 SET public_holiday_hours = 0
                 WHERE payroll_timesheet_id IN (
                     SELECT id FROM payroll_timesheets WHERE cycle_number IN (1, 2, 3)
                 )",
                [],
            )
            .unwrap();
        apply_to_connection(&mut connection).unwrap();

        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM timesheets", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(connection.query_row("SELECT worked_hours FROM payroll_timesheet_weeks WHERE payroll_timesheet_id = ?1", [later_id], |r| r.get::<_, f64>(0)).unwrap(), 99.0);
        assert_eq!(connection.query_row("SELECT worked_hours FROM payroll_timesheet_weeks WHERE payroll_timesheet_id = ?1", [week_22_id], |r| r.get::<_, f64>(0)).unwrap(), 88.0);
        assert_eq!(
            connection
                .query_row(
                    "SELECT previous_cycle_hours FROM payroll_timesheets WHERE id = ?1",
                    [week_22_id],
                    |r| r.get::<_, f64>(0)
                )
                .unwrap(),
            7.5
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM payroll_timesheet_manual_adjustments WHERE reason = ?1",
                    [REASON],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            48
        );
        for figure in FIGURES {
            let (first, surname) = figure.name.split_once(' ').unwrap();
            let id: i64 = connection.query_row("SELECT pt.id FROM payroll_timesheets pt JOIN personal_assistants pa ON pa.id=pt.personal_assistant_id WHERE pa.first_name=?1 AND pa.surname=?2 AND pt.payroll_year=?3 AND pt.cycle_number=?4", params![first, surname, PAYROLL_YEAR, figure.cycle], |r| r.get(0)).unwrap();
            let previous: Option<f64> = connection
                .query_row(
                    "SELECT previous_cycle_hours FROM payroll_timesheets WHERE id=?1",
                    [id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(
                previous,
                (figure.previous_cycle_minutes > 0)
                    .then(|| figure.previous_cycle_minutes as f64 / 60.0)
            );
            for (index, expected) in figure.worked_minutes.iter().enumerate() {
                let actual: f64 = connection.query_row("SELECT worked_hours FROM payroll_timesheet_weeks WHERE payroll_timesheet_id=?1 AND week_number=?2", params![id, index as i64 + 1], |r| r.get(0)).unwrap();
                assert_eq!(
                    actual,
                    *expected as f64 / 60.0,
                    "{} cycle {} week {}",
                    figure.name,
                    figure.cycle,
                    index + 1
                );
                let adjustment: i64 = connection.query_row("SELECT adjustment_minutes FROM payroll_timesheet_manual_adjustments WHERE payroll_timesheet_id=?1 AND week_number=?2", params![id, index as i64 + 1], |r| r.get(0)).unwrap();
                let week_start = NaiveDate::parse_from_str(figure.first_week, "%d/%m/%Y").unwrap()
                    + Duration::days(index as i64 * 7);
                assert_eq!(
                    raw_minutes_for_week(
                        &connection,
                        assistant_id(&connection, first, surname),
                        week_start
                    )
                    .unwrap()
                        + adjustment,
                    *expected
                );
            }
            if let Some((week, minutes)) = figure.annual_leave {
                let actual: f64 = connection.query_row("SELECT annual_leave_hours FROM payroll_timesheet_weeks WHERE payroll_timesheet_id=?1 AND week_number=?2", params![id, week], |r| r.get(0)).unwrap();
                assert_eq!(actual, minutes as f64 / 60.0);
            }
            for (week, date, minutes) in figure.public_holidays {
                let actual: f64 = connection.query_row("SELECT hours FROM payroll_timesheet_public_holidays WHERE payroll_timesheet_id=?1 AND week_number=?2 AND holiday_date=?3", params![id, week, date], |r| r.get(0)).unwrap();
                assert_eq!(actual, *minutes as f64 / 60.0);
                let aggregate: f64 = connection.query_row("SELECT public_holiday_hours FROM payroll_timesheet_weeks WHERE payroll_timesheet_id=?1 AND week_number=?2", params![id, week], |r| r.get(0)).unwrap();
                let detail_total: f64 = connection.query_row("SELECT SUM(hours) FROM payroll_timesheet_public_holidays WHERE payroll_timesheet_id=?1 AND week_number=?2", params![id, week], |r| r.get(0)).unwrap();
                assert_eq!(aggregate, detail_total);
            }
        }
        let angus_cycle_four_adjustment: i64 = connection.query_row("SELECT a.adjustment_minutes FROM payroll_timesheet_manual_adjustments a JOIN payroll_timesheets pt ON pt.id=a.payroll_timesheet_id WHERE pt.personal_assistant_id=1 AND pt.cycle_number=4 AND a.week_number=4", [], |r| r.get(0)).unwrap();
        assert_eq!(angus_cycle_four_adjustment, 15);
    }

    fn assistant_id(connection: &Connection, first: &str, surname: &str) -> i64 {
        connection
            .query_row(
                "SELECT id FROM personal_assistants WHERE first_name=?1 AND surname=?2",
                params![first, surname],
                |row| row.get(0),
            )
            .unwrap()
    }
}
