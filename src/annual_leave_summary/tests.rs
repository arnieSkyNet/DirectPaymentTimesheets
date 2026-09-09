use super::*;

fn date(value: &str) -> NaiveDate {
    checked_date(value, "test date").unwrap()
}
fn approx(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-8, "{actual} != {expected}");
}
fn entry(id: i64, effective: &str, basis: HoursBasis, hours: &str) -> ContractedHoursEntry {
    ContractedHoursEntry {
        id,
        personal_assistant_id: 1,
        effective_date: effective.into(),
        contracted_hours: hours.into(),
        hours_basis: basis,
        created_at: "test".into(),
    }
}
fn evidence(basis: HoursBasis) -> Evidence {
    Evidence {
        assistant: PersonalAssistant {
            id: 1,
            first_name: "Test".into(),
            surname: "PA".into(),
            date_of_birth: None,
            national_insurance_number: None,
            address: None,
            postcode: None,
            telephone: None,
            email: None,
            employment_status: Some("Active".into()),
            sick_pay_enabled: true,
            mileage_enabled: true,
            start_date: Some("01/04/2026".into()),
            leaving_date: None,
            signature: None,
        },
        settings: AnnualLeaveSettings::default(),
        history: vec![entry(1, "01/04/2026", basis, "20")],
        cycles: vec![],
        schedule_dates: vec![],
    }
}
fn cycle(number: i64, start: &str, items: Vec<WorkedItemSnapshot>) -> CycleEvidence {
    let start = date(start);
    CycleEvidence {
        record: PayrollTimesheet {
            id: number,
            personal_assistant_id: 1,
            payroll_year: "2026/27".into(),
            cycle_number: number,
            previous_cycle_hours: None,
            created_at: "test".into(),
            updated_at: "test".into(),
        },
        weeks: (0..4)
            .map(|i| PayrollTimesheetWeek {
                id: number * 4 + i,
                payroll_timesheet_id: number,
                week_number: i + 1,
                week_commencing: display(start + Duration::days(i * 7)),
                worked_hours: 0.0,
                annual_leave_hours: 0.0,
                sick_leave_hours: 0.0,
                public_holiday_hours: 0.0,
                travel_miles: 0.0,
            })
            .collect(),
        leave: vec![],
        items,
        sent: true,
        snapshot_state: Some(SnapshotState::Submitted),
        period_end: Some(start + Duration::days(27)),
    }
}
fn work(day: &str, minutes: i64) -> WorkedItemSnapshot {
    WorkedItemSnapshot {
        week_number: 1,
        source_type: "imported_shift".into(),
        timesheet_id: None,
        direct_shift_id: None,
        source_evidence: None,
        work_date: Some(day.into()),
        worked_minutes: minutes,
        pay_rate_id: Some(1),
        pay_rate_effective_date: Some("01/04/2026".into()),
        total_hourly_rate: Some(15.0),
        reason: None,
    }
}
fn leave(week: i64, day: &str, hours: f64) -> PayrollTimesheetAnnualLeave {
    PayrollTimesheetAnnualLeave {
        id: 1,
        payroll_timesheet_id: 1,
        week_number: week,
        leave_date: day.into(),
        hours,
        created_at: "test".into(),
        updated_at: "test".into(),
    }
}
fn summary(evidence: &Evidence) -> Summary {
    calculate(evidence, LeaveYear(2026), date("08/09/2026")).unwrap()
}

