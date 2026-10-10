//! Stage 6: durable deactivation intent and provenance-backed file recovery.
//! Lock order: production OS lock, then SQLite writer reservation, then filesystem.
//! Recovery never adopts a destination by filename/hash alone: the retained private
//! staging hard link proves publication until database registration has committed.
use super::*;
use std::io::Write;

pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    let tx = db.unchecked_transaction()?;
    tx.execute_batch("CREATE TABLE payroll_filing_requests(
        personal_assistant_id INTEGER PRIMARY KEY, previous_first_name TEXT NOT NULL,
        previous_surname TEXT NOT NULL, payslip_root TEXT NOT NULL, pdf_root TEXT NOT NULL,
        state TEXT NOT NULL CHECK(state IN ('pending','complete')), last_error TEXT NOT NULL DEFAULT '',
        requested_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
        CREATE TABLE payroll_filing_intents(
        id INTEGER PRIMARY KEY, personal_assistant_id INTEGER NOT NULL,
        source_path TEXT NOT NULL UNIQUE,destination_path TEXT NOT NULL UNIQUE,
        sha256 TEXT NOT NULL CHECK(length(sha256)=64), staging_path TEXT NOT NULL UNIQUE,
        registration TEXT NOT NULL, document_id INTEGER,
        state TEXT NOT NULL CHECK(state IN ('planned','staged','published','registered','source_removed','complete')),
        last_error TEXT NOT NULL DEFAULT '',created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
        CREATE TRIGGER filing_identity_immutable BEFORE UPDATE OF personal_assistant_id,source_path,destination_path,sha256,staging_path,registration,document_id ON payroll_filing_intents BEGIN SELECT RAISE(ABORT,'Immutable filing intent'); END;
        CREATE TRIGGER filing_evidence_retained BEFORE DELETE ON payroll_filing_intents BEGIN SELECT RAISE(ABORT,'Retained filing evidence'); END;
        UPDATE schema_version SET version=39;")?;
    let tables = tx
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for table in tables {
        for op in ["INSERT", "UPDATE", "DELETE"] {
            tx.execute_batch(&format!("CREATE TRIGGER dpt39_{table}_{op} BEFORE {op} ON {} BEGIN SELECT CASE WHEN dpt_schema_version()<39 THEN RAISE(ABORT,'Incompatible application schema') END; END;",crate::database_recovery::quote(&table)))?;
        }
    }
    tx.commit()
}
#[cfg(test)]
pub(crate) fn remove_schema_39_fixture(db: &Connection) {
    let guards = db
        .prepare("SELECT name FROM sqlite_master WHERE type='trigger' AND name LIKE 'dpt39_%'")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    for name in guards {
        db.execute_batch(&format!(
            "DROP TRIGGER {}",
            crate::database_recovery::quote(&name)
        ))
        .unwrap();
    }
    db.execute_batch("DROP TABLE IF EXISTS payroll_filing_intents; DROP TABLE IF EXISTS payroll_filing_requests;").unwrap();
}
fn roots(app: &Application) -> (String, String) {
    let root = |p: &PathBuf| {
        naming::root_without_payroll_year_suffix(&crate::paths::expand_path(p))
            .to_string_lossy()
            .into_owned()
    };
    (
        root(&app.context.config.folders.payslip_folder),
        root(&app.context.config.folders.pdf_output),
    )
}
pub(super) fn request(
    db: &Connection,
    app: &Application,
    pa: &PersonalAssistant,
    old: &PersonalAssistant,
) -> Result<()> {
    let (payslips, pdfs) = roots(app);
    // The request and saved inactive status commit together; file operations do not.
    let pending:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_filing_requests WHERE personal_assistant_id=?1 AND state='pending')",[pa.id],|r|r.get(0))?;
    if pending {
        return Err("Existing incomplete filing must be resumed before another deactivation; no PA changes saved".into());
    }
    db.execute("INSERT INTO payroll_filing_requests(personal_assistant_id,previous_first_name,previous_surname,payslip_root,pdf_root,state) VALUES(?1,?2,?3,?4,?5,'pending') ON CONFLICT(personal_assistant_id) DO UPDATE SET previous_first_name=excluded.previous_first_name,previous_surname=excluded.previous_surname,payslip_root=excluded.payslip_root,pdf_root=excluded.pdf_root,state='pending',last_error='',requested_at=CURRENT_TIMESTAMP",params![pa.id,old.first_name,old.surname,payslips,pdfs])?;
    Ok(())
}
pub(super) fn available_bases(app: &Application) -> (Vec<PathBuf>, Vec<String>) {
    let mut bases = Vec::new();
    let mut failures = Vec::new();
    for root in [
        &app.context.config.folders.payslip_folder,
        &app.context.config.folders.pdf_output,
    ] {
        let expanded = crate::paths::expand_path(root);
        match filing_base(&expanded).and_then(|base| { if !base.try_exists()? { return Err("Configured root is unavailable".into()); } Ok(base) }) {
            Ok(base) => { if !bases.contains(&base) { bases.push(base); } }
            Err(e)=>failures.push(format!("Filing pending for {}: {e}. Reconnect/check access and use Resume incomplete filing; configured paths were retained.",expanded.display()))
        }
    }
    (bases, failures)
}
pub fn pending_summary(db: &Connection, pa: Option<i64>) -> Result<String> {
    let requests:i64=db.query_row("SELECT COUNT(*) FROM payroll_filing_requests WHERE state='pending' AND (?1 IS NULL OR personal_assistant_id=?1)",[pa],|r|r.get(0))?;
    let old: i64 = db.query_row(
        "SELECT COUNT(*) FROM payroll_file_moves WHERE (?1 IS NULL OR personal_assistant_id=?1)",
        [pa],
        |r| r.get(0),
    )?;
    let mut rows=db.prepare("SELECT source_path,destination_path,state,last_error FROM payroll_filing_intents WHERE state<>'complete' AND (?1 IS NULL OR personal_assistant_id=?1) ORDER BY id")?;
    let pending = rows
        .query_map([pa], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if requests == 0 && old == 0 && pending.is_empty() {
        return Ok(String::new());
    }
    let mut summary=format!("Incomplete filing: {requests} pending PA scan(s), {} document operation(s), {old} legacy cleanup(s). Use Personal Assistant Maintenance → Resume incomplete filing. Unrelated payroll remains available.",pending.len());
    if pa.is_some() {
        let errors=db.prepare("SELECT last_error FROM payroll_filing_requests WHERE personal_assistant_id=?1 AND state='pending'")?.query_map([pa],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        for error in errors.into_iter().filter(|e| !e.is_empty()) {
            summary.push_str(&format!("\n{error}"));
        }
        let legacy=db.prepare("SELECT source_path,destination_path FROM payroll_file_moves WHERE personal_assistant_id=?1 ORDER BY source_path")?.query_map([pa],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        for (source, destination) in legacy {
            summary.push_str(&format!("\nlegacy cleanup pending: {source} → {destination}; destination/source will be verified before deletion."));
        }
        for (source, dest, state, error) in pending {
            let state = match state.as_str() {
                "planned" => "planned — copy pending",
                "staged" => "staged — publication pending",
                "published" => "published — registration pending",
                "registered" => "registered — original cleanup pending",
                "source_removed" => "original removed — final cleanup pending",
                other => other,
            };
            summary.push_str(&format!(
                "\n{state}: {source} → {dest}{}",
                if error.is_empty() {
                    String::new()
                } else {
                    format!(" — {error}")
                }
            ));
        }
    }
    Ok(summary)
}
pub(super) fn finish_scan(db: &Connection, pa: i64, messages: &[String]) -> Result<()> {
    let failures = messages
        .iter()
        .filter(|m| m.contains("pending") || m.contains("failed") || m.contains("refused"))
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    let pending:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_filing_intents WHERE personal_assistant_id=?1 AND state<>'complete')",[pa],|r|r.get(0))?;
    db.execute(
        "UPDATE payroll_filing_requests SET state=?1,last_error=?2 WHERE personal_assistant_id=?3",
        params![
            if failures.is_empty() && !pending {
                "complete"
            } else {
                "pending"
            },
            failures,
            pa
        ],
    )?;
    Ok(())
}
pub fn resume(app: &Application, pa: i64) -> Result<Vec<String>> {
    let db = crate::payroll_evidence::open(app)?;
    let _guard = crate::timesheet_delivery::production_lock(&db)?;
    super::ensure_pa_not_uncertain(&db, pa)?;
    let repository = PersonalAssistantRepository::new(db);
    let current = repository
        .get_all()?
        .into_iter()
        .find(|p| p.id == pa)
        .ok_or("PA no longer exists")?;
    let request:Option<(String,String,String,String)>=repository.connection.query_row("SELECT previous_first_name,previous_surname,payslip_root,pdf_root FROM payroll_filing_requests WHERE personal_assistant_id=?1 AND state='pending'",[pa],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    if let Some((_, _, payslips, pdfs)) = &request {
        if roots(app) != (payslips.clone(), pdfs.clone()) {
            return Err("Configured roots changed since filing was requested. Restore the original configured paths and reconnect them; no scan or automatic filesystem reconciliation was performed.".into());
        }
    }
    let mut messages = missing_registrations(&repository.connection, pa)?;
    let (bases, errors) = available_bases(app);
    messages.extend(errors);
    let ids=repository.connection.prepare("SELECT id FROM payroll_filing_intents WHERE personal_assistant_id=?1 AND state<>'complete' ORDER BY id")?.query_map([pa],|r|r.get::<_,i64>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for id in ids {
        if let Err(e) = recover(&repository.connection, id, &bases) {
            record_error(&repository.connection, id, &e.to_string())?;
            messages.push(format!("Document filing pending: {e}. Retain both copies; reconnect/check access or seek reconciliation, then resume."));
        }
    }
    if let Err(e) = cleanup(&repository.connection, pa, &bases) {
        messages.push(format!("Cleanup pending: {e}"));
    }
    if let Some((first, surname, _, _)) = request {
        if !naming::is_inactive(&current) {
            messages.push("Filing pending: PA was reactivated before the original scan completed. Captured document intents were recovered, but an unplanned rescan could archive new active-employment documents; it was refused. Retain files and request explicit reconciliation; employment status was not changed.".into());
            finish_scan(&repository.connection, pa, &messages)?;
            return Ok(messages);
        }
        let mut previous = current.clone();
        previous.first_name = first;
        previous.surname = surname;
        // An explicit resume completes old intent even after reactivation; it never
        // moves Archived history back to active filing or changes PA status.
        if let Err(e) = archive_existing(app, &current, &previous, &repository, &mut messages) {
            messages.push(format!("Filing pending: {e}"));
            finish_scan(&repository.connection, pa, &messages)?;
        }
    }
    if !messages.is_empty() {
        finish_scan(&repository.connection, pa, &messages)?;
    }
    if messages.is_empty() {
        messages.push("No incomplete filing remains; archived history stays in place.".into());
    }
    Ok(messages)
}

/// Exact retained row membership, including historical hashes/statuses, independent
/// of the path field being relocated. Restored/stale metadata cannot pass this check.
fn registration(db: &Connection, path: &str, pa: i64) -> Result<String> {
    let foreign:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM imported_payroll_documents WHERE stored_path=?1 AND personal_assistant_id<>?2 UNION SELECT 1 FROM payslip_revisions WHERE stored_path=?1 AND personal_assistant_id<>?2 UNION SELECT 1 FROM payroll_timesheet_snapshot_states s JOIN payroll_timesheets p ON p.id=s.payroll_timesheet_id WHERE s.pdf_path=?1 AND p.personal_assistant_id<>?2 UNION SELECT 1 FROM payroll_submissions s JOIN payroll_timesheets p ON p.id=s.payroll_timesheet_id WHERE s.pdf_path=?1 AND p.personal_assistant_id<>?2)",params![path,pa],|r|r.get(0))?;
    if foreign {
        return Err("Filing path belongs to another PA; nothing changed".into());
    }
    let mut data = format!("PA:{pa};");
    for (table, column) in [
        ("imported_payroll_documents", "stored_path"),
        ("payslip_revisions", "stored_path"),
        ("payroll_timesheet_snapshot_states", "pdf_path"),
        ("payroll_submissions", "pdf_path"),
    ] {
        data.push_str(table);
        let mut statement = db.prepare(&format!(
            "SELECT * FROM {table} WHERE {column}=?1 ORDER BY rowid"
        ))?;
        let count = statement.column_count();
        let path_index = statement
            .column_names()
            .iter()
            .position(|c| *c == column)
            .unwrap();
        let rows = statement
            .query_map([path], |row| {
                let mut result = String::new();
                for i in 0..count {
                    if i != path_index {
                        result.push_str(&format!("{:?};", row.get_ref(i)?));
                    }
                }
                Ok(result)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for row in rows {
            data.push_str(&row);
        }
    }
    Ok(crate::shift_changes::digest(data.as_bytes()))
}
pub(super) fn known_pending(
    db: &Connection,
    pa: i64,
    source: &Path,
    dest: &Path,
    digest: &str,
) -> Result<bool> {
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_filing_intents WHERE personal_assistant_id=?1 AND source_path=?2 AND destination_path=?3 AND sha256=?4 AND state<>'complete')",params![pa,source.to_str(),dest.to_str(),digest],|r|r.get(0))?)
}
fn record_error(db: &Connection, id: i64, error: &str) -> Result<()> {
    db.execute(
        "UPDATE payroll_filing_intents SET last_error=?1,updated_at=CURRENT_TIMESTAMP WHERE id=?2",
        params![error, id],
    )?;
    Ok(())
}
fn progress(db: &Connection, id: i64, state: &str) -> Result<()> {
    db.execute("UPDATE payroll_filing_intents SET state=?1,last_error='',updated_at=CURRENT_TIMESTAMP WHERE id=?2",params![state,id])?;
    Ok(())
}
pub(super) fn move_file(
    db: &Connection,
    pa: i64,
    bases: &[PathBuf],
    movement: &Move,
) -> Result<Vec<String>> {
    let id = prepare(db, pa, movement)?;
    checkpoint("planned")?;
    match recover(db, id, bases) {
        Ok(()) => Ok(vec![
            "1 payroll file filed; original evidence and delivery history preserved.".into(),
        ]),
        Err(e) => {
            record_error(db, id, &e.to_string())?;
            Err(e)
        }
    }
}
fn prepare(db: &Connection, pa: i64, m: &Move) -> Result<i64> {
    super::ensure_pa_not_uncertain(db, pa)?;
    let tx = rusqlite::Transaction::new_unchecked(db, rusqlite::TransactionBehavior::Immediate)?;
    if let Some((id,dest,digest,owner,complete))=tx.query_row("SELECT id,destination_path,sha256,personal_assistant_id,state='complete' FROM payroll_filing_intents WHERE source_path=?1",[m.source.to_str()],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?,r.get::<_,bool>(4)?))).optional()? {
        if dest!=m.destination.to_string_lossy()||digest!=m.digest||owner!=pa { return Err("Existing filing intent differs; retain files and review original evidence".into()); }
        if complete && m.source.try_exists()? { return Err("Source pathname reappeared after completed filing; retain it and review historical evidence. It was not counted as newly filed.".into()); }
        return Ok(id);
    }
    checked_path(&m.source)?;
    checked_path(&m.destination)?;
    verified_bytes(&m.source, &m.digest)?;
    if m.source == m.destination || m.destination.try_exists()? {
        return Err("Archive destination already exists without verified publication provenance; originals retained".into());
    }
    let binding = registration(&tx, m.source.to_str().ok_or("Non-UTF8 source")?, pa)?;
    let empty = registration(
        &tx,
        m.destination.to_str().ok_or("Non-UTF8 destination")?,
        pa,
    )?;
    if empty != registration_empty(pa) {
        return Err("Destination already registered; filing refused".into());
    }
    let token: String = tx.query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))?;
    let stage = m
        .destination
        .parent()
        .ok_or("Missing destination parent")?
        .join(format!(".dpt-filing-{token}.tmp"));
    tx.execute("INSERT INTO payroll_filing_intents(personal_assistant_id,source_path,destination_path,sha256,staging_path,registration,document_id,state) VALUES(?1,?2,?3,?4,?5,?6,?7,'planned')",params![pa,m.source.to_str(),m.destination.to_str(),m.digest,stage.to_str(),binding,m.document_id])?;
    let id = tx.last_insert_rowid();
    tx.commit()?;
    Ok(id)
}
fn registration_empty(pa: i64) -> String {
    crate::shift_changes::digest(format!("PA:{pa};imported_payroll_documentspayslip_revisionspayroll_timesheet_snapshot_statespayroll_submissions").as_bytes())
}
fn recover(db: &Connection, id: i64, bases: &[PathBuf]) -> Result<()> {
    let (pa,source,dest,digest,stage,binding,state):(i64,String,String,String,String,String,String)=db.query_row("SELECT personal_assistant_id,source_path,destination_path,sha256,staging_path,registration,state FROM payroll_filing_intents WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?)))?;
    if state == "complete" {
        return Ok(());
    }
    super::ensure_pa_not_uncertain(db, pa)?;
    let source = Path::new(&source);
    let dest = Path::new(&dest);
    let stage = Path::new(&stage);
    for path in [source, dest, stage] {
        checked_path(path)?;
    }
    let s = normalised_path(source)?;
    let d = normalised_path(dest)?;
    if s == d
        || !bases
            .iter()
            .any(|base| s.starts_with(base) && d.starts_with(base))
        || stage.parent() != dest.parent()
    {
        return Err("Filing paths no longer belong to the original managed roots; no automatic reconciliation".into());
    }
    let before_registration = matches!(state.as_str(), "planned" | "staged" | "published");
    if before_registration {
        if registration(db, source.to_str().unwrap(), pa)? != binding
            || registration(db, dest.to_str().unwrap(), pa)? != registration_empty(pa)
        {
            return Err("Registered evidence changed or restored metadata does not match filing intent; retain files and review".into());
        }
        verified_bytes(source, &digest)?;
        fs::create_dir_all(dest.parent().unwrap())?;
        if !stage.try_exists()? {
            if state != "planned" || dest.try_exists()? {
                return Err(
                    "Publication provenance is missing; destination must not be adopted".into(),
                );
            }
            let mut output = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(stage)?;
            checkpoint("staging_created")?;
            std::io::copy(&mut fs::File::open(source)?, &mut output)?;
            output.flush()?;
            output.sync_all()?;
            verified_bytes(stage, &digest)?;
            sync_publication_directory(stage.parent().unwrap())?;
        }
        verified_bytes(stage, &digest)?;
        // A retry after a failed staging fsync must flush the file again before
        // publishing; a valid digest alone does not establish durability.
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(stage)?
            .sync_all()?;
        for entry in fs::read_dir(dest.parent().unwrap())? {
            let existing = entry?.path();
            if key(&existing) == key(dest) && existing != dest {
                return Err("Case-insensitive destination conflict; originals retained".into());
            }
        }
        if dest.try_exists()? {
            if state == "planned" {
                return Err(
                    "Destination exists before authorised publication; no orphan adoption".into(),
                );
            }
            if !same_file::is_same_file(stage, dest)? {
                return Err("Existing destination is not this operation's retained publication; no files changed".into());
            }
        } else {
            progress(db, id, "staged")?;
            checkpoint("staged")?;
            fs::hard_link(stage, dest)?;
            checkpoint("published_before_state")?;
        }
        verified_bytes(dest, &digest)?;
        sync_publication_directory(dest.parent().unwrap())?;
        progress(db, id, "published")?;
        checkpoint("published")?;
        let tx =
            rusqlite::Transaction::new_unchecked(db, rusqlite::TransactionBehavior::Immediate)?;
        if registration(&tx, source.to_str().unwrap(), pa)? != binding
            || registration(&tx, dest.to_str().unwrap(), pa)? != registration_empty(pa)
        {
            return Err("Filing registration changed; originals retained".into());
        }
        verified_bytes(source, &digest)?;
        verified_bytes(dest, &digest)?;
        for (table, column) in [
            ("imported_payroll_documents", "stored_path"),
            ("payslip_revisions", "stored_path"),
            ("payroll_timesheet_snapshot_states", "pdf_path"),
            ("payroll_submissions", "pdf_path"),
        ] {
            tx.execute(
                &format!("UPDATE {table} SET {column}=?1 WHERE {column}=?2"),
                params![dest.to_str(), source.to_str()],
            )?;
        }
        progress(&tx, id, "registered")?;
        tx.commit()?;
        checkpoint("registered")?;
    }
    // Hold the writer reservation through deletion: restore and unguarded SQL
    // writers cannot invalidate the verified registered evidence in this window.
    let tx = rusqlite::Transaction::new_unchecked(db, rusqlite::TransactionBehavior::Immediate)?;
    if registration(&tx, dest.to_str().unwrap(), pa)? != binding
        || registration(&tx, source.to_str().unwrap(), pa)? != registration_empty(pa)
    {
        return Err(
            "Registered destination no longer matches original filing evidence; originals retained"
                .into(),
        );
    }
    verified_bytes(dest, &digest)?;
    // Check retained publication provenance before touching the original, even
    // after registration. An external replacement must not become a late
    // staging-cleanup error after the original has already been removed.
    if stage.try_exists()? {
        verified_bytes(stage, &digest)?;
        if !same_file::is_same_file(stage, dest)? {
            return Err("Registered destination no longer matches retained publication provenance; original retained for review".into());
        }
    }
    if source.try_exists()? {
        if state == "source_removed" {
            return Err("Original pathname was recreated after verified removal; retain it for review, cleanup refused".into());
        }
        verified_bytes(source, &digest)?;
        fs::remove_file(source)?;
        sync_directory(source.parent().unwrap())?;
        checkpoint("deleted_before_state")?;
    }
    progress(&tx, id, "source_removed")?;
    tx.commit()?;
    checkpoint("source_removed")?;
    // Only remove our retained private link; never an unrelated staging pathname.
    if stage.try_exists()? {
        verified_bytes(stage, &digest)?;
        if !same_file::is_same_file(stage, dest)? {
            return Err("Staging provenance changed; cleanup blocked".into());
        }
        fs::remove_file(stage)?;
        sync_publication_directory(stage.parent().unwrap())?;
    }
    checkpoint("stage_removed")?;
    progress(db, id, "complete")?;
    Ok(())
}

#[cfg(test)]
thread_local! { static INTERRUPT: std::cell::RefCell<Option<&'static str>> = const { std::cell::RefCell::new(None) }; }
fn checkpoint(_point: &str) -> Result<()> {
    #[cfg(test)]
    if INTERRUPT.with(|p| *p.borrow() == Some(_point)) {
        return Err(format!("Injected interruption at {_point}").into());
    }
    Ok(())
}
#[cfg(test)]
#[path = "payroll_filing_tests.rs"]
mod tests;

/// Enumerate available years without letting one bad entry hide other years.
pub(super) fn scan_years(base: &Path) -> (Vec<PathBuf>, Vec<String>) {
    let mut years = vec![base.to_path_buf()];
    let mut failures = Vec::new();
    match fs::read_dir(base) {
        Ok(entries) => {
            for entry in entries {
                let result = (|| -> Result<()> {
                    let entry = entry?;
                    if naming::is_payroll_year_directory_name(&entry.file_name().to_string_lossy())
                    {
                        checked_path(&entry.path())?;
                        if entry.file_type()?.is_dir() {
                            years.push(entry.path());
                        }
                    }
                    Ok(())
                })();
                if let Err(e) = result {
                    failures.push(format!("Filing pending while enumerating {}: {e}. Check access and Resume incomplete filing.",base.display()));
                }
            }
        }
        Err(e) => failures.push(format!(
            "Filing pending while enumerating {}: {e}. Reconnect/check permissions and resume.",
            base.display()
        )),
    }
    years.sort();
    (years, failures)
}

/// Missing current registered files are reported, never matched to an archive by
/// filename/hash. This also exposes path mismatches following an older DB restore.
pub(super) fn missing_registrations(db: &Connection, pa: i64) -> Result<Vec<String>> {
    let paths=db.prepare("SELECT stored_path FROM imported_payroll_documents WHERE personal_assistant_id=?1 AND superseded_by IS NULL UNION SELECT stored_path FROM payslip_revisions WHERE personal_assistant_id=?1 AND is_current=1 UNION SELECT s.pdf_path FROM payroll_timesheet_snapshot_states s JOIN payroll_timesheets p ON p.id=s.payroll_timesheet_id WHERE p.personal_assistant_id=?1")?.query_map([pa],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut failures = Vec::new();
    for path in paths {
        match Path::new(&path).try_exists() {
            Ok(true)=>{},
            Ok(false)=>failures.push(format!("Filing pending: registered payroll document is unavailable at {path}. Reconnect its drive or review the verified recovery copy. Archived counterparts are not automatically adopted; retain them and seek reconciliation.")),
            Err(e)=>failures.push(format!("Filing pending: cannot inspect registered document {path} ({:?}). Check access and reconnect; no filesystem reconciliation was performed.",e.kind()))
        }
    }
    Ok(failures)
}
