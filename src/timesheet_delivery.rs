//! Production timesheet delivery identity, claims and append-only review evidence.
use crate::payroll_evidence::{now, Result};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};

const PAYLOADS: &[(&str, &str, &str)] = &[
    (
        "items",
        "payroll_timesheet_worked_item_snapshots",
        "payroll_submission_items",
    ),
    (
        "weeks",
        "payroll_timesheet_weeks",
        "payroll_submission_weeks",
    ),
    (
        "leave",
        "payroll_timesheet_annual_leave",
        "payroll_submission_leave",
    ),
    (
        "holidays",
        "payroll_timesheet_public_holidays",
        "payroll_submission_holidays",
    ),
    (
        "corrections",
        "payroll_correction_applications",
        "payroll_submission_corrections",
    ),
];

pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    let tx = db.unchecked_transaction()?;
    tx.execute_batch("CREATE TABLE timesheet_documents (
        id INTEGER PRIMARY KEY, payroll_timesheet_id INTEGER NOT NULL,
        pdf_path TEXT NOT NULL, pdf_sha256 TEXT NOT NULL, pdf_bytes BLOB,
        generated_at TEXT NOT NULL, legacy INTEGER NOT NULL DEFAULT 0, legacy_submission_id INTEGER UNIQUE);
        ALTER TABLE payroll_timesheet_snapshot_states ADD COLUMN document_id INTEGER REFERENCES timesheet_documents(id);
        ALTER TABLE payroll_submissions ADD COLUMN document_id INTEGER REFERENCES timesheet_documents(id);
        CREATE TABLE timesheet_delivery_attempts (
        id INTEGER PRIMARY KEY, intent_id TEXT NOT NULL UNIQUE,
        payroll_timesheet_id INTEGER NOT NULL, document_id INTEGER NOT NULL REFERENCES timesheet_documents(id),
        submission_id INTEGER REFERENCES payroll_submissions(id),
        classification TEXT NOT NULL CHECK(classification IN ('first_send','resend')),
        recipients TEXT NOT NULL, pdf_path TEXT NOT NULL, message_id TEXT NOT NULL UNIQUE,
        started_at TEXT NOT NULL, completed_at TEXT,
        outcome TEXT NOT NULL CHECK(outcome IN ('uncertain','accepted','confirmed_not_sent')),
        detail TEXT NOT NULL, resolved_at TEXT, payroll_department_notes TEXT);
        CREATE UNIQUE INDEX timesheet_unresolved_attempt ON timesheet_delivery_attempts(payroll_timesheet_id)
            WHERE outcome='uncertain' AND resolved_at IS NULL;
        CREATE TABLE timesheet_delivery_reviews (
        id INTEGER PRIMARY KEY, attempt_id INTEGER REFERENCES timesheet_delivery_attempts(id),
        payroll_timesheet_id INTEGER NOT NULL, decision TEXT NOT NULL CHECK(decision IN ('accepted','confirmed_not_sent')),
        reason TEXT NOT NULL CHECK(length(trim(reason))>0), actor TEXT NOT NULL, reviewed_at TEXT NOT NULL, original_evidence TEXT NOT NULL);
        INSERT INTO timesheet_documents(payroll_timesheet_id,pdf_path,pdf_sha256,pdf_bytes,generated_at,legacy,legacy_submission_id)
            SELECT payroll_timesheet_id,pdf_path,pdf_sha256,pdf_bytes,COALESCE(submitted_at,''),1,id FROM payroll_submissions ORDER BY id;
        UPDATE payroll_submissions SET document_id=(SELECT d.id FROM timesheet_documents d
            WHERE d.legacy_submission_id=payroll_submissions.id);
        INSERT INTO timesheet_documents(payroll_timesheet_id,pdf_path,pdf_sha256,generated_at,legacy)
            SELECT s.payroll_timesheet_id,s.pdf_path,s.pdf_sha256,s.generated_at,1 FROM payroll_timesheet_snapshot_states s
            WHERE s.state<>'submitted' OR NOT EXISTS(SELECT 1 FROM payroll_submissions p WHERE p.payroll_timesheet_id=s.payroll_timesheet_id AND p.pdf_sha256=s.pdf_sha256 AND p.pdf_path=s.pdf_path);
        UPDATE payroll_timesheet_snapshot_states SET document_id=CASE WHEN state='submitted' THEN
            COALESCE((SELECT p.document_id FROM payroll_submissions p WHERE p.payroll_timesheet_id=payroll_timesheet_snapshot_states.payroll_timesheet_id AND p.pdf_sha256=payroll_timesheet_snapshot_states.pdf_sha256 AND p.pdf_path=payroll_timesheet_snapshot_states.pdf_path ORDER BY p.id DESC LIMIT 1),
            (SELECT MAX(id) FROM timesheet_documents d WHERE d.payroll_timesheet_id=payroll_timesheet_snapshot_states.payroll_timesheet_id))
            ELSE (SELECT MAX(id) FROM timesheet_documents d WHERE d.payroll_timesheet_id=payroll_timesheet_snapshot_states.payroll_timesheet_id) END;
        CREATE UNIQUE INDEX timesheet_document_submission ON payroll_submissions(document_id) WHERE document_id IS NOT NULL;
        CREATE TRIGGER immutable_timesheet_document BEFORE UPDATE ON timesheet_documents
        WHEN NEW.id<>OLD.id OR NEW.payroll_timesheet_id<>OLD.payroll_timesheet_id OR NEW.pdf_path<>OLD.pdf_path OR NEW.pdf_sha256<>OLD.pdf_sha256 OR NEW.generated_at<>OLD.generated_at OR NEW.legacy<>OLD.legacy OR NEW.legacy_submission_id IS NOT OLD.legacy_submission_id OR (OLD.pdf_bytes IS NOT NULL AND NEW.pdf_bytes IS NOT OLD.pdf_bytes)
        BEGIN SELECT RAISE(ABORT,'Immutable timesheet document'); END;
        CREATE TRIGGER retain_timesheet_document BEFORE DELETE ON timesheet_documents BEGIN SELECT RAISE(ABORT,'Retained timesheet document'); END;
        CREATE TRIGGER immutable_delivery_attempt BEFORE UPDATE ON timesheet_delivery_attempts
        WHEN NEW.intent_id<>OLD.intent_id OR NEW.payroll_timesheet_id<>OLD.payroll_timesheet_id OR NEW.document_id<>OLD.document_id OR NEW.classification<>OLD.classification OR NEW.recipients<>OLD.recipients OR NEW.pdf_path<>OLD.pdf_path OR NEW.message_id<>OLD.message_id OR NEW.started_at<>OLD.started_at OR NEW.payroll_department_notes IS NOT OLD.payroll_department_notes
        OR (OLD.completed_at IS NOT NULL AND (NEW.outcome<>OLD.outcome OR NEW.detail<>OLD.detail OR NEW.completed_at IS NOT OLD.completed_at))
        OR (OLD.submission_id IS NOT NULL AND NEW.submission_id IS NOT OLD.submission_id)
        OR (OLD.resolved_at IS NOT NULL AND NEW.resolved_at IS NOT OLD.resolved_at)
        BEGIN SELECT RAISE(ABORT,'Immutable delivery evidence'); END;
        CREATE TRIGGER retain_delivery_attempt BEFORE DELETE ON timesheet_delivery_attempts BEGIN SELECT RAISE(ABORT,'Retained delivery attempt'); END;
        CREATE TRIGGER retain_delivery_review BEFORE DELETE ON timesheet_delivery_reviews BEGIN SELECT RAISE(ABORT,'Retained delivery review'); END;
        CREATE TRIGGER immutable_delivery_review BEFORE UPDATE ON timesheet_delivery_reviews BEGIN SELECT RAISE(ABORT,'Immutable delivery review'); END;
        UPDATE schema_version SET version=36;")?;
    for (name, source, _) in PAYLOADS {
        tx.execute_batch(&format!("CREATE TABLE timesheet_attempt_{name} AS SELECT 0 AS attempt_id,s.* FROM {source} s WHERE 0;
            CREATE TRIGGER immutable_attempt_{name} BEFORE UPDATE ON timesheet_attempt_{name} BEGIN SELECT RAISE(ABORT,'Immutable claimed payroll payload'); END;
            CREATE TRIGGER retain_attempt_{name} BEFORE DELETE ON timesheet_attempt_{name} BEGIN SELECT RAISE(ABORT,'Retained claimed payroll payload'); END;"))?;
    }
    // Published older binaries do not reject future schemas. Their connections lack
    // this function, so these triggers fail closed rather than accepting old writes.
    let tables = tx
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for table in tables {
        for operation in ["INSERT", "UPDATE", "DELETE"] {
            tx.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS dpt36_{table}_{operation} BEFORE {operation} ON {table} BEGIN SELECT CASE WHEN dpt_schema_version()<36 THEN RAISE(ABORT,'Incompatible application schema') END; END;"))?;
        }
    }
    tx.commit()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Intent {
    pub intent_id: String,
    pub record: i64,
    pub document: i64,
    pub digest: String,
    pub path: String,
    pub resend: bool,
}
pub fn blocked(db: &Connection, record: i64) -> rusqlite::Result<bool> {
    db.query_row("SELECT EXISTS(SELECT 1 FROM timesheet_delivery_attempts WHERE payroll_timesheet_id=?1 AND outcome='uncertain' AND resolved_at IS NULL)", [record], |r|r.get(0))
}
pub fn capture(db: &Connection, record: i64) -> Result<Intent> {
    if blocked(db, record)? {
        return Err("Delivery uncertain — explicit review required".into());
    }
    let legacy_uncertain:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_timesheets p JOIN payroll_timesheet_email_status e ON e.personal_assistant_id=p.personal_assistant_id AND e.payroll_year=p.payroll_year AND e.cycle_number=p.cycle_number WHERE p.id=?1 AND e.sent_at LIKE 'indeterminate:%')",[record],|r|r.get(0))?;
    if legacy_uncertain {
        return Err("Legacy delivery uncertain — explicit review required".into());
    }
    let (document,digest,state,path):(i64,String,String,String) = db.query_row("SELECT document_id,pdf_sha256,state,pdf_path FROM payroll_timesheet_snapshot_states WHERE payroll_timesheet_id=?1",[record],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
    if state == "indeterminate" {
        return Err("Legacy delivery uncertain — explicit review required".into());
    }
    Ok(Intent {
        intent_id: db.query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))?,
        record,
        document,
        digest,
        path,
        resend: state == "submitted",
    })
}
pub fn retain_current_bytes(db: &Connection, record: i64, bytes: &[u8]) -> Result<()> {
    let (document,digest):(i64,String)=db.query_row("SELECT document_id,pdf_sha256 FROM payroll_timesheet_snapshot_states WHERE payroll_timesheet_id=?1",[record],|r|Ok((r.get(0)?,r.get(1)?)))?;
    if format!("{:x}", Sha256::digest(bytes)) != digest {
        return Err("PDF digest changed".into());
    }
    db.execute(
        "UPDATE timesheet_documents SET pdf_bytes=?1 WHERE id=?2 AND pdf_bytes IS NULL",
        params![bytes, document],
    )?;
    Ok(())
}
pub fn bytes(db: &Connection, intent: &Intent) -> Result<Vec<u8>> {
    let (_original_path,digest,retained):(String,String,Option<Vec<u8>>) = db.query_row("SELECT pdf_path,pdf_sha256,pdf_bytes FROM timesheet_documents WHERE id=?1 AND payroll_timesheet_id=?2",params![intent.document,intent.record],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    let current_path:String=db.query_row("SELECT pdf_path FROM payroll_timesheet_snapshot_states WHERE payroll_timesheet_id=?1 AND document_id=?2",params![intent.record,intent.document],|r|r.get(0))?;
    if current_path != intent.path {
        return Err("Document location changed; review a new selection".into());
    }
    let disk = std::fs::read(&current_path)?;
    if digest != intent.digest || format!("{:x}", Sha256::digest(&disk)) != digest {
        return Err("Approved PDF changed; review a new selection".into());
    }
    if retained.as_ref().is_some_and(|b| b != &disk) {
        return Err("PDF differs from immutable retained bytes".into());
    }
    Ok(retained.unwrap_or(disk))
}

pub enum Claim {
    AlreadyAccepted,
    Claimed(i64),
}
#[cfg(test)]
pub fn claim(
    db: &Connection,
    intent: &Intent,
    recipients: &str,
    message_id: &str,
    pdf: &[u8],
) -> Result<Claim> {
    claim_validated(db, intent, recipients, message_id, pdf, |_| Ok(()))
}
fn claim_validated<F>(
    db: &Connection,
    intent: &Intent,
    recipients: &str,
    message_id: &str,
    pdf: &[u8],
    validate: F,
) -> Result<Claim>
where
    F: FnOnce(&Connection) -> Result<()>,
{
    let tx = db.unchecked_transaction()?;
    // Obtain the writer lock before reading state (also protects concurrent instances).
    tx.execute("UPDATE schema_version SET version=version", [])?;
    let prior: Option<(String, Option<String>)> = tx
        .query_row(
            "SELECT outcome,resolved_at FROM timesheet_delivery_attempts WHERE intent_id=?1",
            [&intent.intent_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((outcome, resolved)) = prior {
        let reviewed_accepted:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM timesheet_delivery_reviews r JOIN timesheet_delivery_attempts a ON a.id=r.attempt_id WHERE a.intent_id=?1 AND r.decision='accepted')",[&intent.intent_id],|r|r.get(0))?;
        if outcome == "accepted" || (resolved.is_some() && reviewed_accepted) {
            return Ok(Claim::AlreadyAccepted);
        }
        return Err(
            "Dispatch intent already attempted; start a fresh selection after review".into(),
        );
    }
    let current = capture(&tx, intent.record)?;
    if current.document != intent.document
        || current.digest != intent.digest
        || current.resend != intent.resend
        || current.path != intent.path
    {
        return Err("Document or send classification changed; review a new selection".into());
    }
    validate(&tx)?;
    if bytes(&tx, intent)? != pdf {
        return Err("Approved immutable bytes changed".into());
    }
    if format!("{:x}", Sha256::digest(pdf)) != intent.digest {
        return Err("Approved PDF digest changed".into());
    }
    if !intent.resend {
        crate::payroll_evidence::lifecycle::verify_candidate_evidence(&tx, intent.record)?;
        let competing:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM payroll_timesheet_worked_item_snapshots s JOIN timesheet_attempt_items i ON (s.timesheet_id IS NOT NULL AND s.timesheet_id=i.timesheet_id) OR (s.direct_shift_id IS NOT NULL AND s.direct_shift_id=i.direct_shift_id) JOIN timesheet_delivery_attempts a ON a.id=i.attempt_id WHERE s.payroll_timesheet_id=?1 AND a.payroll_timesheet_id<>?1 AND a.outcome='uncertain' AND a.resolved_at IS NULL UNION SELECT 1 FROM payroll_correction_applications c JOIN timesheet_attempt_corrections i ON i.correction_id=c.correction_id JOIN timesheet_delivery_attempts a ON a.id=i.attempt_id WHERE c.payroll_timesheet_id=?1 AND a.payroll_timesheet_id<>?1 AND a.outcome='uncertain' AND a.resolved_at IS NULL)",[intent.record],|r|r.get(0))?;
        if competing {
            return Err("Represented work/corrections belong to another unresolved attempt; resolve it before sending".into());
        }
    }
    retain_current_bytes(&tx, intent.record, pdf)?;
    let submission: Option<i64> = if intent.resend {
        Some(tx.query_row(
            "SELECT id FROM payroll_submissions WHERE document_id=?1 ORDER BY id DESC LIMIT 1",
            [intent.document],
            |r| r.get(0),
        )?)
    } else {
        None
    };
    tx.execute("INSERT INTO timesheet_delivery_attempts(intent_id,payroll_timesheet_id,document_id,submission_id,classification,recipients,message_id,started_at,pdf_path,outcome,detail,payroll_department_notes) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'uncertain','Claimed before transport; acceptance not yet established',(SELECT payroll_department_notes FROM payroll_timesheets WHERE id=?2))",params![intent.intent_id,intent.record,intent.document,submission,if intent.resend {"resend"} else {"first_send"},recipients,message_id,now(),intent.path])?;
    let attempt = tx.last_insert_rowid();
    tx.execute("INSERT INTO sickness_attempt_evidence SELECT ?1,document_id,evidence,correction_id FROM sickness_document_evidence WHERE document_id=?2",params![attempt,intent.document])?;
    if !intent.resend {
        tx.execute("UPDATE payroll_timesheet_snapshot_states SET state='indeterminate',indeterminate_at=?1 WHERE payroll_timesheet_id=?2 AND document_id=?3 AND state='candidate'",params![now(),intent.record,intent.document])?;
        for (name, source, _) in PAYLOADS {
            tx.execute(&format!("INSERT INTO timesheet_attempt_{name} SELECT ?1,s.* FROM {source} s WHERE payroll_timesheet_id=?2"),params![attempt,intent.record])?;
        }
    }
    tx.commit()?;
    Ok(Claim::Claimed(attempt))
}

fn finalise(db: &Connection, attempt: i64, accepted: bool, at: &str) -> Result<()> {
    let (record,document,classification):(i64,i64,String)=db.query_row("SELECT payroll_timesheet_id,document_id,classification FROM timesheet_delivery_attempts WHERE id=?1",[attempt],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    if classification == "resend" {
        return Ok(());
    }
    let changed=db.execute("UPDATE payroll_timesheet_snapshot_states SET state=?1,indeterminate_at=NULL,submitted_at=CASE WHEN ?2 THEN ?3 ELSE NULL END WHERE payroll_timesheet_id=?4 AND document_id=?5 AND state='indeterminate'",params![if accepted {"submitted"} else {"candidate"},accepted,at,record,document])?;
    if changed != 1 {
        return Err("Protected document changed; cannot finalise".into());
    }
    if !accepted {
        return Ok(());
    }
    db.execute("INSERT INTO payroll_submissions(payroll_timesheet_id,submitted_at,supersedes_id,pdf_path,pdf_sha256,pdf_bytes,payroll_department_notes,document_id) SELECT ?1,?2,(SELECT MAX(id) FROM payroll_submissions WHERE payroll_timesheet_id=?1),a.pdf_path,d.pdf_sha256,d.pdf_bytes,a.payroll_department_notes,d.id FROM timesheet_documents d JOIN timesheet_delivery_attempts a ON a.document_id=d.id WHERE a.id=?3",params![record,at,attempt])?;
    let submission = db.last_insert_rowid();
    db.execute("INSERT INTO sickness_submission_evidence SELECT ?1,document_id,evidence,correction_id FROM sickness_attempt_evidence WHERE attempt_id=?2",params![submission,attempt])?;
    for (name, _, target) in PAYLOADS {
        let columns = db
            .prepare(&format!("PRAGMA table_info(timesheet_attempt_{name})"))?
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let selected = columns
            .iter()
            .skip(1)
            .filter(|c| !(*name == "corrections" && c.as_str() == "payroll_timesheet_id"))
            .map(|c| format!("s.\"{c}\""))
            .collect::<Vec<_>>()
            .join(",");
        db.execute(&format!("INSERT INTO {target} SELECT ?1,{selected} FROM timesheet_attempt_{name} s WHERE attempt_id=?2"),params![submission,attempt])?;
    }
    db.execute(
        "UPDATE timesheet_delivery_attempts SET submission_id=?1 WHERE id=?2",
        params![submission, attempt],
    )?;
    db.execute("INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) SELECT personal_assistant_id,payroll_year,cycle_number,'timesheet',?1 FROM payroll_timesheets WHERE id=?2 ON CONFLICT(personal_assistant_id,payroll_year,cycle_number,email_type) DO UPDATE SET sent_at=excluded.sent_at",params![at,record])?;
    Ok(())
}
pub fn finish(db: &Connection, attempt: i64, outcome: &str, detail: &str) -> Result<()> {
    let tx = db.unchecked_transaction()?;
    tx.execute("UPDATE schema_version SET version=version", [])?;
    let at = now();
    let changed=tx.execute("UPDATE timesheet_delivery_attempts SET outcome=?1,detail=?2,completed_at=?3 WHERE id=?4 AND outcome='uncertain' AND resolved_at IS NULL",params![outcome,detail,at,attempt])?;
    if changed != 1 {
        return Err("Attempt already finalised".into());
    }
    if outcome != "uncertain" {
        finalise(&tx, attempt, outcome == "accepted", &at)?;
    }
    tx.commit()?;
    Ok(())
}
pub fn review(db: &mut Connection, attempt: i64, decision: &str, reason: &str) -> Result<()> {
    let _transport_guard = production_lock(db)?;
    if reason.trim().is_empty() {
        return Err("Document review evidence and reason before resolving".into());
    }
    if !["accepted", "confirmed_not_sent"].contains(&decision) {
        return Err("Invalid review decision".into());
    }
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let record:i64=tx.query_row("SELECT payroll_timesheet_id FROM timesheet_delivery_attempts WHERE id=?1 AND outcome='uncertain' AND resolved_at IS NULL",[attempt],|r|r.get(0))?;
    let at = now();
    finalise(&tx, attempt, decision == "accepted", &at)?;
    tx.execute("INSERT INTO timesheet_delivery_reviews(attempt_id,payroll_timesheet_id,decision,reason,actor,reviewed_at,original_evidence) VALUES (?1,?2,?3,?4,'local_employer',?5,'See retained uncertain delivery attempt')",params![attempt,record,decision,reason.trim(),at])?;
    tx.execute(
        "UPDATE timesheet_delivery_attempts SET resolved_at=?1 WHERE id=?2",
        params![at, attempt],
    )?;
    tx.commit()?;
    Ok(())
}

#[derive(Debug)]
pub struct History {
    pub id: i64,
    pub document: i64,
    pub submission: Option<i64>,
    pub period: String,
    pub classification: String,
    pub recipients: String,
    pub message_id: String,
    pub started: String,
    pub completed: Option<String>,
    pub outcome: String,
    pub detail: String,
    pub resolution: Option<String>,
}
pub fn history(db: &Connection) -> Result<Vec<History>> {
    Ok(db.prepare("SELECT a.id,a.document_id,COALESCE((SELECT first_name||' '||surname||' — ' FROM personal_assistants WHERE id=p.personal_assistant_id),'')||p.payroll_year||' payroll period '||p.cycle_number,a.classification,a.recipients,a.message_id,a.started_at,a.completed_at,a.outcome,a.detail,(SELECT r.decision||': '||r.reason||' ('||r.actor||', '||r.reviewed_at||')' FROM timesheet_delivery_reviews r WHERE r.attempt_id=a.id ORDER BY r.id DESC LIMIT 1),a.submission_id FROM timesheet_delivery_attempts a JOIN payroll_timesheets p ON p.id=a.payroll_timesheet_id ORDER BY a.id DESC")?.query_map([],|r|Ok(History{id:r.get(0)?,document:r.get(1)?,period:r.get(2)?,classification:r.get(3)?,recipients:r.get(4)?,message_id:r.get(5)?,started:r.get(6)?,completed:r.get(7)?,outcome:r.get(8)?,detail:r.get(9)?,resolution:r.get(10)?,submission:r.get(11)?}))?.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Claims survive crashes; only proof of non-acceptance releases retry eligibility.
pub fn execute<F, V>(
    db: &Connection,
    intent: &Intent,
    recipients: &str,
    message_id: &str,
    pdf: &[u8],
    validate: V,
    send: F,
) -> Result<bool>
where
    F: FnOnce() -> std::result::Result<(), crate::email_service::ProductionSendError>,
    V: FnOnce(&Connection) -> Result<()>,
{
    let _transport_guard = production_lock(db)?;
    let attempt = match claim_validated(db, intent, recipients, message_id, pdf, validate)? {
        Claim::AlreadyAccepted => return Ok(false),
        Claim::Claimed(id) => id,
    };
    match send() {
        Ok(()) => {
            if let Err(error) = finish(
                db,
                attempt,
                "accepted",
                "SMTP accepted the message; recipient delivery is not proven",
            ) {
                let detail=format!("SMTP reported acceptance, but submission finalisation failed: {error}. Explicit review required");
                // Best effort: retain known transport evidence even when the submission
                // transaction failed. If the DB itself is unavailable, the claim survives.
                let _ = finish(db, attempt, "uncertain", &detail);
                return Err(format!("Delivery uncertain: {detail}").into());
            }
            Ok(true)
        }
        Err(crate::email_service::ProductionSendError::NotSent(detail)) => {
            finish(db, attempt, "confirmed_not_sent", &detail)?;
            Err(format!("Confirmed not sent: {detail}; start a new selection for retry").into())
        }
        Err(crate::email_service::ProductionSendError::Uncertain(detail)) => {
            finish(db, attempt, "uncertain", &detail)?;
            Err(
                format!("Delivery uncertain: {detail}; explicit review required, do not retry")
                    .into(),
            )
        }
    }
}

/// Legacy uncertainty has no attempt to invent. Retain the old marker in review evidence.
pub fn review_legacy(db: &mut Connection, record: i64, decision: &str, reason: &str) -> Result<()> {
    let _transport_guard = production_lock(db)?;
    if reason.trim().is_empty() || !["accepted", "confirmed_not_sent"].contains(&decision) {
        return Err("Document a valid review decision and supporting evidence".into());
    }
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if blocked(&tx, record)? {
        return Err("Review the recorded attempt instead".into());
    }
    let snapshot:Option<(String,Option<i64>,String,Option<String>)>=tx.query_row("SELECT state,document_id,pdf_sha256,indeterminate_at FROM payroll_timesheet_snapshot_states WHERE payroll_timesheet_id=?1",[record],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let statuses:String=tx.query_row("SELECT COALESCE(group_concat(email_type||'='||COALESCE(sent_at,'')), '') FROM payroll_timesheet_email_status e JOIN payroll_timesheets p ON e.personal_assistant_id=p.personal_assistant_id AND e.payroll_year=p.payroll_year AND e.cycle_number=p.cycle_number WHERE p.id=?1",[record],|r|r.get(0))?;
    if !snapshot
        .as_ref()
        .is_some_and(|(state, _, _, _)| state == "indeterminate")
        && !statuses.contains("timesheet=indeterminate:")
    {
        return Err("No unresolved legacy timesheet evidence".into());
    }
    let at = now();
    if decision == "accepted" {
        let (state,document,digest,_)=snapshot.as_ref().ok_or("Cannot establish an immutable submission without its PDF snapshot; retain the block until evidence is recovered")?;
        if state != "submitted" {
            let intent=Intent{intent_id:String::new(),record,document:document.ok_or("Legacy document identity unavailable")?,digest:digest.clone(),path:tx.query_row("SELECT pdf_path FROM payroll_timesheet_snapshot_states WHERE payroll_timesheet_id=?1",[record],|r|r.get(0))?,resend:false};
            let pdf = bytes(&tx, &intent)?;
            retain_current_bytes(&tx, record, &pdf)?;
            crate::payroll_evidence::lifecycle::archive_submission(&tx, record, &at)?;
        }
    }
    tx.execute("INSERT INTO timesheet_delivery_reviews(attempt_id,payroll_timesheet_id,decision,reason,actor,reviewed_at,original_evidence) VALUES(NULL,?1,?2,?3,'local_employer',?4,?5)",params![record,decision,reason.trim(),at,format!("Legacy snapshot={snapshot:?}; original email statuses={statuses}")])?;
    if snapshot
        .as_ref()
        .is_some_and(|(state, _, _, _)| state == "indeterminate")
    {
        tx.execute("UPDATE payroll_timesheet_snapshot_states SET state=?1,indeterminate_at=NULL,submitted_at=?2 WHERE payroll_timesheet_id=?3",params![if decision=="accepted" {"submitted"} else {"candidate"},if decision=="accepted" {Some(at.clone())} else {None},record])?;
    }
    if decision == "accepted" {
        tx.execute("INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) SELECT personal_assistant_id,payroll_year,cycle_number,'timesheet',?1 FROM payroll_timesheets WHERE id=?2 ON CONFLICT(personal_assistant_id,payroll_year,cycle_number,email_type) DO UPDATE SET sent_at=excluded.sent_at",params![at,record])?;
    } else {
        tx.execute("DELETE FROM payroll_timesheet_email_status WHERE email_type='timesheet' AND sent_at LIKE 'indeterminate:%' AND (personal_assistant_id,payroll_year,cycle_number) IN (SELECT personal_assistant_id,payroll_year,cycle_number FROM payroll_timesheets WHERE id=?1)",[record])?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payroll_snapshot_service::{publish_candidate, CandidatePublication};
    use crate::payroll_worked_item_repository::PayrollWorkedItemRepository;
    fn fixture() -> (tempfile::TempDir, Connection, Intent, Vec<u8>) {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::database::open(dir.path().join("delivery.sqlite")).unwrap();
        crate::database::create_schema(&db).unwrap();
        db.execute("INSERT INTO payroll_timesheets(id,personal_assistant_id,payroll_year,cycle_number,created_at,updated_at,payroll_department_notes) VALUES(1,1,'2026/27',1,'created','updated','original payroll note')",[]).unwrap();
        let repo = PayrollWorkedItemRepository::new(
            crate::database::open(dir.path().join("delivery.sqlite")).unwrap(),
        );
        let path = dir.path().join("timesheet.pdf");
        publish_candidate(
            &repo,
            CandidatePublication {
                payroll_timesheet_id: 1,
                items: &[],
                final_pdf_path: &path,
                generated_at: "generated",
                previous_cycle_minutes: 0,
                week_ids: &[0; 4],
                week_totals_minutes: &[0; 4],
            },
            |p| {
                std::fs::write(p, b"%PDF-1.4 original immutable bytes")?;
                Ok(())
            },
        )
        .unwrap();
        let intent = capture(&db, 1).unwrap();
        let pdf = bytes(&db, &intent).unwrap();
        (dir, db, intent, pdf)
    }
    fn count(db: &Connection, table: &str) -> i64 {
        db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }
    fn send(db: &Connection, i: &Intent, pdf: &[u8]) -> Result<bool> {
        execute(
            db,
            i,
            "To: payroll; CC: employer; BCC: PA",
            &format!("<{}@test.local>", i.intent_id),
            pdf,
            |_| Ok(()),
            || Ok(()),
        )
    }
    #[test]
    fn first_send_and_resend_preserve_one_submission_and_every_attempt() {
        let (_dir, db, intent, pdf) = fixture();
        assert!(!intent.resend);
        assert!(send(&db, &intent, &pdf).unwrap());
        assert!(!send(&db, &intent, &pdf).unwrap());
        assert_eq!(count(&db, "timesheet_delivery_attempts"), 1);
        assert_eq!(count(&db, "payroll_submissions"), 1);
        let resend = capture(&db, 1).unwrap();
        assert!(resend.resend);
        assert!(send(&db, &resend, &pdf).unwrap());
        assert_eq!(count(&db, "payroll_submissions"), 1);
        assert_eq!(count(&db, "timesheet_delivery_attempts"), 2);
        assert_eq!(
            db.query_row::<i64, _, _>(
                "SELECT COUNT(DISTINCT submission_id) FROM timesheet_delivery_attempts",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            db.query_row::<Vec<u8>, _, _>("SELECT pdf_bytes FROM payroll_submissions", [], |r| r
                .get(0))
                .unwrap(),
            pdf
        );
        assert!(db
            .execute("UPDATE timesheet_documents SET pdf_bytes=x'00'", [])
            .is_err());
        assert!(db
            .execute("DELETE FROM timesheet_delivery_attempts", [])
            .is_err());
    }
    #[test]
    fn uncertainty_survives_restart_and_review_preserves_original_evidence() {
        let (dir, db, intent, pdf) = fixture();
        let result = execute(
            &db,
            &intent,
            "Payroll",
            "<uncertain@test.local>",
            &pdf,
            |_| Ok(()),
            || {
                Err(crate::email_service::ProductionSendError::Uncertain(
                    "acknowledgement lost".into(),
                ))
            },
        );
        assert!(result.is_err());
        drop(db);
        let mut db = crate::database::open(dir.path().join("delivery.sqlite")).unwrap();
        assert!(blocked(&db, 1).unwrap());
        assert!(capture(&db, 1).is_err());
        assert!(review(&mut db, 1, "confirmed_not_sent", " ").is_err());
        review(
            &mut db,
            1,
            "confirmed_not_sent",
            "Payroll and SMTP queue reviewed; no acceptance",
        )
        .unwrap();
        assert!(!blocked(&db, 1).unwrap());
        let retry = capture(&db, 1).unwrap();
        assert!(!retry.resend);
        assert!(send(&db, &retry, &pdf).unwrap());
        assert_eq!(count(&db, "timesheet_delivery_attempts"), 2);
        assert_eq!(
            db.query_row::<String, _, _>(
                "SELECT outcome FROM timesheet_delivery_attempts WHERE id=1",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            "uncertain"
        );
        assert!(review(&mut db, 1, "accepted", "second decision").is_err());
        assert!(db
            .execute(
                "UPDATE timesheet_delivery_reviews SET reason='erase evidence'",
                []
            )
            .is_err());
    }
    #[test]
    fn accepted_review_freezes_claimed_payload_and_duplicate_dispatch_skips() {
        let (_dir, mut db, intent, pdf) = fixture();
        claim(&db, &intent, "Payroll", "<interrupted@test.local>", &pdf).unwrap();
        // Simulate maintenance or external changes after the durable pre-SMTP capture.
        db.execute(
            "UPDATE payroll_timesheets SET payroll_department_notes='later note'",
            [],
        )
        .unwrap();
        review(
            &mut db,
            1,
            "accepted",
            "SMTP queue confirms acceptance of this Message-ID",
        )
        .unwrap();
        assert!(!send(&db, &intent, &pdf).unwrap());
        assert_eq!(count(&db, "payroll_submissions"), 1);
        assert_eq!(
            db.query_row::<String, _, _>(
                "SELECT payroll_department_notes FROM payroll_submissions",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            "original payroll note"
        );
    }
    #[test]
    fn uncertain_resend_blocks_settled_record_without_changing_settlement() {
        let (_dir, mut db, intent, pdf) = fixture();
        send(&db, &intent, &pdf).unwrap();
        db.execute("INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) VALUES(1,'2026/27',1,'payslip','settled-original')",[]).unwrap();
        let resend = capture(&db, 1).unwrap();
        claim(
            &db,
            &resend,
            "Payroll",
            "<resend-uncertain@test.local>",
            &pdf,
        )
        .unwrap();
        assert!(crate::payroll_evidence::lifecycle::ensure_generatable(&db, 1).is_err());
        assert!(capture(&db, 1).is_err());
        review(
            &mut db,
            2,
            "accepted",
            "Payroll confirmed receiving the duplicate PDF",
        )
        .unwrap();
        assert!(capture(&db, 1).unwrap().resend);
        assert_eq!(count(&db, "payroll_submissions"), 1);
        assert_eq!(
            db.query_row::<String, _, _>(
                "SELECT sent_at FROM payroll_timesheet_email_status WHERE email_type='payslip'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            "settled-original"
        );
    }
    #[test]
    fn explicit_rejection_releases_candidate_but_requires_fresh_dispatch_intent() {
        let (_dir, db, intent, pdf) = fixture();
        assert!(execute(
            &db,
            &intent,
            "Payroll",
            "<reject@test.local>",
            &pdf,
            |_| Ok(()),
            || Err(crate::email_service::ProductionSendError::NotSent(
                "451 rejected before DATA".into()
            ))
        )
        .is_err());
        assert!(!blocked(&db, 1).unwrap());
        assert!(send(&db, &intent, &pdf).is_err());
        assert!(!capture(&db, 1).unwrap().resend);
        assert_eq!(count(&db, "payroll_submissions"), 0);
        assert!(send(&db, &capture(&db, 1).unwrap(), &pdf).unwrap());
    }
    #[test]
    fn concurrent_connections_claim_only_one_attempt() {
        let (dir, db, intent, pdf) = fixture();
        let another = capture(&db, 1).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let mut threads = Vec::new();
        for (n, choice) in [intent, another].into_iter().enumerate() {
            let path = dir.path().join("delivery.sqlite");
            let bytes = pdf.clone();
            let gate = barrier.clone();
            threads.push(std::thread::spawn(move || {
                let connection = crate::database::open(path).unwrap();
                gate.wait();
                matches!(
                    claim(
                        &connection,
                        &choice,
                        "Payroll",
                        &format!("<claim-{n}@test.local>"),
                        &bytes
                    ),
                    Ok(Claim::Claimed(_))
                )
            }));
        }
        assert_eq!(
            threads
                .into_iter()
                .filter_map(|t| t.join().ok())
                .filter(|claimed| *claimed)
                .count(),
            1
        );
        assert_eq!(count(&db, "timesheet_delivery_attempts"), 1);
    }
    #[test]
    fn stale_document_and_changed_bytes_never_claim_transport() {
        let (_dir, db, intent, pdf) = fixture();
        db.execute("INSERT INTO timesheet_documents(payroll_timesheet_id,pdf_path,pdf_sha256,pdf_bytes,generated_at) SELECT payroll_timesheet_id,pdf_path,pdf_sha256,pdf_bytes,'new generation' FROM timesheet_documents WHERE id=?1",[intent.document]).unwrap();
        let new = db.last_insert_rowid();
        db.execute("UPDATE payroll_timesheet_snapshot_states SET document_id=?1 WHERE payroll_timesheet_id=1",[new]).unwrap();
        assert!(claim(&db, &intent, "Payroll", "<stale@test.local>", &pdf).is_err());
        let fresh = capture(&db, 1).unwrap();
        std::fs::write(&intent.path, b"changed file").unwrap();
        assert!(bytes(&db, &fresh).is_err());
        assert!(claim(&db, &fresh, "Payroll", "<changed@test.local>", &pdf).is_err());
        assert_eq!(count(&db, "timesheet_delivery_attempts"), 0);
    }
    #[test]
    fn moved_document_retains_identity_and_requires_fresh_path_confirmation() {
        let (dir, db, intent, pdf) = fixture();
        let moved = dir.path().join("Archived.pdf");
        std::fs::rename(&intent.path, &moved).unwrap();
        db.execute(
            "UPDATE payroll_timesheet_snapshot_states SET pdf_path=?1 WHERE payroll_timesheet_id=1",
            [moved.to_str()],
        )
        .unwrap();
        assert!(bytes(&db, &intent).is_err());
        let fresh = capture(&db, 1).unwrap();
        assert_eq!(fresh.document, intent.document);
        assert!(send(&db, &fresh, &pdf).unwrap());
        assert_eq!(
            db.query_row::<String, _, _>("SELECT pdf_path FROM payroll_submissions", [], |r| r
                .get(0))
                .unwrap(),
            moved.to_string_lossy()
        );
    }
    #[test]
    fn upgrade_35_preserves_history_and_never_backfills_attempts() {
        let (dir, db, intent, pdf) = fixture();
        send(&db, &intent, &pdf).unwrap();
        let original: String = db
            .query_row("SELECT submitted_at FROM payroll_submissions", [], |r| {
                r.get(0)
            })
            .unwrap();
        crate::database::tests::remove_schema_36_fixture(&db);
        db.execute("UPDATE schema_version SET version=35", [])
            .unwrap();
        // Both published Linux/Windows 1.0.4 and macOS 1.0.5 use schema 35.
        crate::database::create_schema(&db).unwrap();
        crate::database::create_schema(&db).unwrap();
        assert_eq!(count(&db, "timesheet_delivery_attempts"), 0);
        assert_eq!(count(&db, "timesheet_documents"), 1);
        assert_eq!(count(&db, "payroll_submissions"), 1);
        assert_eq!(
            db.query_row::<String, _, _>("SELECT submitted_at FROM payroll_submissions", [], |r| r
                .get(0))
                .unwrap(),
            original
        );
        assert_eq!(bytes(&db, &capture(&db, 1).unwrap()).unwrap(), pdf);
        drop(db);
        let reopened = crate::database::open(dir.path().join("delivery.sqlite")).unwrap();
        crate::database::create_schema(&reopened).unwrap();
        assert!(capture(&reopened, 1).unwrap().resend);
    }
    #[test]
    fn migration_36_failure_rolls_back_every_new_table_and_column() {
        let (_dir, db, _, _) = fixture();
        crate::database::tests::remove_schema_36_fixture(&db);
        db.execute_batch("UPDATE schema_version SET version=35; CREATE TRIGGER fail36 BEFORE UPDATE ON schema_version BEGIN SELECT RAISE(ABORT,'migration fixture'); END;").unwrap();
        assert!(crate::database::create_schema(&db).is_err());
        assert!(db.prepare("SELECT * FROM timesheet_documents").is_err());
        assert!(db
            .prepare("SELECT document_id FROM payroll_submissions")
            .is_err());
        assert_eq!(
            db.query_row::<i64, _, _>("SELECT version FROM schema_version", [], |r| r.get(0))
                .unwrap(),
            35
        );
        db.execute_batch("DROP TRIGGER fail36").unwrap();
        crate::database::create_schema(&db).unwrap();
    }
    #[test]
    fn legacy_uncertainty_is_reviewed_without_inventing_an_attempt() {
        let (_dir, mut db, _, _) = fixture();
        db.execute("UPDATE payroll_timesheet_snapshot_states SET state='indeterminate',indeterminate_at='original-legacy-marker'",[]).unwrap();
        review_legacy(
            &mut db,
            1,
            "confirmed_not_sent",
            "Payroll and relay confirm no acceptance",
        )
        .unwrap();
        assert_eq!(count(&db, "timesheet_delivery_attempts"), 0);
        assert!(!capture(&db, 1).unwrap().resend);
        assert!(db
            .query_row::<String, _, _>(
                "SELECT original_evidence FROM timesheet_delivery_reviews",
                [],
                |r| r.get(0)
            )
            .unwrap()
            .contains("original-legacy-marker"));
    }
    #[test]
    fn old_connections_cannot_write_new_schema_and_future_schema_is_rejected() {
        let (dir, db, _, _) = fixture();
        let old = Connection::open(dir.path().join("delivery.sqlite")).unwrap();
        assert!(old
            .execute(
                "UPDATE payroll_timesheets SET payroll_department_notes='old app'",
                []
            )
            .is_err());
        assert!(old
            .execute("UPDATE schema_version SET version=35", [])
            .is_err());
        assert!(old
            .execute("DELETE FROM timesheet_delivery_attempts", [])
            .is_err());
        db.execute("UPDATE schema_version SET version=39", [])
            .unwrap();
        assert!(crate::database::create_schema(&db)
            .unwrap_err()
            .to_string()
            .contains("newer"));
        assert_eq!(
            db.query_row::<i64, _, _>("SELECT version FROM schema_version", [], |r| r.get(0))
                .unwrap(),
            39
        );
    }

    #[test]
    fn identical_regeneration_preserves_sent_identity_and_corrections_create_new_identity() {
        let (dir, db, intent, pdf) = fixture();
        send(&db, &intent, &pdf).unwrap();
        let repo = PayrollWorkedItemRepository::new(
            crate::database::open(dir.path().join("delivery.sqlite")).unwrap(),
        );
        let path = std::path::Path::new(&intent.path);
        publish_candidate(
            &repo,
            CandidatePublication {
                payroll_timesheet_id: 1,
                items: &[],
                final_pdf_path: path,
                generated_at: "regenerated",
                previous_cycle_minutes: 0,
                week_ids: &[0; 4],
                week_totals_minutes: &[0; 4],
            },
            |p| {
                std::fs::write(p, &pdf)?;
                Ok(())
            },
        )
        .unwrap();
        let unchanged = capture(&db, 1).unwrap();
        assert_eq!(unchanged.document, intent.document);
        assert!(unchanged.resend);
        db.execute(
            "UPDATE payroll_timesheets SET payroll_department_notes='corrected payroll note'",
            [],
        )
        .unwrap();
        publish_candidate(
            &repo,
            CandidatePublication {
                payroll_timesheet_id: 1,
                items: &[],
                final_pdf_path: path,
                generated_at: "corrected",
                previous_cycle_minutes: 0,
                week_ids: &[0; 4],
                week_totals_minutes: &[0; 4],
            },
            |p| {
                std::fs::write(p, b"%PDF-1.4 corrected payroll note")?;
                Ok(())
            },
        )
        .unwrap();
        let corrected = capture(&db, 1).unwrap();
        assert_ne!(corrected.document, intent.document);
        assert!(!corrected.resend);
        assert_eq!(count(&db, "payroll_submissions"), 1);
    }
    #[test]
    fn identical_regeneration_recovers_missing_original_at_new_destination() {
        for missing in [false, true] {
            let (dir, db, intent, pdf) = fixture();
            send(&db, &intent, &pdf).unwrap();
            let original_submission: (String, String, String) = db
                .query_row(
                    "SELECT pdf_path,pdf_sha256,submitted_at FROM payroll_submissions",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap();
            if missing {
                std::fs::remove_file(&intent.path).unwrap();
            }
            let destination = dir.path().join("new-output").join("restored.pdf");
            let repo = PayrollWorkedItemRepository::new(
                crate::database::open(dir.path().join("delivery.sqlite")).unwrap(),
            );
            publish_candidate(
                &repo,
                CandidatePublication {
                    payroll_timesheet_id: 1,
                    items: &[],
                    final_pdf_path: &destination,
                    generated_at: "recovered",
                    previous_cycle_minutes: 0,
                    week_ids: &[0; 4],
                    week_totals_minutes: &[0; 4],
                },
                |path| {
                    std::fs::write(path, &pdf)?;
                    Ok(())
                },
            )
            .unwrap();
            let recovered = capture(&db, 1).unwrap();
            assert_eq!(recovered.document, intent.document);
            assert!(recovered.resend);
            assert_eq!(recovered.path, destination.to_string_lossy());
            assert_eq!(bytes(&db, &recovered).unwrap(), pdf);
            assert!(bytes(&db, &intent).is_err());
            let preserved: (String, String, String) = db
                .query_row(
                    "SELECT pdf_path,pdf_sha256,submitted_at FROM payroll_submissions",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap();
            assert_eq!(preserved, original_submission);
            assert_eq!(count(&db, "timesheet_documents"), 1);
            assert_eq!(count(&db, "timesheet_delivery_attempts"), 1);
        }
    }

    #[test]
    fn failed_identical_publication_retains_registered_path_identity_and_sent_state() {
        let (dir, db, intent, pdf) = fixture();
        send(&db, &intent, &pdf).unwrap();
        let destination = dir.path().join("directory-instead-of-file.pdf");
        std::fs::create_dir(&destination).unwrap();
        let before = crate::database_recovery::fingerprint(&db).unwrap();
        let repo = PayrollWorkedItemRepository::new(
            crate::database::open(dir.path().join("delivery.sqlite")).unwrap(),
        );
        assert!(publish_candidate(
            &repo,
            CandidatePublication {
                payroll_timesheet_id: 1,
                items: &[],
                final_pdf_path: &destination,
                generated_at: "failed",
                previous_cycle_minutes: 0,
                week_ids: &[0; 4],
                week_totals_minutes: &[0; 4],
            },
            |path| {
                std::fs::write(path, &pdf)?;
                Ok(())
            }
        )
        .is_err());
        assert_eq!(crate::database_recovery::fingerprint(&db).unwrap(), before);
        let retained = capture(&db, 1).unwrap();
        assert_eq!(retained.path, intent.path);
        assert!(retained.resend);
        assert_eq!(bytes(&db, &retained).unwrap(), pdf);
    }

    #[test]
    fn pending_work_membership_cannot_be_claimed_by_another_payroll_record() {
        let (_dir, db, intent, pdf) = fixture();
        db.execute("INSERT INTO payroll_timesheet_worked_item_snapshots(payroll_timesheet_id,week_number,source_type,timesheet_id,work_date,worked_minutes,pay_rate_id,pay_rate_effective_date,total_hourly_rate,captured_at) VALUES(1,1,'imported_shift',55,'2026-04-01',60,1,'2026-01-01',12,'captured')",[]).unwrap();
        claim(
            &db,
            &intent,
            "Payroll",
            "<pending-evidence@test.local>",
            &pdf,
        )
        .unwrap();
        db.execute_batch("INSERT INTO payroll_timesheets(id,personal_assistant_id,payroll_year,cycle_number,created_at,updated_at) VALUES(2,1,'2026/27',2,'created','updated');
            INSERT INTO timesheet_documents(payroll_timesheet_id,pdf_path,pdf_sha256,pdf_bytes,generated_at) SELECT 2,pdf_path,pdf_sha256,pdf_bytes,generated_at FROM timesheet_documents WHERE payroll_timesheet_id=1;
            INSERT INTO payroll_timesheet_snapshot_states(payroll_timesheet_id,state,pdf_path,pdf_sha256,generated_at,document_id) SELECT 2,'candidate',pdf_path,pdf_sha256,generated_at,id FROM timesheet_documents WHERE payroll_timesheet_id=2;
            INSERT INTO payroll_timesheet_worked_item_snapshots(payroll_timesheet_id,week_number,source_type,timesheet_id,work_date,worked_minutes,pay_rate_id,pay_rate_effective_date,total_hourly_rate,captured_at) VALUES(2,1,'imported_shift',55,'2026-04-01',60,1,'2026-01-01',12,'captured');").unwrap();
        let second = capture(&db, 2).unwrap();
        crate::sickness_service::capture_document(&db, 2, second.document).unwrap();
        assert!(claim(
            &db,
            &second,
            "Payroll",
            "<competing-evidence@test.local>",
            &pdf
        )
        .err()
        .unwrap()
        .to_string()
        .contains("another unresolved attempt"));
        assert_eq!(count(&db, "timesheet_delivery_attempts"), 1);
    }
    #[test]
    fn active_sender_cannot_be_reviewed_until_os_lock_is_released() {
        let (_dir, mut db, intent, pdf) = fixture();
        claim(&db, &intent, "Payroll", "<active@test.local>", &pdf).unwrap();
        let guard = production_lock(&db).unwrap();
        assert!(review(&mut db, 1, "confirmed_not_sent", "Verified queue")
            .unwrap_err()
            .to_string()
            .contains("active"));
        assert!(blocked(&db, 1).unwrap());
        drop(guard);
        review(
            &mut db,
            1,
            "confirmed_not_sent",
            "Sender stopped; queue and payroll verified",
        )
        .unwrap();
        assert!(!blocked(&db, 1).unwrap());
    }
    #[test]
    fn legacy_marker_without_snapshot_requires_evidence_and_preserves_marker() {
        let (_dir, mut db, _, _) = fixture();
        db.execute("DELETE FROM payroll_timesheet_snapshot_states", [])
            .unwrap();
        db.execute("INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) VALUES(1,'2026/27',1,'timesheet','indeterminate:legacy-no-snapshot')",[]).unwrap();
        assert!(review_legacy(
            &mut db,
            1,
            "accepted",
            "Acceptance claimed but original PDF missing"
        )
        .is_err());
        review_legacy(
            &mut db,
            1,
            "confirmed_not_sent",
            "Verified payroll and SMTP logs establish no acceptance",
        )
        .unwrap();
        assert_eq!(count(&db, "timesheet_delivery_attempts"), 0);
        assert!(db
            .query_row::<String, _, _>(
                "SELECT original_evidence FROM timesheet_delivery_reviews",
                [],
                |r| r.get(0)
            )
            .unwrap()
            .contains("indeterminate:legacy-no-snapshot"));
    }
}

/// An OS lock prevents review from releasing a claim while another instance is
/// still in SMTP. Kernel release on process exit allows review after a crash.
/// Never delete this sidecar: unlinking an active lock could split ownership.
pub(crate) fn production_lock(db: &Connection) -> Result<Option<std::fs::File>> {
    let Some(path) = db.path().filter(|p| !p.is_empty() && *p != ":memory:") else {
        return Ok(None);
    };
    let path = std::fs::canonicalize(path)?.with_extension("timesheet-delivery.lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    file.try_lock().map_err(|e|format!("Payroll sending, publication or recovery is active in another instance; wait for it to finish before retrying or reviewing: {e}"))?;
    Ok(Some(file))
}