#[test]
fn fixed_april_year_and_march_inclusive_end() {
    assert_eq!(LeaveYear::containing(date("08/09/2026")), LeaveYear(2026));
    assert_eq!(LeaveYear::containing(date("31/03/2027")), LeaveYear(2026));
    assert_eq!(LeaveYear::containing(date("01/04/2027")), LeaveYear(2027));
    assert_eq!(LeaveYear(2026).end(), date("31/03/2027"));
    assert_eq!(LeaveYear(2026).label(), "2026/27");
    assert_eq!(
        (LeaveYear(2023).end() - LeaveYear(2023).start()).num_days() + 1,
        366
    );
}
#[test]
fn independent_rule_dates_never_change_leave_year_or_pa_basis() {
    let mut e = evidence(HoursBasis::Contracted);
    e.settings.variable_effective_from = "01/01".into();
    e.settings.contracted_effective_from = "15/09".into();
    let s = summary(&e);
    approx(s.entitlement, 112.0);
    assert!(!s.variable);
    assert_eq!(
        rule_effective_on("15/09", date("01/04/2026")).unwrap(),
        date("15/09/2025")
    );
    e.history[0].hours_basis = HoursBasis::Variable;
    e.cycles
        .push(cycle(1, "01/04/2026", vec![work("02/04/2026", 6000)]));
    approx(summary(&e).entitlement, 12.0);
}
#[test]
fn historic_selector_uses_employment_and_evidence_without_empty_decades() {
    let mut e = evidence(HoursBasis::Contracted);
    e.assistant.start_date = Some("17/05/2024".into());
    assert_eq!(
        e.years(date("08/09/2026")),
        vec![LeaveYear(2026), LeaveYear(2025), LeaveYear(2024)]
    );
    e.assistant.start_date = None;
    e.schedule_dates = vec![date("01/04/2024")];
    assert_eq!(
        e.years(date("08/09/2026")),
        vec![LeaveYear(2026), LeaveYear(2024)]
    );
    e.cycles
        .push(cycle(1, "25/03/2024", vec![work("31/03/2023", 60)]));
    assert!(e.years(date("08/09/2026")).contains(&LeaveYear(2022)));
}
#[test]
fn full_and_part_year_contracted_employment_is_inclusive() {
    let mut e = evidence(HoursBasis::Contracted);
    approx(summary(&e).entitlement, 112.0);
    e.assistant.start_date = Some("01/07/2026".into());
    e.assistant.leaving_date = Some("30/09/2026".into());
    approx(summary(&e).entitlement, 112.0 * 92.0 / 365.0);
    e.assistant.leaving_date = e.assistant.start_date.clone();
    approx(summary(&e).entitlement, 112.0 / 365.0);
    assert_eq!(e.assistant.employment_status.as_deref(), Some("Active"));
}
#[test]
fn wholly_outside_employment_and_invalid_reversed_dates() {
    let mut e = evidence(HoursBasis::Contracted);
    e.assistant.start_date = Some("01/04/2027".into());
    approx(summary(&e).entitlement, 0.0);
    e.assistant.start_date = None;
    e.assistant.leaving_date = Some("31/03/2026".into());
    approx(summary(&e).entitlement, 0.0);
    e.assistant.start_date = Some("01/04/2026".into());
    assert!(calculate(&e, LeaveYear(2026), date("08/09/2026")).is_err());
}
#[test]
fn midweek_hours_changes_use_actual_dates_and_highest_id() {
    let mut e = evidence(HoursBasis::Contracted);
    e.history
        .push(entry(3, "17/06/2026", HoursBasis::Contracted, "30"));
    e.history
        .push(entry(2, "17/06/2026", HoursBasis::Contracted, "99"));
    let s = summary(&e);
    let earlier = (date("17/06/2026") - date("01/04/2026")).num_days() as f64;
    approx(
        s.entitlement,
        (20.0 * earlier + 30.0 * (365.0 - earlier)) * 5.6 / 365.0,
    );
    assert_eq!(s.contracted_parts.len(), 2);
    assert_eq!(s.contracted_parts[0].end, date("16/06/2026"));
    assert_eq!(s.contracted_parts[1].start, date("17/06/2026"));
}
#[test]
fn mixed_multiple_changes_only_accrue_variable_work_dates() {
    let mut e = evidence(HoursBasis::Contracted);
    e.history.extend([
        entry(2, "08/04/2026", HoursBasis::Variable, ""),
        entry(3, "15/04/2026", HoursBasis::Contracted, "10"),
        entry(4, "22/04/2026", HoursBasis::Variable, ""),
    ]);
    e.cycles.push(cycle(
        1,
        "01/04/2026",
        vec![
            work("07/04/2026", 6000),
            work("08/04/2026", 3000),
            work("14/04/2026", 3000),
            work("15/04/2026", 6000),
            work("22/04/2026", 3000),
        ],
    ));
    let s = summary(&e);
    approx(
        s.entitlement,
        (20.0 * 7.0 + 10.0 * 7.0) * 5.6 / 365.0 + 18.0,
    );
    assert_eq!(s.label(), "Annual entitlement/accrued");
    assert_eq!(s.calculated_to, Some(date("28/04/2026")));
}
#[test]
fn per_period_rounding_not_per_shift_week_or_grand_total() {
    let mut e = evidence(HoursBasis::Variable);
    // Each 2h shift accrues .2414h; each period's 6h accrues .7242 => 1h.
    for (n, start) in [(1, "01/04/2026"), (2, "29/04/2026")] {
        e.cycles.push(cycle(
            n,
            start,
            vec![work(start, 120), work(start, 120), work(start, 120)],
        ));
    }
    approx(summary(&e).entitlement, 2.0); // grand total would round 1.4484 to 1
    approx(round_variable(4.0, 12.07), 0.0);
    approx(round_variable(248.0 / 60.0, 12.07), 0.0);
    approx(round_variable(249.0 / 60.0, 12.07), 1.0);
    approx(round_variable(5000.0, 12.07), 604.0); // 603.5 at the default rate
    approx(round_variable(100.0, 12.5), 13.0); // exact half
    approx(round_variable(50.0, 12.07), 6.0);
    approx(round_variable(54.0, 12.07), 7.0);
}
#[test]
fn unsent_indeterminate_and_missing_submitted_evidence_do_not_accrue() {
    let mut e = evidence(HoursBasis::Variable);
    let mut c = cycle(1, "01/04/2026", vec![work("01/04/2026", 6000)]);
    c.sent = false;
    e.cycles.push(c);
    approx(summary(&e).entitlement, 0.0);
    e.cycles[0].sent = true;
    e.cycles[0].weeks[0].worked_hours = 100.0;
    for state in [
        None,
        Some(SnapshotState::Candidate),
        Some(SnapshotState::Indeterminate),
    ] {
        e.cycles[0].snapshot_state = state;
        let s = summary(&e);
        approx(s.entitlement, 0.0);
        assert!(!s.incomplete.is_empty());
    }
    e.cycles[0].snapshot_state = Some(SnapshotState::Submitted);
    approx(summary(&e).entitlement, 12.0);
}
#[test]
fn leave_sickness_public_holiday_and_mileage_are_not_work() {
    let mut e = evidence(HoursBasis::Variable);
    let mut c = cycle(1, "01/04/2026", vec![work("01/04/2026", 6000)]);
    c.weeks[0].annual_leave_hours = 100.0;
    c.weeks[0].sick_leave_hours = 100.0;
    c.weeks[0].public_holiday_hours = 100.0;
    c.weeks[0].travel_miles = 100.0;
    e.cycles.push(c);
    let s = summary(&e);
    approx(s.entitlement, 12.0);
    approx(s.taken, 100.0);
    approx(s.remaining(), -88.0);
    assert!(s
        .incomplete
        .iter()
        .any(|w| w.contains("additional sickness accrual calculation required")));
    e.history[0].hours_basis = HoursBasis::Contracted;
    assert!(!summary(&e)
        .incomplete
        .iter()
        .any(|w| w.contains("Sick/SSP")));
}
#[test]
fn late_shifts_keep_actual_dates_including_previous_leave_year() {
    let mut e = evidence(HoursBasis::Variable);
    e.assistant.start_date = Some("01/04/2025".into());
    e.history[0].effective_date = "01/04/2025".into();
    let mut late = work("31/03/2026", 6000);
    late.source_type = "previous_cycle_late_shift".into();
    late.week_number = 0;
    e.cycles
        .push(cycle(1, "01/04/2026", vec![late, work("01/04/2026", 3000)]));
    approx(summary(&e).entitlement, 6.0);
    let historic = calculate(&e, LeaveYear(2025), date("08/09/2026")).unwrap();
    approx(historic.entitlement, 12.0);
    assert_eq!(historic.calculated_to, Some(date("28/04/2026")));
}
#[test]
fn variable_employment_dates_inclusive_and_no_future_projection() {
    let mut e = evidence(HoursBasis::Variable);
    e.assistant.start_date = Some("02/04/2026".into());
    e.assistant.leaving_date = Some("03/04/2026".into());
    e.cycles.push(cycle(
        1,
        "01/04/2026",
        (1..=4)
            .map(|d| work(&format!("0{d}/04/2026"), 3000))
            .collect(),
    ));
    approx(summary(&e).entitlement, 12.0);
    e.assistant.leaving_date = None;
    e.cycles.clear();
    e.cycles.push(cycle(
        1,
        "01/09/2026",
        vec![work("08/09/2026", 3000), work("09/09/2026", 6000)],
    ));
    approx(summary(&e).entitlement, 6.0);
}
#[test]
fn dated_leave_actual_year_overrides_legacy_total_without_double_count() {
    let mut e = evidence(HoursBasis::Variable);
    e.assistant.start_date = None;
    let mut c = cycle(1, "30/03/2026", vec![]);
    c.weeks[0].annual_leave_hours = 15.0;
    c.leave = vec![leave(1, "31/03/2026", 5.0), leave(1, "01/04/2026", 10.0)];
    e.cycles.push(c);
    approx(summary(&e).taken, 10.0);
    approx(
        calculate(&e, LeaveYear(2025), date("08/09/2026"))
            .unwrap()
            .taken,
        5.0,
    );
    e.cycles[0].leave.clear();
    approx(summary(&e).taken, 0.0);
    approx(
        calculate(&e, LeaveYear(2025), date("08/09/2026"))
            .unwrap()
            .taken,
        15.0,
    );
}
#[test]
fn historic_completed_contracted_year_stays_full_and_precise() {
    let mut e = evidence(HoursBasis::Contracted);
    e.assistant.start_date = Some("01/04/2023".into());
    e.history[0].effective_date = "01/04/2023".into();
    e.history[0].contracted_hours = "17.75".into();
    approx(
        calculate(&e, LeaveYear(2023), date("08/09/2026"))
            .unwrap()
            .entitlement,
        99.4,
    );
}
#[test]
fn undated_manual_adjustments_require_whole_week_to_qualify() {
    let mut e = evidence(HoursBasis::Variable);
    let mut item = work("01/04/2026", 6000);
    item.work_date = None;
    item.source_type = "manual_adjustment".into();
    e.cycles.push(cycle(1, "01/04/2026", vec![item]));
    approx(summary(&e).entitlement, 12.0);
    e.assistant.start_date = Some("02/04/2026".into());
    let s = summary(&e);
    approx(s.entitlement, 0.0);
    assert!(s
        .incomplete
        .iter()
        .any(|w| w.contains("cannot allocate safely")));
}
#[test]
fn absent_history_and_undated_legacy_evidence_are_explicitly_incomplete() {
    let mut e = evidence(HoursBasis::Variable);
    e.history.clear();
    assert!(!summary(&e).incomplete.is_empty());
    e.history
        .push(entry(1, "01/04/2026", HoursBasis::Variable, ""));
    let mut item = work("01/04/2026", 6000);
    item.source_type = "legacy_previous_cycle_adjustment".into();
    item.work_date = None;
    e.cycles.push(cycle(1, "01/04/2026", vec![item]));
    let s = summary(&e);
    approx(s.entitlement, 0.0);
    assert!(s.incomplete.iter().any(|w| w.contains("no actual dates")));
}

