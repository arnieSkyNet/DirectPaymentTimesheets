//! One-time cutover inventory, not an assertion of individual shift payment.
use super::*;

#[derive(Serialize, Deserialize)]
pub struct Inventory {
    pub provenance: String,
    pub evidence: Vec<WorkEvidence>,
    pub settled: Vec<Settlement>,
}
#[derive(Serialize, Deserialize)]
pub struct Settlement {
    pub record: i64,
    pub sent_at: String,
}
fn settlements(db: &Connection) -> rusqlite::Result<Vec<Settlement>> {
    db.prepare("SELECT p.id,e.sent_at FROM payroll_timesheets p JOIN payroll_timesheet_email_status e ON e.personal_assistant_id=p.personal_assistant_id AND e.payroll_year=p.payroll_year AND e.cycle_number=p.cycle_number WHERE e.email_type='payslip' AND e.sent_at IS NOT NULL AND substr(e.sent_at,1,14)<>'indeterminate:' ORDER BY p.id")?
        .query_map([], |r| Ok(Settlement {record:r.get(0)?,sent_at:r.get(1)?}))?.collect()
}
pub fn inventory_path(db: &Connection) -> rusqlite::Result<Option<std::path::PathBuf>> {
    let path: String = db.query_row(
        "SELECT file FROM pragma_database_list WHERE name='main'",
        [],
        |r| r.get(0),
    )?;
    Ok((!path.is_empty())
        .then(|| std::path::PathBuf::from(format!("{path}.schema28-cutover.toml"))))
}
pub fn migrate(db: &Connection, before_28: bool) -> rusqlite::Result<()> {
    migrate_inner(db, before_28).map_err(|e| {
        rusqlite::Error::InvalidParameterName(format!("Legacy cutover migration: {e}"))
    })
}
fn migrate_inner(db: &Connection, before_28: bool) -> Result<()> {
    let inventory = if before_28 {
        Some(Inventory { provenance: "Evidence and definitive settlement captured while upgrading from a schema before 28; no historical timestamp inferred".into(), evidence: load_connection(db)?, settled: settlements(db)? })
    } else if let Some(path) = inventory_path(db)?.filter(|p| p.exists()) {
        Some(toml::from_str::<Inventory>(&std::fs::read_to_string(
            path,
        )?)?)
    } else {
        None
    };
    let tx = db.unchecked_transaction()?;
    tx.execute_batch("CREATE TABLE payroll_legacy_cutover (id INTEGER PRIMARY KEY CHECK(id=1), recorded_at TEXT NOT NULL, inventory TEXT NOT NULL);
        CREATE TABLE payroll_legacy_evidence (source TEXT NOT NULL, source_id INTEGER NOT NULL, fingerprint TEXT NOT NULL, PRIMARY KEY(source,source_id));
        CREATE TABLE payroll_legacy_settlements (payroll_timesheet_id INTEGER PRIMARY KEY, sent_at TEXT NOT NULL);")?;
    if let Some(inventory) = inventory {
        if inventory.provenance.trim().is_empty() {
            return Err("Inventory provenance is required".into());
        }
        let current = load_connection(&tx)?;
        for e in &inventory.evidence {
            if !current
                .iter()
                .any(|c| c.key() == e.key() && c.fingerprint() == e.fingerprint())
            {
                return Err(format!(
                    "Cutover evidence {} does not match stored evidence",
                    e.key()
                )
                .into());
            }
            tx.execute(
                "INSERT INTO payroll_legacy_evidence VALUES (?1,?2,?3)",
                params![e.source, e.id, e.fingerprint()],
            )?;
        }
        let current = settlements(&tx)?;
        for s in &inventory.settled {
            if !current
                .iter()
                .any(|c| c.record == s.record && c.sent_at == s.sent_at)
            {
                return Err("Cutover settlement does not match definitive payslip status".into());
            }
            tx.execute(
                "INSERT INTO payroll_legacy_settlements VALUES (?1,?2)",
                params![s.record, s.sent_at],
            )?;
        }
        tx.execute(
            "INSERT INTO payroll_legacy_cutover VALUES (1,?1,?2)",
            params![now(), toml::to_string(&inventory)?],
        )?;
    }
    tx.execute("UPDATE schema_version SET version=29", [])?;
    tx.commit()?;
    Ok(())
}
/// Caller must also establish current definitive settlement and period containment.
/// Material edits lose the exemption; notes-only edits do not.
pub fn contains(db: &Connection, record: i64, evidence: &WorkEvidence) -> Result<bool> {
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_legacy_evidence e JOIN payroll_legacy_settlements s ON s.payroll_timesheet_id=?1 WHERE e.source=?2 AND e.source_id=?3 AND e.fingerprint=?4)",params![record,evidence.source,evidence.id,evidence.fingerprint()],|r|r.get(0))?)
}

pub fn has_settlement(db: &Connection, record: i64) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM payroll_legacy_settlements WHERE payroll_timesheet_id=?1)",
        [record],
        |r| r.get(0),
    )?)
}
