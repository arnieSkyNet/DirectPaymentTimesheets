//! Append-only preparation/document versions. No send, payment or correction state.
use crate::{
    app::Application,
    payroll_evidence::{self, Result},
    payroll_timesheet_repository::PayrollTimesheet,
    payroll_worked_item_repository::WorkedItemSnapshot,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    let tx = db.unchecked_transaction()?;
    tx.execute_batch("CREATE TABLE timesheet_amendment_versions (
        id INTEGER PRIMARY KEY, payroll_timesheet_id INTEGER NOT NULL REFERENCES payroll_timesheets(id),
        event TEXT NOT NULL CHECK(event IN ('save','generated')),
        document_id INTEGER REFERENCES timesheet_documents(id),
        before_state TEXT, after_state TEXT NOT NULL,
        recorded_at TEXT NOT NULL, actor TEXT NOT NULL);
        CREATE INDEX timesheet_amendment_record ON timesheet_amendment_versions(payroll_timesheet_id,id);
        CREATE TRIGGER retain_timesheet_amendment BEFORE DELETE ON timesheet_amendment_versions BEGIN SELECT RAISE(ABORT,'Retained timesheet amendment'); END;
        CREATE TRIGGER immutable_timesheet_amendment BEFORE UPDATE ON timesheet_amendment_versions BEGIN SELECT RAISE(ABORT,'Immutable timesheet amendment'); END;
        UPDATE schema_version SET version=42;")?;
    let tables = tx
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for table in tables {
        for op in ["INSERT", "UPDATE", "DELETE"] {
            tx.execute_batch(&format!("CREATE TRIGGER dpt42_{table}_{op} BEFORE {op} ON {table} BEGIN SELECT CASE WHEN dpt_schema_version()<42 THEN RAISE(ABORT,'Incompatible application schema') END; END;"))?;
        }
    }
    tx.commit()
}
fn available(db: &Connection) -> rusqlite::Result<bool> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='timesheet_amendment_versions')",
        [],
        |r| r.get(0),
    )
}
pub fn saved(db: &Connection, record: i64) -> Result<bool> {
    Ok(available(db)? && db.query_row("SELECT EXISTS(SELECT 1 FROM timesheet_amendment_versions WHERE payroll_timesheet_id=?1 AND event='save')",[record],|r|r.get(0))?)
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Version {
    pub document: i64,
    pub totals: [i64; 4],
    pub carry: i64,
    pub items: Vec<WorkedItemSnapshot>,
    pub applications: Vec<(i64, i64)>,
    pub evidence: Vec<(i64, String, String)>,
    pub details: serde_json::Value,
}
pub fn latest(db: &Connection, record: i64) -> Result<Option<Version>> {
    if !available(db)? {
        return Ok(None);
    }
    let text:Option<String>=db.query_row("SELECT after_state FROM timesheet_amendment_versions WHERE payroll_timesheet_id=?1 AND event='generated' ORDER BY id DESC LIMIT 1",[record],|r|r.get(0)).optional()?;
    text.map(|text| serde_json::from_str(&text).map_err(Into::into))
        .transpose()
}
fn rows(db: &Connection, table: &str, record: i64) -> Result<serde_json::Value> {
    let mut stmt = db.prepare(&format!(
        "SELECT * FROM {table} WHERE payroll_timesheet_id=?1 ORDER BY rowid"
    ))?;
    let count = stmt.column_count();
    let values = stmt
        .query_map([record], |r| {
            (0..count)
                .map(|i| {
                    Ok(match r.get_ref(i)? {
                        rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                        rusqlite::types::ValueRef::Integer(n) => n.into(),
                        rusqlite::types::ValueRef::Real(n) => n.into(),
                        rusqlite::types::ValueRef::Text(t) => {
                            String::from_utf8_lossy(t).to_string().into()
                        }
                        rusqlite::types::ValueRef::Blob(_) => {
                            return Err(rusqlite::Error::InvalidQuery)
                        }
                    })
                })
                .collect::<rusqlite::Result<Vec<_>>>()
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(serde_json::to_value(values)?)
}
pub fn details(db: &Connection, record: i64) -> Result<serde_json::Value> {
    let (carry, notes): (Option<f64>, String) = db.query_row(
        "SELECT previous_cycle_hours,payroll_department_notes FROM payroll_timesheets WHERE id=?1",
        [record],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(serde_json::json!({"carry":carry,"notes":notes,
        "weeks":rows(db,"payroll_timesheet_weeks",record)?,
        "leave":rows(db,"payroll_timesheet_annual_leave",record)?,
        "holidays":rows(db,"payroll_timesheet_public_holidays",record)?,
        "manual":rows(db,"payroll_timesheet_manual_adjustments",record)?}))
}
pub fn record_save(db: &Connection, record: i64, before: &serde_json::Value) -> Result<()> {
    db.execute("INSERT INTO timesheet_amendment_versions(payroll_timesheet_id,event,before_state,after_state,recorded_at,actor) VALUES(?1,'save',?2,?3,?4,'local_employer')",params![record,serde_json::to_string(before)?,serde_json::to_string(&details(db,record)?)?,payroll_evidence::now()])?;
    // A general saved amendment supersedes the old sickness-only restriction,
    // without deleting its original evidence or changing its financial history.
    db.execute("UPDATE sickness_corrections SET retired_at=?1 WHERE payroll_timesheet_id=?2 AND retired_at IS NULL",params![payroll_evidence::now(),record])?;
    Ok(())
}
pub fn record_generation(
    db: &Connection,
    record: i64,
    totals: [i64; 4],
    carry: i64,
    items: &[WorkedItemSnapshot],
) -> Result<()> {
    if !saved(db, record)? {
        return Ok(());
    }
    let (pa,start):(i64,String)=db.query_row("SELECT p.personal_assistant_id,s.first_week_commencing FROM payroll_timesheets p JOIN payroll_schedules s ON s.payroll_year=p.payroll_year AND s.cycle_number=p.cycle_number WHERE p.id=?1",[record],|r|Ok((r.get(0)?,r.get(1)?)))?;
    let start = crate::date_utils::parse_legacy(&start)?;
    let raw = payroll_evidence::load_for_pa(db, pa)?;
    let pre = payroll_evidence::preflight_for_pa(db, &raw, pa)?;
    let evidence = pre
        .eligible
        .iter()
        .filter_map(|e| {
            let date = e.date().ok()?;
            if date < start || date > start + chrono::Duration::days(27) {
                return None;
            }
            Some(((date - start).num_days() / 7 + 1, e.key(), e.fingerprint()))
        })
        .collect();
    let tx = db.unchecked_transaction()?;
    let document = tx.query_row(
        "SELECT document_id FROM payroll_timesheet_snapshot_states WHERE payroll_timesheet_id=?1",
        [record],
        |r| r.get(0),
    )?;
    let applications=tx.prepare("SELECT correction_id,minutes FROM payroll_correction_applications WHERE payroll_timesheet_id=?1 ORDER BY correction_id")?.query_map([record],|r|Ok((r.get(0)?,r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let version = Version {
        document,
        totals,
        carry,
        items: items.to_vec(),
        applications,
        evidence,
        details: details(&tx, record)?,
    };
    tx.execute("INSERT INTO timesheet_amendment_versions(payroll_timesheet_id,event,document_id,after_state,recorded_at,actor) VALUES(?1,'generated',?2,?3,?4,'local_employer')",params![record,document,serde_json::to_string(&version)?,payroll_evidence::now()])?;
    tx.commit()?;
    Ok(())
}
pub fn verify_generated(db: &Connection, record: i64) -> Result<()> {
    let version = latest(db, record)?.ok_or("Regenerate the saved amendment before sending")?;
    let document: i64 = db.query_row(
        "SELECT document_id FROM payroll_timesheet_snapshot_states WHERE payroll_timesheet_id=?1",
        [record],
        |r| r.get(0),
    )?;
    if version.document != document || version.details != details(db, record)? {
        return Err(
            "Saved timesheet differs from its generated version; regenerate before sending".into(),
        );
    }
    Ok(())
}
/// Saved weekly figures are authoritative inputs for regeneration. Retained
/// source membership remains evidence; balancing manual items describe the edit.
pub fn hours(
    app: &Application,
    record: &PayrollTimesheet,
) -> Result<crate::pay_rate_allocation::ReconciledPayrollHours> {
    let db = payroll_evidence::open(app)?;
    let weeks = app.payroll_timesheet_repository.get_weeks(record.id)?;
    if weeks.len() != 4 {
        return Err("Expected four saved payroll weeks".into());
    }
    let mut totals = [0; 4];
    for week in &weeks {
        if !(1..=4).contains(&week.week_number)
            || !week.worked_hours.is_finite()
            || week.worked_hours < 0.0
        {
            return Err("Invalid saved worked hours".into());
        }
        totals[(week.week_number - 1) as usize] = (week.worked_hours * 60.0).round() as i64;
    }
    let mut items = payroll_evidence::lifecycle::submitted_items(&db, record.id, app)?;
    for (index, total) in totals.iter().enumerate() {
        let before: i64 = items
            .iter()
            .filter(|i| i.week_number == (index + 1) as i64)
            .map(|i| i.worked_minutes)
            .sum();
        let delta = total - before;
        if delta != 0 {
            let week = &weeks[index];
            let start = crate::date_utils::parse_legacy(&week.week_commencing)?;
            let rate = crate::pay_rate_allocation::select_manual_adjustment_rate(
                &app.pay_rate_repository,
                record.personal_assistant_id,
                start,
                start + chrono::Duration::days(6),
                delta,
            )?
            .ok_or("No applicable pay rate for the amended payroll week")?;
            items.push(WorkedItemSnapshot {
                week_number: (index + 1) as i64,
                source_type: "manual_adjustment".into(),
                timesheet_id: None,
                direct_shift_id: None,
                source_evidence: None,
                work_date: None,
                worked_minutes: delta,
                pay_rate_id: Some(rate.id),
                pay_rate_effective_date: Some(rate.effective_date),
                total_hourly_rate: Some(rate.base_hourly_rate + rate.employer_top_up_rate),
                reason: Some("Employer amended saved timesheet total".into()),
            });
        }
    }
    Ok(crate::pay_rate_allocation::ReconciledPayrollHours {
        weeks: std::array::from_fn(|_| Vec::new()),
        week_totals_minutes: totals,
        previous_cycle_minutes: record
            .previous_cycle_hours
            .map_or(0, |h| (h * 60.0).round() as i64),
        snapshot_items: items,
    })
}

#[cfg(test)]
pub(crate) fn remove_schema_42_fixture(db: &Connection) {
    let triggers = db
        .prepare("SELECT name FROM sqlite_master WHERE type='trigger' AND name LIKE 'dpt42_%'")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    for name in triggers {
        db.execute_batch(&format!("DROP TRIGGER {name}")).unwrap();
    }
    db.execute_batch("DROP TABLE IF EXISTS timesheet_amendment_versions")
        .unwrap();
}

#[cfg(test)]
mod tests {
    #[test]
    fn feature2_schema42_startup_requires_version_retention_guards() {
        let dir = crate::private_storage::tempdir().unwrap();
        let path = dir.path().join("synthetic.sqlite");
        let db = crate::database::open(&path).unwrap();
        crate::database::create_schema(&db).unwrap();
        db.execute_batch("DROP TRIGGER immutable_timesheet_amendment")
            .unwrap();
        let before = crate::database_recovery::fingerprint(&db).unwrap();
        assert!(crate::database::initialise_database(&path).is_err());
        assert_eq!(crate::database_recovery::fingerprint(&db).unwrap(), before);
    }
    #[test]
    fn feature2_schema42_upgrade_is_atomic_and_rejects_old_writers() {
        let dir = crate::private_storage::tempdir().unwrap();
        let db = crate::database::open(dir.path().join("synthetic.sqlite")).unwrap();
        crate::database::create_legacy_schema(&db, 41).unwrap();
        db.execute_batch("CREATE TRIGGER fail_upgrade BEFORE UPDATE ON schema_version WHEN NEW.version=42 BEGIN SELECT RAISE(ABORT,'synthetic rollback'); END;").unwrap();
        assert!(crate::database::create_schema(&db).is_err());
        assert!(!super::available(&db).unwrap());
        assert_eq!(
            db.query_row("SELECT version FROM schema_version", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            41
        );
        db.execute_batch("DROP TRIGGER fail_upgrade").unwrap();
        crate::database::create_schema(&db).unwrap();
        assert!(super::available(&db).unwrap());
        db.create_scalar_function(
            "dpt_schema_version",
            0,
            rusqlite::functions::FunctionFlags::SQLITE_UTF8,
            |_| Ok(41i64),
        )
        .unwrap();
        assert!(db
            .execute(
                "INSERT INTO employers(name) VALUES('Synthetic employer')",
                []
            )
            .is_err());
    }
}