#[test]
fn application_reads_operational_defaults_and_only_this_pas_definitively_sent_payslip() {
    use rusqlite::Connection;
    let (_directory, app) = crate::payroll_timesheet_screen::tests::test_application();
    let db = Connection::open(&app.context.environment.database_path).unwrap();
    let mut pa = evidence(HoursBasis::Variable).assistant;
    pa.employment_status = Some("Inactive".into()); // history remains accessible
    app.personal_assistant_repository.insert(&pa).unwrap();
    let pa_id = app.personal_assistant_repository.get_all().unwrap()[0].id;
    let mut history = entry(0, "01/04/2026", HoursBasis::Variable, "");
    history.personal_assistant_id = pa_id;
    app.contracted_hours_repository.insert(&history).unwrap();
    db.execute("INSERT INTO payroll_schedules (payroll_year,cycle_number,first_week_commencing,latest_posting_date,pay_date,created_at,payslips_sent) VALUES ('2026/27',1,'01/04/2026','28/04/2026','30/04/2026','test',1)",[]).unwrap();
    let record = app
        .payroll_timesheet_repository
        .insert("2026/27", 1, pa_id, None, "test")
        .unwrap();
    app.payroll_timesheet_repository
        .create_missing_weeks(
            record,
            &std::array::from_fn(|i| display(date("01/04/2026") + Duration::days(i as i64 * 7))),
            &[100.0, 0.0, 0.0, 0.0],
        )
        .unwrap();
    let weeks = app.payroll_timesheet_repository.get_weeks(record).unwrap();
    let ids = std::array::from_fn(|i| weeks[i].id);
    app.payroll_worked_item_repository
        .replace_candidate(
            record,
            &[work("02/04/2026", 6000)],
            "unused.pdf",
            "unused-hash",
            "test",
            0,
            &ids,
            &[6000, 0, 0, 0],
        )
        .unwrap();
    app.payroll_worked_item_repository
        .protect_for_send(record, "test")
        .unwrap();
    app.payroll_worked_item_repository
        .mark_submitted_and_email_sent(record, pa_id, "2026/27", 1, "test")
        .unwrap();
    // A timesheet email, cycle-wide flag, or another PA's payslip is insufficient.
    app.payroll_timesheet_email_repository
        .mark_sent(pa_id + 1, "2026/27", 1, "payslip", "test")
        .unwrap();
    let loaded = Evidence::load(&app, pa_id).unwrap();
    assert_eq!(loaded.settings, AnnualLeaveSettings::default());
    approx(summary(&loaded).entitlement, 0.0);
    app.payroll_timesheet_email_repository
        .protect_payslip_for_send(pa_id, "2026/27", 1, "attempt")
        .unwrap();
    approx(
        summary(&Evidence::load(&app, pa_id).unwrap()).entitlement,
        0.0,
    );
    app.payroll_timesheet_email_repository
        .mark_payslip_sent_from_indeterminate(pa_id, "2026/27", 1, "attempt", "sent")
        .unwrap();
    let version: i64 = db
        .query_row("PRAGMA data_version", [], |r| r.get(0))
        .unwrap();
    let loaded = Evidence::load(&app, pa_id).unwrap();
    approx(summary(&loaded).entitlement, 12.0);
    assert_eq!(summary(&loaded).calculated_to, Some(date("28/04/2026")));
    for year in loaded.years(date("08/09/2026")) {
        calculate(&loaded, year, date("08/09/2026")).unwrap();
    }
    assert_eq!(
        version,
        db.query_row::<i64, _, _>("PRAGMA data_version", [], |r| r.get(0))
            .unwrap()
    );
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT COUNT(*) FROM annual_leave_settings", [], |r| r
            .get(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT version FROM schema_version", [], |r| r.get(0))
            .unwrap(),
        crate::database::CURRENT_SCHEMA_VERSION
    );
}

fn historical_cycle(number: i64, start: &str) -> CycleEvidence {
    let mut c = cycle(number, start, vec![]);
    c.snapshot_state = None;
    for week in &mut c.weeks[..3] {
        week.worked_hours = 2.0;
    }
    c
}

#[test]
fn historical_sent_variable_preparation_totals_round_once_per_period() {
    let mut e = evidence(HoursBasis::Variable);
    e.cycles.push(historical_cycle(1, "01/04/2026"));
    let s = summary(&e);
    approx(s.entitlement, 1.0);
    assert!(s.incomplete.is_empty());
    assert!(s.details.iter().any(|d| d.contains("historical fallback")));
    assert_eq!(s.calculated_to, Some(date("28/04/2026")));
    e.cycles.push(historical_cycle(2, "29/04/2026"));
    approx(summary(&e).entitlement, 2.0);
    e.settings.accrual_percentage = 25.0;
    approx(summary(&e).entitlement, 4.0); // each 1.5 rounds to 2
}

#[test]
fn historical_preparation_requires_definitively_sent_payslip() {
    let mut e = evidence(HoursBasis::Variable);
    let mut c = historical_cycle(1, "01/04/2026");
    c.sent = false; // Evidence::load maps both Unsent and Indeterminate to false.
    e.cycles.push(c);
    approx(summary(&e).entitlement, 0.0);
    assert_eq!(summary(&e).calculated_to, None);
}

#[test]
fn historical_preparation_rejects_mixed_missing_and_ambiguous_basis() {
    for history in [
        vec![
            entry(1, "01/04/2026", HoursBasis::Variable, ""),
            entry(2, "04/04/2026", HoursBasis::Contracted, "0"),
        ],
        vec![entry(1, "02/04/2026", HoursBasis::Variable, "")],
        vec![],
        vec![
            entry(1, "01/04/2026", HoursBasis::Contracted, "0"),
            entry(2, "01/04/2026", HoursBasis::Variable, ""),
        ],
    ] {
        let mut e = evidence(HoursBasis::Variable);
        e.history = history;
        e.cycles.push(historical_cycle(1, "01/04/2026"));
        let s = summary(&e);
        approx(s.entitlement, 0.0);
        assert!(s
            .incomplete
            .iter()
            .any(|w| w.contains("cannot be safely attributed")));
    }
}

#[test]
fn historical_preparation_never_splits_undated_weeks_at_boundaries() {
    for (start, leaving, today) in [
        ("02/04/2026", None, "08/09/2026"),
        ("01/04/2026", Some("03/04/2026"), "08/09/2026"),
        ("01/04/2026", None, "03/04/2026"),
    ] {
        let mut e = evidence(HoursBasis::Variable);
        e.assistant.start_date = Some(start.into());
        e.assistant.leaving_date = leaving.map(str::to_owned);
        e.cycles.push(historical_cycle(1, "01/04/2026"));
        let s = calculate(&e, LeaveYear(2026), date(today)).unwrap();
        approx(s.entitlement, 0.0);
        assert_eq!(s.incomplete.is_empty(), start == "02/04/2026");
    }
    let mut e = evidence(HoursBasis::Variable);
    e.assistant.start_date = Some("01/04/2025".into());
    e.history[0].effective_date = "01/04/2025".into();
    e.cycles.push(historical_cycle(1, "30/03/2026"));
    approx(summary(&e).entitlement, 0.0);
    assert!(summary(&e).incomplete.is_empty());
    assert!(summary(&e)
        .details
        .iter()
        .any(|d| d.contains("Historical boundary exclusion")));
}

#[test]
fn historical_fallback_excludes_non_work_and_submitted_items_take_priority() {
    let mut e = evidence(HoursBasis::Variable);
    let mut c = historical_cycle(1, "01/04/2026");
    c.weeks[0].annual_leave_hours = 100.0;
    c.weeks[0].sick_leave_hours = 100.0;
    c.weeks[0].public_holiday_hours = 100.0;
    c.weeks[0].travel_miles = 100.0;
    c.record.previous_cycle_hours = Some(100.0);
    e.cycles.push(c);
    approx(summary(&e).entitlement, 1.0);
    e.cycles[0].snapshot_state = Some(SnapshotState::Submitted);
    e.cycles[0].items.push(work("02/04/2026", 6000));
    let s = summary(&e);
    approx(s.entitlement, 12.0);
    assert!(!s.details.iter().any(|d| d.contains("historical fallback")));
}

#[test]
fn historical_fallback_load_uses_per_pa_production_payslip_status() {
    use rusqlite::Connection;
    let (_directory, app) = crate::payroll_timesheet_screen::tests::test_application();
    let db = Connection::open(&app.context.environment.database_path).unwrap();
    let mut pa = evidence(HoursBasis::Variable).assistant;
    pa.employment_status = Some("Inactive".into()); // history remains accessible
    app.personal_assistant_repository.insert(&pa).unwrap();
    let pa_id = app.personal_assistant_repository.get_all().unwrap()[0].id;
    let mut history = entry(0, "01/04/2026", HoursBasis::Variable, "");
    history.personal_assistant_id = pa_id;
    app.contracted_hours_repository.insert(&history).unwrap();
    db.execute("INSERT INTO payroll_schedules (payroll_year,cycle_number,first_week_commencing,latest_posting_date,pay_date,created_at,payslips_sent) VALUES ('2026/27',1,'01/04/2026','28/04/2026','30/04/2026','test',1)",[]).unwrap();
    let record = app
        .payroll_timesheet_repository
        .insert("2026/27", 1, pa_id, None, "test")
        .unwrap();
    app.payroll_timesheet_repository
        .create_missing_weeks(
            record,
            &std::array::from_fn(|i| display(date("01/04/2026") + Duration::days(i as i64 * 7))),
            &[100.0, 0.0, 0.0, 0.0],
        )
        .unwrap();

    app.payroll_timesheet_email_repository
        .mark_sent(pa_id, "2026/27", 1, "timesheet", "test")
        .unwrap();
    app.payroll_timesheet_email_repository
        .mark_sent(pa_id + 1, "2026/27", 1, "payslip", "test")
        .unwrap();
    approx(
        summary(&Evidence::load(&app, pa_id).unwrap()).entitlement,
        0.0,
    );
    app.payroll_timesheet_email_repository
        .protect_payslip_for_send(pa_id, "2026/27", 1, "attempt")
        .unwrap();
    approx(
        summary(&Evidence::load(&app, pa_id).unwrap()).entitlement,
        0.0,
    );
    app.payroll_timesheet_email_repository
        .mark_payslip_sent_from_indeterminate(pa_id, "2026/27", 1, "attempt", "sent")
        .unwrap();
    let loaded = Evidence::load(&app, pa_id).unwrap();
    assert!(loaded.cycles[0].items.is_empty());
    assert_eq!(loaded.cycles[0].snapshot_state, None);
    let s = summary(&loaded);
    approx(s.entitlement, 12.0);
    assert!(s.incomplete.is_empty());
}

#[test]
fn historical_april_boundary_excludes_only_ambiguous_week() {
    let mut e = evidence(HoursBasis::Variable);
    e.assistant.start_date = Some("01/04/2025".into());
    e.history[0].effective_date = "01/04/2025".into();
    let mut c = historical_cycle(1, "23/03/2026");
    for (week, hours) in c.weeks.iter_mut().zip([5.0, 6.5, 0.0, 6.5]) {
        week.worked_hours = hours;
    }
    e.cycles.push(c);
    let s = summary(&e);
    approx(s.entitlement, 1.0); // Only 6.5h from w/c 13 April, not 13h.
    assert_eq!(s.calculated_to, Some(date("19/04/2026")));
    assert!(s.incomplete.is_empty());
    assert!(s
        .details
        .iter()
        .any(|w| w.contains("w/c 30/03/2026")
            && w.contains("excluded because individual work dates")));
    assert!(s.details.iter().any(|d| d.contains("6.5000 h ×")));
    let previous = calculate(&e, LeaveYear(2025), date("08/09/2026")).unwrap();
    approx(previous.entitlement, 1.0); // Only 5h; boundary hours excluded here too.
    assert!(previous.incomplete.is_empty());
    assert!(previous
        .details
        .iter()
        .any(|w| w.contains("w/c 30/03/2026")));

    e.cycles[0].weeks[1].worked_hours = 0.0;
    let s = summary(&e);
    approx(s.entitlement, 1.0);
    assert!(s.incomplete.is_empty());

    e.cycles[0].weeks[1].worked_hours = 6.5;
    e.cycles[0].snapshot_state = Some(SnapshotState::Submitted);
    e.cycles[0].items = vec![
        work("31/03/2026", 300),
        work("01/04/2026", 90),
        work("13/04/2026", 390),
    ];
    let s = summary(&e);
    approx(s.entitlement, 1.0);
    assert!(s.incomplete.is_empty());
    assert!(s.details.iter().any(|d| d.contains("8.0000 h ×")));
    assert!(!s.details.iter().any(|d| d.contains("historical fallback")));
}

#[test]
fn historical_unsafe_week_preserves_other_weeks_for_each_boundary_and_invalid_total() {
    for case in 0..6 {
        let mut e = evidence(HoursBasis::Variable);
        let mut c = historical_cycle(1, "01/04/2026");
        for week in &mut c.weeks {
            week.worked_hours = 6.5;
        }
        match case {
            0 => e.assistant.start_date = Some("03/04/2026".into()),
            1 => e.assistant.leaving_date = Some("24/04/2026".into()),
            2 => e.history[0].effective_date = "03/04/2026".into(),
            3 => e
                .history
                .push(entry(2, "24/04/2026", HoursBasis::Contracted, "0")),
            4 => c.weeks[0].worked_hours = f64::NAN,
            _ => c.weeks[0].worked_hours = -1.0,
        }
        e.cycles.push(c);
        let s = summary(&e);
        approx(s.entitlement, 2.0); // Three safe weeks: 19.5h, rounded once.
        assert_eq!(s.incomplete.is_empty(), matches!(case, 0 | 1 | 3));
    }
    let mut e = evidence(HoursBasis::Variable);
    let mut c = historical_cycle(1, "01/04/2026");
    for week in &mut c.weeks {
        week.worked_hours = 6.5;
    }
    e.cycles.push(c);
    let s = calculate(&e, LeaveYear(2026), date("24/04/2026")).unwrap();
    approx(s.entitlement, 2.0);
    assert!(s.incomplete.iter().any(|w| w.contains("w/c 22/04/2026")));
}

#[test]
fn historical_boundary_note_does_not_hide_genuine_incomplete_conditions() {
    for case in 0..4 {
        let mut e = evidence(HoursBasis::Variable);
        e.assistant.start_date = Some("01/04/2025".into());
        e.history[0].effective_date = "01/04/2025".into();
        let mut c = historical_cycle(1, "23/03/2026");
        for (week, hours) in c.weeks.iter_mut().zip([5.0, 6.5, 0.0, 6.5]) {
            week.worked_hours = hours;
        }
        match case {
            0 => e.history[0].effective_date = "03/04/2026".into(),
            1 => c.weeks[3].sick_leave_hours = 2.0,
            2 => c.weeks[1].worked_hours = f64::INFINITY,
            _ => {
                e.history
                    .push(entry(2, "01/04/2026", HoursBasis::Contracted, "0"));
                e.history
                    .push(entry(3, "01/04/2026", HoursBasis::Variable, ""));
                e.history
                    .push(entry(4, "06/04/2026", HoursBasis::Variable, ""));
            }
        }
        e.cycles.push(c);
        let s = summary(&e);
        approx(s.entitlement, 1.0);
        assert!(!s.incomplete.is_empty());
        if case == 0 {
            assert!(s
                .incomplete
                .iter()
                .any(|w| w.contains("Hours Basis history is missing")));
        }
        if case == 1 {
            assert!(s.incomplete.iter().any(|w| w.contains("Sick/SSP")));
            assert!(s
                .details
                .iter()
                .any(|d| d.contains("Historical boundary exclusion")));
        }
    }
}
