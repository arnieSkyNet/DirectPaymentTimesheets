//! Immutable PA bundle dispatch evidence. The OS lock spans validation, claim,
//! SMTP and finalisation; SQLite transactions never span network transport.
use crate::email_service::{PayrollEmailPreview, ProductionSendError};
use crate::payslip_delivery_service::{PayslipDeliveryIdentity, PayslipEmailBundle};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};
pub type Result<T> = crate::payroll_evidence::Result<T>;

pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    let tx = db.unchecked_transaction()?;
    tx.execute_batch("CREATE TABLE payroll_bundle_attempts(
        id INTEGER PRIMARY KEY, personal_assistant_id INTEGER NOT NULL,
        payroll_year TEXT, cycle_number INTEGER, marker TEXT NOT NULL UNIQUE,
        message_id TEXT NOT NULL UNIQUE, intent TEXT NOT NULL, started_at TEXT NOT NULL,
        recipients TEXT NOT NULL, settings_digest TEXT NOT NULL);
        CREATE TABLE payroll_bundle_attachments(
        attempt_id INTEGER NOT NULL REFERENCES payroll_bundle_attempts(id), ordinal INTEGER NOT NULL,
        document_id INTEGER, document_type TEXT NOT NULL, document_year TEXT,
        path TEXT NOT NULL, filename TEXT NOT NULL, sha256 TEXT NOT NULL CHECK(length(sha256)=64),
        bytes BLOB NOT NULL, PRIMARY KEY(attempt_id,ordinal));
        CREATE TABLE payroll_bundle_events(
        id INTEGER PRIMARY KEY, attempt_id INTEGER NOT NULL REFERENCES payroll_bundle_attempts(id),
        outcome TEXT NOT NULL CHECK(outcome IN ('accepted','confirmed_not_sent','uncertain')),
        detail TEXT NOT NULL, recorded_at TEXT NOT NULL);
        CREATE UNIQUE INDEX payroll_bundle_transport_result ON payroll_bundle_events(attempt_id);
        CREATE TABLE payroll_delivery_reviews(
        id INTEGER PRIMARY KEY, attempt_id INTEGER REFERENCES payroll_bundle_attempts(id),
        legacy_target TEXT, original_evidence TEXT NOT NULL,
        decision TEXT NOT NULL CHECK(decision IN ('accepted','confirmed_not_sent','uncertain')),
        reason TEXT NOT NULL CHECK(length(trim(reason))>0), actor TEXT NOT NULL CHECK(length(trim(actor))>0),
        reviewed_at TEXT NOT NULL,
        CHECK((attempt_id IS NOT NULL AND legacy_target IS NULL) OR (attempt_id IS NULL AND legacy_target IS NOT NULL)));
        CREATE UNIQUE INDEX payroll_bundle_resolution ON payroll_delivery_reviews(attempt_id) WHERE decision<>'uncertain';
        CREATE UNIQUE INDEX payroll_legacy_resolution ON payroll_delivery_reviews(legacy_target,original_evidence) WHERE decision<>'uncertain';
        CREATE TABLE timesheet_inconclusive_reviews(
        id INTEGER PRIMARY KEY, attempt_id INTEGER, payroll_timesheet_id INTEGER NOT NULL,
        reason TEXT NOT NULL CHECK(length(trim(reason))>0), actor TEXT NOT NULL, reviewed_at TEXT NOT NULL,
        original_evidence TEXT NOT NULL);
        UPDATE schema_version SET version=41;")?;
    // Do not backfill recipients or attempts for legacy delivery markers.
    for table in [
        "payroll_bundle_attempts",
        "payroll_bundle_attachments",
        "payroll_bundle_events",
        "payroll_delivery_reviews",
        "timesheet_inconclusive_reviews",
    ] {
        for op in ["UPDATE", "DELETE"] {
            tx.execute_batch(&format!("CREATE TRIGGER stage8_{table}_{op} BEFORE {op} ON {table} BEGIN SELECT RAISE(ABORT,'Retained immutable delivery evidence'); END;"))?;
        }
    }
    let tables = tx
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for table in tables {
        for op in ["INSERT", "UPDATE", "DELETE"] {
            tx.execute_batch(&format!("CREATE TRIGGER dpt41_{table}_{op} BEFORE {op} ON {} BEGIN SELECT CASE WHEN dpt_schema_version()<41 THEN RAISE(ABORT,'Incompatible application schema') END; END;", crate::database_recovery::quote(&table)))?;
        }
    }
    tx.commit()
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
pub fn recipients(p: &PayrollEmailPreview) -> String {
    format!(
        "To: {}; CC: {}; BCC: {}",
        p.to,
        p.cc.as_deref().unwrap_or("none"),
        p.bcc.as_deref().unwrap_or("none")
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Approval {
    pub bundle: PayslipEmailBundle,
    pub preview: PayrollEmailPreview,
    pub bytes: Vec<Vec<u8>>,
    pub digests: Vec<String>,
    pub settings_digest: String,
}
impl Approval {
    pub fn capture(
        bundle: PayslipEmailBundle,
        preview: PayrollEmailPreview,
        settings_digest: String,
    ) -> Result<Self> {
        if bundle.paths.is_empty()
            || bundle.paths.len() != bundle.attachment_identity.len()
            || bundle.paths.len() != preview.attachment_filenames.len()
        {
            return Err("Incomplete attachment approval".into());
        }
        if bundle.document_ids.len()
            != bundle
                .attachment_identity
                .iter()
                .filter(|(kind, _)| *kind != "payslip")
                .count()
            || bundle
                .attachment_identity
                .iter()
                .filter(|(kind, _)| *kind == "payslip")
                .count()
                > 1
            || bundle
                .attachment_identity
                .iter()
                .any(|(kind, _)| !matches!(*kind, "payslip" | "p45" | "p60"))
            || preview.attachment_path != bundle.paths[0].to_string_lossy()
            || preview.additional_attachment_paths != bundle.paths[1..]
        {
            return Err("Approved attachment identities do not match the email".into());
        }
        let bytes = bundle
            .paths
            .iter()
            .map(std::fs::read)
            .collect::<std::io::Result<Vec<_>>>()?;
        if bytes.iter().any(|b| !b.starts_with(b"%PDF-")) {
            return Err("Attachment is not a PDF".into());
        }
        let digests = bytes.iter().map(|b| digest(b)).collect();
        Ok(Self {
            bundle,
            preview,
            bytes,
            digests,
            settings_digest,
        })
    }
    pub fn verify(&self, current: &Self) -> Result<()> {
        if self != current {
            return Err("Recipients, settings, attachments or delivery eligibility changed; approve a fresh selection".into());
        }
        Ok(())
    }
}

// Uses only the ledger's recorded identities, never a mutable file path.
fn transition(
    db: &Connection,
    pa: i64,
    year: Option<&str>,
    cycle: Option<i64>,
    ordinary: bool,
    documents: &[i64],
    expected: Option<&str>,
    next: Option<&str>,
) -> Result<()> {
    if ordinary {
        let (year, cycle) = year
            .zip(cycle)
            .ok_or("Ordinary payslip has no cycle identity")?;
        db.execute("INSERT OR IGNORE INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) VALUES(?1,?2,?3,'payslip',NULL)",params![pa,year,cycle])?;
        if db.execute("UPDATE payroll_timesheet_email_status SET sent_at=?1 WHERE personal_assistant_id=?2 AND payroll_year=?3 AND cycle_number=?4 AND email_type='payslip' AND sent_at IS ?5",params![next,pa,year,cycle,expected])?!=1 { return Err("Payslip delivery state changed".into()); }
        db.execute("UPDATE payslip_revisions SET sent_at=?1 WHERE personal_assistant_id=?2 AND payroll_year=?3 AND cycle_number=?4 AND is_current=1 AND sent_at IS ?5",params![next,pa,year,cycle,expected])?;
    }
    let mut seen = std::collections::HashSet::new();
    for id in documents {
        if !seen.insert(id) || db.execute("UPDATE imported_payroll_documents SET sent_at=?1 WHERE id=?2 AND personal_assistant_id=?3 AND sent_at IS ?4 AND history_state='needs_sending' AND superseded_by IS NULL",params![next,id,pa,expected])?!=1 { return Err("Supplement delivery eligibility changed".into()); }
    }
    Ok(())
}

/// Caller refreshes addresses, eligibility, settings and reads all attachment bytes
/// inside the immediate claim transaction. Transport receives those verified bytes.
pub fn dispatch<V, S>(
    db: &mut Connection,
    pa: i64,
    identity: Option<PayslipDeliveryIdentity<'_>>,
    approved: &Approval,
    validate: V,
    send: S,
) -> Result<()>
where
    V: FnOnce(&Connection) -> Result<Approval>,
    S: FnOnce(
        &PayrollEmailPreview,
        &[Vec<u8>],
        &str,
    ) -> std::result::Result<(), ProductionSendError>,
{
    let _lock = crate::timesheet_delivery::production_lock(db)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = validate(&tx)?;
    approved.verify(&current)?;
    if current.bundle.paths.is_empty() {
        return Err("No eligible attachments".into());
    }
    // Recheck registry identity against the bytes that will actually be sent.
    let mut documents = current.bundle.document_ids.iter();
    for (index, (kind, year)) in current.bundle.attachment_identity.iter().enumerate() {
        if digest(&current.bytes[index]) != current.digests[index] {
            return Err("Approved attachment digest does not bind its bytes".into());
        }
        if *kind == "payslip" {
            let attachment = current
                .bundle
                .ordinary_attachment
                .as_ref()
                .ok_or("Missing ordinary payslip byte identity")?;
            if attachment.0 != current.bundle.paths[index] || attachment.1 != current.digests[index]
            {
                return Err("Ordinary payslip identity changed".into());
            }
            let identity = identity
                .as_ref()
                .ok_or("Ordinary payslip cycle identity missing")?;
            let revision_protected:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM payslip_revisions WHERE personal_assistant_id=?1 AND payroll_year=?2 AND cycle_number=?3 AND is_current=1 AND sent_at IS NOT NULL)",params![pa,identity.payroll_year,identity.cycle_number],|r|r.get(0))?;
            if revision_protected {
                return Err("Current revision already has delivery evidence; a fresh eligible approval is required".into());
            }
            if let Some((path, hash)) = crate::payroll_replacement::current_payslip(
                &tx,
                pa,
                identity.payroll_year,
                identity.cycle_number,
            )? {
                if path != attachment.0 || hash != attachment.1 {
                    return Err("Current payslip revision changed".into());
                }
            }
        } else {
            let id = documents.next().ok_or("Supplement identity missing")?;
            let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM imported_payroll_documents WHERE id=?1 AND personal_assistant_id=?2 AND stored_path=?3 AND sha256=?4 AND document_type=?5 AND document_year IS ?6 AND sent_at IS NULL AND history_state='needs_sending' AND superseded_by IS NULL)",params![id,pa,current.bundle.paths[index].to_string_lossy(),current.digests[index],kind,year],|r|r.get(0))?;
            if !valid {
                return Err("Supplement registry, bytes or delivery history changed".into());
            }
        }
    }
    let at = now();
    let nonce: String = tx.query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))?;
    let marker = format!("indeterminate:{at}:{nonce}");
    let message_id = format!("<{nonce}@directpaymenttimesheets.local>");
    let year = identity.as_ref().map(|i| i.payroll_year);
    let cycle = identity.as_ref().map(|i| i.cycle_number);
    if identity
        .as_ref()
        .is_some_and(|i| i.personal_assistant_id != pa)
    {
        return Err("PA identity mismatch".into());
    }
    transition(
        &tx,
        pa,
        year,
        cycle,
        current.bundle.email_types.contains(&"payslip"),
        &current.bundle.document_ids,
        None,
        Some(&marker),
    )?;
    tx.execute("INSERT INTO payroll_bundle_attempts(personal_assistant_id,payroll_year,cycle_number,marker,message_id,intent,started_at,recipients,settings_digest) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![pa,year,cycle,marker,message_id,format!("From: {}\n{}\nSubject: {}\n\n{}",current.preview.from,recipients(&current.preview),current.preview.subject,current.preview.body),at,recipients(&current.preview),current.settings_digest])?;
    let attempt = tx.last_insert_rowid();
    let mut ids = current.bundle.document_ids.iter();
    for (index, ((kind, year), path)) in current
        .bundle
        .attachment_identity
        .iter()
        .zip(&current.bundle.paths)
        .enumerate()
    {
        let id = if *kind == "payslip" {
            None
        } else {
            ids.next().copied()
        };
        tx.execute("INSERT INTO payroll_bundle_attachments(attempt_id,ordinal,document_id,document_type,document_year,path,filename,sha256,bytes) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![attempt,index as i64,id,kind,year,path.to_string_lossy(),current.preview.attachment_filenames[index],current.digests[index],current.bytes[index]])?;
    }
    tx.commit()?;
    let result = send(&current.preview, &current.bytes, &message_id);
    let (outcome, detail) = match &result {
        Ok(()) => (
            "accepted",
            "SMTP server accepted the message; recipient receipt is not established".to_string(),
        ),
        Err(ProductionSendError::NotSent(s)) => ("confirmed_not_sent", s.clone()),
        Err(ProductionSendError::Uncertain(s)) => ("uncertain", s.clone()),
    };
    // Record the transport evidence independently of delivery status finalisation.
    db.execute("INSERT INTO payroll_bundle_events(attempt_id,outcome,detail,recorded_at) VALUES(?1,?2,?3,?4)",params![attempt,outcome,detail,now()]).map_err(|e|format!("Attempt {attempt} remains protected: transport evidence could not be persisted: {e}"))?;
    if outcome != "uncertain" {
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transition(
            &tx,
            pa,
            year,
            cycle,
            current.bundle.email_types.contains(&"payslip"),
            &current.bundle.document_ids,
            Some(&marker),
            if outcome == "accepted" {
                Some(at.as_str())
            } else {
                None
            },
        )
        .map_err(|e| {
            format!(
                "Attempt {attempt}: {outcome}; finalisation failed, protected review required: {e}"
            )
        })?;
        tx.commit()?;
    }
    result.map_err(|e| {
        format!(
            "Attempt {attempt}: {e}; {}",
            if outcome == "uncertain" {
                "delivery remains uncertain; automatic retry is blocked"
            } else {
                "non-acceptance established; fresh approval required"
            }
        )
        .into()
    })
}

#[derive(Debug)]
pub struct ReviewTarget {
    pub key: String,
    pub label: String,
    pub evidence: String,
    pub unresolved: bool,
}
pub fn history(db: &Connection) -> Result<Vec<ReviewTarget>> {
    let mut targets = Vec::new();
    let mut statement=db.prepare("SELECT a.id,a.personal_assistant_id,a.marker,a.recipients,a.message_id,a.started_at,COALESCE(e.outcome,'uncertain'),COALESCE(e.detail,'Interrupted reservation: no transport result'),e.recorded_at,(SELECT decision||': '||reason||' — '||actor||' at '||reviewed_at FROM payroll_delivery_reviews r WHERE r.attempt_id=a.id ORDER BY id DESC LIMIT 1),EXISTS(SELECT 1 FROM payroll_timesheet_email_status s WHERE s.personal_assistant_id=a.personal_assistant_id AND s.sent_at=a.marker UNION ALL SELECT 1 FROM imported_payroll_documents d WHERE d.personal_assistant_id=a.personal_assistant_id AND d.sent_at=a.marker),COALESCE(p.first_name||' '||p.surname,'PA '||a.personal_assistant_id) FROM payroll_bundle_attempts a LEFT JOIN payroll_bundle_events e ON e.attempt_id=a.id LEFT JOIN personal_assistants p ON p.id=a.personal_assistant_id ORDER BY a.id DESC")?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, Option<String>>(8)?,
                r.get::<_, Option<String>>(9)?,
                r.get::<_, bool>(10)?,
                r.get::<_, String>(11)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, recipients, message, started, outcome, detail, completed, review, unresolved, name) in
        rows
    {
        let attachments=db.prepare("SELECT document_type,document_id,filename,sha256 FROM payroll_bundle_attachments WHERE attempt_id=?1 ORDER BY ordinal")?.query_map([id],|r|Ok(format!("{} {:?}: {}; SHA-256 {}",r.get::<_,String>(0)?,r.get::<_,Option<i64>>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let reviews=db.prepare("SELECT decision||': '||reason||' — '||actor||' at '||reviewed_at FROM payroll_delivery_reviews WHERE attempt_id=?1 ORDER BY id")?.query_map([id],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        targets.push(ReviewTarget{key:format!("attempt:{id}"),label:format!("{name} — attempt {id} — {outcome}"),evidence:format!("{recipients}\nMessage-ID: {message}\nStarted {started}; completed {completed:?}\n{detail}\n{}\n{}\nLatest review: {review:?}",attachments.join("\n"),reviews.join("\n")),unresolved});
    }
    let legacy=db.prepare("SELECT 'status:'||s.id,COALESCE(p.first_name||' '||p.surname,'PA '||s.personal_assistant_id)||' — payslip '||s.payroll_year||' cycle '||s.cycle_number,s.sent_at FROM payroll_timesheet_email_status s LEFT JOIN personal_assistants p ON p.id=s.personal_assistant_id WHERE s.email_type='payslip' AND s.sent_at LIKE 'indeterminate:%' AND NOT EXISTS(SELECT 1 FROM payroll_bundle_attempts a WHERE a.marker=s.sent_at) UNION ALL SELECT 'document:'||d.id,COALESCE(p.first_name||' '||p.surname,'PA '||d.personal_assistant_id)||' — '||d.document_type||' document '||d.id,d.sent_at FROM imported_payroll_documents d LEFT JOIN personal_assistants p ON p.id=d.personal_assistant_id WHERE d.sent_at LIKE 'indeterminate:%' AND NOT EXISTS(SELECT 1 FROM payroll_bundle_attempts a WHERE a.marker=d.sent_at) UNION ALL SELECT 'revision:'||r.id,COALESCE(p.first_name||' '||p.surname,'PA '||r.personal_assistant_id)||' — retained payslip revision '||r.id||' — '||r.payroll_year||' cycle '||r.cycle_number,r.sent_at FROM payslip_revisions r LEFT JOIN personal_assistants p ON p.id=r.personal_assistant_id WHERE r.sent_at LIKE 'indeterminate:%' AND NOT EXISTS(SELECT 1 FROM payroll_timesheet_email_status s WHERE s.personal_assistant_id=r.personal_assistant_id AND s.payroll_year=r.payroll_year AND s.cycle_number=r.cycle_number AND s.email_type='payslip' AND s.sent_at=r.sent_at) AND NOT EXISTS(SELECT 1 FROM payroll_bundle_attempts a WHERE a.marker=r.sent_at)")?.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (key, label, marker) in legacy {
        let attachment = if let Some(id) = key.strip_prefix("document:") {
            db.query_row("SELECT stored_path||'; registered SHA-256 '||sha256 FROM imported_payroll_documents WHERE id=?1",[id],|r|r.get::<_,String>(0))?
        } else if let Some(id) = key.strip_prefix("status:") {
            let revisions=db.prepare("SELECT r.stored_path||'; registered SHA-256 '||r.sha256 FROM payslip_revisions r JOIN payroll_timesheet_email_status s ON s.personal_assistant_id=r.personal_assistant_id AND s.payroll_year=r.payroll_year AND s.cycle_number=r.cycle_number WHERE s.id=?1 AND r.sent_at=s.sent_at")?.query_map([id],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            if revisions.is_empty() {
                "Legacy ordinary attachment and dispatched bytes unknown".into()
            } else {
                revisions.join("\n")
            }
        } else if let Some(id) = key.strip_prefix("revision:") {
            db.query_row("SELECT stored_path||'; registered SHA-256 '||sha256 FROM payslip_revisions WHERE id=?1",[id],|r|r.get(0))?
        } else {
            "Legacy attachment identity unknown".into()
        };
        let reviews=db.prepare("SELECT decision||': '||reason||' — '||actor||' at '||reviewed_at FROM payroll_delivery_reviews WHERE legacy_target=?1 ORDER BY id")?.query_map([&key],|r|r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?.join("\n");
        targets.push(ReviewTarget{key,label:format!("Legacy uncertain: {label}"),evidence:format!("Original marker: {marker}\nRecipients and Message-ID unknown; no attempt invented.\n{attachment}\nRegistered identity is not proof of original dispatched bytes.\n{reviews}"),unresolved:true});
    }
    Ok(targets)
}

pub fn review(
    db: &mut Connection,
    key: &str,
    expected_evidence: &str,
    decision: &str,
    reason: &str,
    actor: &str,
) -> Result<()> {
    let _lock = crate::timesheet_delivery::production_lock(db)?;
    if !["accepted", "confirmed_not_sent", "uncertain"].contains(&decision)
        || reason.trim().is_empty()
        || actor.trim().is_empty()
    {
        return Err("Decision, evidence reason and reviewer identity are required".into());
    }
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let target = history(&tx)?
        .into_iter()
        .find(|t| t.key == key && t.unresolved)
        .ok_or("Attempt is already resolved or no longer uncertain")?;
    if target.evidence != expected_evidence {
        return Err("Review evidence changed; confirm a fresh review".into());
    }
    let at = now();
    let attempt = key
        .strip_prefix("attempt:")
        .map(str::parse::<i64>)
        .transpose()?;
    if let Some(id) = attempt {
        let (pa,year,cycle,marker):(i64,Option<String>,Option<i64>,String)=tx.query_row("SELECT personal_assistant_id,payroll_year,cycle_number,marker FROM payroll_bundle_attempts WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
        // A retained accepted transport result rules out releasing this reservation.
        let outcome: Option<String> = tx
            .query_row(
                "SELECT outcome FROM payroll_bundle_events WHERE attempt_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        if decision == "confirmed_not_sent" && outcome.as_deref() == Some("accepted") {
            return Err("Retained SMTP acceptance contradicts a not-sent decision".into());
        }
        if decision != "uncertain" {
            let ordinary:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM payroll_bundle_attachments WHERE attempt_id=?1 AND document_type='payslip')",[id],|r|r.get(0))?;
            let ids=tx.prepare("SELECT document_id FROM payroll_bundle_attachments WHERE attempt_id=?1 AND document_id IS NOT NULL ORDER BY ordinal")?.query_map([id],|r|r.get::<_,i64>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            transition(
                &tx,
                pa,
                year.as_deref(),
                cycle,
                ordinary,
                &ids,
                Some(&marker),
                if decision == "accepted" {
                    Some(at.as_str())
                } else {
                    None
                },
            )?;
        }
    } else if decision != "uncertain" {
        let next = if decision == "accepted" {
            Some(at.as_str())
        } else {
            None
        };
        if let Some(id) = key.strip_prefix("document:") {
            if tx.execute("UPDATE imported_payroll_documents SET sent_at=?1 WHERE id=?2 AND sent_at LIKE 'indeterminate:%'",params![next,id])?!=1 {return Err("Legacy marker changed".into());}
        } else if let Some(id) = key.strip_prefix("status:") {
            tx.execute("UPDATE payslip_revisions SET sent_at=?1 WHERE is_current=1 AND (personal_assistant_id,payroll_year,cycle_number,sent_at) IN (SELECT personal_assistant_id,payroll_year,cycle_number,sent_at FROM payroll_timesheet_email_status WHERE id=?2)",params![next,id])?;
            if tx.execute("UPDATE payroll_timesheet_email_status SET sent_at=?1 WHERE id=?2 AND email_type='payslip' AND sent_at LIKE 'indeterminate:%'",params![next,id])?!=1 {return Err("Legacy marker changed".into());}
        } else if let Some(id) = key.strip_prefix("revision:") {
            if tx.execute("UPDATE payslip_revisions SET sent_at=?1 WHERE id=?2 AND sent_at LIKE 'indeterminate:%'",params![next,id])?!=1 {return Err("Legacy revision marker changed".into());}
            if decision == "accepted" {
                // Only a current revision can establish a missing cycle status.
                // Never overwrite a later accepted or independently uncertain status.
                tx.execute("INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) SELECT personal_assistant_id,payroll_year,cycle_number,'payslip',?1 FROM payslip_revisions WHERE id=?2 AND is_current=1 ON CONFLICT(personal_assistant_id,payroll_year,cycle_number,email_type) DO UPDATE SET sent_at=excluded.sent_at WHERE payroll_timesheet_email_status.sent_at IS NULL",params![next,id])?;
            }
        } else {
            return Err("Unknown review target".into());
        }
    }
    tx.execute("INSERT INTO payroll_delivery_reviews(attempt_id,legacy_target,original_evidence,decision,reason,actor,reviewed_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![attempt,if attempt.is_none(){Some(key)}else{None},target.evidence,decision,reason.trim(),actor.trim(),at])?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn remove_schema_41_fixture(db: &Connection) {
    let triggers=db.prepare("SELECT name FROM sqlite_master WHERE type='trigger' AND (name LIKE 'dpt41_%' OR name LIKE 'stage8_%')").unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    for t in triggers {
        db.execute_batch(&format!(
            "DROP TRIGGER {}",
            crate::database_recovery::quote(&t)
        ))
        .unwrap();
    }
    db.execute_batch("DROP TABLE IF EXISTS payroll_delivery_reviews; DROP TABLE IF EXISTS payroll_bundle_events; DROP TABLE IF EXISTS payroll_bundle_attachments; DROP TABLE IF EXISTS payroll_bundle_attempts; DROP TABLE IF EXISTS timesheet_inconclusive_reviews;").unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payroll_timesheet_email_repository::PayrollTimesheetEmailRepository;
    struct Fixture {
        dir: tempfile::TempDir,
        db: Connection,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let db = crate::database::open(dir.path().join("test.sqlite")).unwrap();
            crate::database::create_schema(&db).unwrap();
            db.execute_batch("INSERT INTO personal_assistants(id,first_name,surname) VALUES(1,'PA1','Test'),(2,'PA2','Test'),(3,'PA3','Test');").unwrap();
            let f = Self { dir, db };
            f.add(1, "p45");
            f.add(1, "p60");
            f.add(2, "p60");
            f
        }
        fn path(&self) -> std::path::PathBuf {
            self.dir.path().join("test.sqlite")
        }
        fn add(&self, pa: i64, kind: &str) -> i64 {
            let path = self.dir.path().join(format!("{pa}-{kind}.pdf"));
            std::fs::write(&path, format!("%PDF-1.4 {pa} {kind}")).unwrap();
            let repo =
                PayrollTimesheetEmailRepository::new(crate::database::open(self.path()).unwrap());
            let id = repo
                .register_document(pa, kind, &path, Some("2026/27"))
                .unwrap();
            repo.reconcile_document(id, true).unwrap();
            id
        }
    }
    fn capture(path: &std::path::Path, pa: i64, ordinary: bool) -> Result<Approval> {
        let repo = PayrollTimesheetEmailRepository::new(crate::database::open(path)?);
        let ordinary_path = path.parent().unwrap().join("ordinary.pdf");
        let identity = PayslipDeliveryIdentity {
            personal_assistant_id: pa,
            payroll_year: "2026/27",
            cycle_number: 1,
        };
        let bundle = crate::payslip_delivery_service::select_payroll_documents(
            &repo,
            pa,
            if ordinary {
                Some((identity, ordinary_path.as_path()))
            } else {
                None
            },
        )?;
        let preview = PayrollEmailPreview {
            from: "employer@example.com".into(),
            to: format!("pa{pa}@example.com"),
            cc: None,
            bcc: Some("employer@example.com".into()),
            subject: "Payroll documents".into(),
            body: "Approved body".into(),
            attachment_path: bundle
                .paths
                .first()
                .ok_or("No eligible attachments")?
                .to_string_lossy()
                .into(),
            additional_attachment_paths: bundle.paths.iter().skip(1).cloned().collect(),
            attachment_filenames: bundle
                .attachment_identity
                .iter()
                .map(|(kind, _)| format!("{kind}.pdf"))
                .collect(),
        };
        Approval::capture(bundle, preview, "settings".into())
    }
    fn dispatch(
        f: &mut Fixture,
        pa: i64,
        approval: &Approval,
        result: std::result::Result<(), ProductionSendError>,
    ) -> Result<()> {
        let path = f.path();
        super::dispatch(
            &mut f.db,
            pa,
            None,
            approval,
            |_| capture(&path, pa, false),
            |_, _, _| result,
        )
    }
    fn target(f: &Fixture) -> ReviewTarget {
        history(&f.db)
            .unwrap()
            .into_iter()
            .find(|t| t.unresolved)
            .unwrap()
    }
    #[test]
    fn ambiguous_errors_retain_bundle_and_block_automatic_retry_after_restart() {
        for detail in [
            "timeout after DATA",
            "TLS error",
            "connection reset",
            "generic SMTP failure",
        ] {
            let mut f = Fixture::new();
            let a = capture(&f.path(), 1, false).unwrap();
            assert!(dispatch(
                &mut f,
                1,
                &a,
                Err(ProductionSendError::Uncertain(detail.into()))
            )
            .is_err());
            assert!(capture(&f.path(), 1, false).is_err());
            let reopened = crate::database::open(f.path()).unwrap();
            assert!(history(&reopened).unwrap().iter().any(|t| t.unresolved));
            assert_eq!(
                f.db.query_row("SELECT outcome FROM payroll_bundle_events", [], |r| r
                    .get::<_, String>(0))
                    .unwrap(),
                "uncertain"
            );
            let mut called = false;
            let path = f.path();
            assert!(super::dispatch(
                &mut f.db,
                1,
                None,
                &a,
                |_| capture(&path, 1, false),
                |_, _, _| {
                    called = true;
                    Ok(())
                }
            )
            .is_err());
            assert!(!called);
        }
    }
    #[test]
    fn explicit_smtp_rejection_releases_all_mixed_documents_and_preserves_attempt() {
        let mut f = Fixture::new();
        let a = capture(&f.path(), 1, false).unwrap();
        assert!(dispatch(
            &mut f,
            1,
            &a,
            Err(ProductionSendError::NotSent("550 rejected".into()))
        )
        .is_err());
        let fresh = capture(&f.path(), 1, false).unwrap();
        dispatch(&mut f, 1, &fresh, Ok(())).unwrap();
        assert_eq!(
            f.db.query_row("SELECT count(*) FROM payroll_bundle_attempts", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            f.db.query_row("SELECT count(*) FROM payroll_bundle_attachments", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            4
        );
        assert!(history(&f.db).unwrap().iter().all(|t| !t.unresolved));
    }
    #[test]
    fn immutable_intent_exact_recipients_bytes_hash_and_message_id_are_retained() {
        let mut f = Fixture::new();
        let a = capture(&f.path(), 1, false).unwrap();
        dispatch(&mut f, 1, &a, Ok(())).unwrap();
        assert_eq!(
            f.db.query_row("SELECT recipients FROM payroll_bundle_attempts", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            "To: pa1@example.com; CC: none; BCC: employer@example.com"
        );
        assert!(f
            .db
            .query_row("SELECT message_id FROM payroll_bundle_attempts", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap()
            .ends_with("@directpaymenttimesheets.local>"));
        for i in 0..a.bytes.len() {
            let (bytes, hash): (Vec<u8>, String) =
                f.db.query_row(
                    "SELECT bytes,sha256 FROM payroll_bundle_attachments WHERE ordinal=?1",
                    [i as i64],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            assert_eq!(bytes, a.bytes[i]);
            assert_eq!(hash, digest(&bytes));
        }
        for table in [
            "payroll_bundle_attempts",
            "payroll_bundle_attachments",
            "payroll_bundle_events",
        ] {
            assert!(f.db.execute(&format!("DELETE FROM {table}"), []).is_err());
        }
        assert!(f
            .db
            .execute(
                "UPDATE payroll_bundle_attempts SET recipients='different'",
                []
            )
            .is_err());
    }
    #[test]
    fn changed_recipients_settings_documents_or_bytes_refuse_before_claim() {
        let mut f = Fixture::new();
        let approved = capture(&f.path(), 1, false).unwrap();
        for mode in 0..5 {
            let mut current = approved.clone();
            match mode {
                0 => current.preview.to = "other@example.com".into(),
                1 => current.preview.bcc = None,
                2 => current.settings_digest = "changed".into(),
                3 => current.bytes[0].push(0),
                _ => {
                    current.bundle.document_ids.pop();
                }
            }
            let mut called = false;
            assert!(super::dispatch(
                &mut f.db,
                1,
                None,
                &approved,
                |_| Ok(current),
                |_, _, _| {
                    called = true;
                    Ok(())
                }
            )
            .is_err());
            assert!(!called);
        }
        std::fs::write(&approved.bundle.paths[0], b"%PDF-1.4 replacement").unwrap();
        let path = f.path();
        assert!(super::dispatch(
            &mut f.db,
            1,
            None,
            &approved,
            |_| capture(&path, 1, false),
            |_, _, _| panic!("stale send")
        )
        .is_err());
        assert_eq!(
            f.db.query_row("SELECT count(*) FROM payroll_bundle_attempts", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn transport_never_rereads_mutable_paths_after_final_validation() {
        let mut f = Fixture::new();
        let a = capture(&f.path(), 1, false).unwrap();
        let path = f.path();
        super::dispatch(
            &mut f.db,
            1,
            None,
            &a,
            |_| capture(&path, 1, false),
            |preview, bytes, message_id| {
                for path in &a.bundle.paths {
                    std::fs::write(path, b"mutated after validation").unwrap();
                }
                assert_eq!(bytes, a.bytes);
                let message =
                    crate::email_service::prepare_bundle_message(preview, bytes, message_id)
                        .unwrap();
                let text = String::from_utf8(message.formatted()).unwrap();
                assert!(text.contains("Message-ID:"));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            f.db.query_row(
                "SELECT bytes FROM payroll_bundle_attachments WHERE ordinal=0",
                [],
                |r| r.get::<_, Vec<u8>>(0)
            )
            .unwrap(),
            a.bytes[0]
        );
    }
    #[test]
    fn interrupted_reservation_survives_restart_and_reviews_without_transport_result() {
        let mut f = Fixture::new();
        let a = capture(&f.path(), 1, false).unwrap();
        let path = f.path();
        let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            super::dispatch(
                &mut f.db,
                1,
                None,
                &a,
                |_| capture(&path, 1, false),
                |_, _, _| panic!("simulated process interruption"),
            )
        }));
        assert!(interrupted.is_err());
        assert!(capture(&path, 1, false).is_err());
        f.db = crate::database::open(path).unwrap();
        let t = target(&f);
        assert!(t.evidence.contains("Interrupted reservation"));
        review(
            &mut f.db,
            &t.key,
            &t.evidence,
            "accepted",
            "SMTP log confirms acceptance",
            "test_reviewer",
        )
        .unwrap();
        assert!(!history(&f.db).unwrap()[0].unresolved);
        assert_eq!(
            f.db.query_row("SELECT count(*) FROM payroll_bundle_events", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn acceptance_then_database_finalisation_failure_preserves_evidence_and_requires_review() {
        let mut f = Fixture::new();
        let a = capture(&f.path(), 1, false).unwrap();
        let path = f.path();
        let result = super::dispatch(
            &mut f.db,
            1,
            None,
            &a,
            |_| capture(&path, 1, false),
            |_, _, _| {
                crate::database::open(&path).unwrap().execute_batch("CREATE TRIGGER final_fail BEFORE UPDATE OF sent_at ON imported_payroll_documents WHEN OLD.sent_at LIKE 'indeterminate:%' AND NEW.sent_at NOT LIKE 'indeterminate:%' BEGIN SELECT RAISE(ABORT,'final write failed'); END;").unwrap();
                Ok(())
            },
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("finalisation failed"));
        assert!(capture(&path, 1, false).is_err());
        let t = target(&f);
        assert!(t.label.contains("accepted"));
        assert!(review(
            &mut f.db,
            &t.key,
            &t.evidence,
            "confirmed_not_sent",
            "guess",
            "test"
        )
        .is_err());
        f.db.execute_batch("DROP TRIGGER final_fail").unwrap();
        review(
            &mut f.db,
            &t.key,
            &t.evidence,
            "accepted",
            "Retained SMTP acceptance",
            "test",
        )
        .unwrap();
        assert!(!history(&f.db).unwrap()[0].unresolved);
    }
    #[test]
    fn audited_still_uncertain_not_sent_fresh_retry_and_repeated_resolution() {
        let mut f = Fixture::new();
        let a = capture(&f.path(), 1, false).unwrap();
        dispatch(
            &mut f,
            1,
            &a,
            Err(ProductionSendError::Uncertain("timeout".into())),
        )
        .unwrap_err();
        let t = target(&f);
        assert!(review(&mut f.db, &t.key, &t.evidence, "accepted", "", "test").is_err());
        assert!(review(&mut f.db, &t.key, &t.evidence, "accepted", "logs", " ").is_err());
        review(
            &mut f.db,
            &t.key,
            &t.evidence,
            "uncertain",
            "Logs inconclusive",
            "test",
        )
        .unwrap();
        assert!(capture(&f.path(), 1, false).is_err());
        assert!(review(
            &mut f.db,
            &t.key,
            &t.evidence,
            "confirmed_not_sent",
            "stale review",
            "test"
        )
        .is_err());
        let fresh = target(&f);
        review(
            &mut f.db,
            &fresh.key,
            &fresh.evidence,
            "confirmed_not_sent",
            "Server audit confirms DATA rejected",
            "test",
        )
        .unwrap();
        assert!(review(
            &mut f.db,
            &fresh.key,
            &fresh.evidence,
            "accepted",
            "Repeated decision",
            "test"
        )
        .is_err());
        assert_eq!(
            f.db.query_row("SELECT outcome FROM payroll_bundle_events", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "uncertain"
        );
        assert_eq!(
            f.db.query_row("SELECT count(*) FROM payroll_delivery_reviews", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            2
        );
        assert!(f
            .db
            .execute("UPDATE payroll_delivery_reviews SET reason='erase'", [])
            .is_err());
        let a = capture(&f.path(), 1, false).unwrap();
        dispatch(&mut f, 1, &a, Ok(())).unwrap();
    }
    #[test]
    fn legacy_uncertainty_retains_unknown_recipients_and_original_marker_without_inventing_attempts(
    ) {
        let mut f = Fixture::new();
        f.db.execute_batch("UPDATE imported_payroll_documents SET sent_at='indeterminate:legacy' WHERE personal_assistant_id=1; INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) VALUES(3,'2025/26',1,'payslip','indeterminate:old');").unwrap();
        let targets = history(&f.db).unwrap();
        assert_eq!(targets.len(), 3);
        for t in targets {
            assert!(t.evidence.contains("unknown"));
            review(
                &mut f.db,
                &t.key,
                &t.evidence,
                "uncertain",
                "No historic logs",
                "test",
            )
            .unwrap();
            let t = history(&f.db)
                .unwrap()
                .into_iter()
                .find(|n| n.key == t.key)
                .unwrap();
            review(
                &mut f.db,
                &t.key,
                &t.evidence,
                "confirmed_not_sent",
                "External audit confirms no send",
                "test",
            )
            .unwrap();
        }
        assert_eq!(
            f.db.query_row("SELECT count(*) FROM payroll_bundle_attempts", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            f.db.query_row("SELECT count(*) FROM payroll_delivery_reviews", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            6
        );
        assert!(history(&f.db).unwrap().is_empty());
    }
    #[test]
    fn active_transport_blocks_reconciliation_and_other_instance_claims() {
        let mut f = Fixture::new();
        let a = capture(&f.path(), 1, false).unwrap();
        let path = f.path();
        super::dispatch(
            &mut f.db,
            1,
            None,
            &a,
            |_| capture(&path, 1, false),
            |_, _, _| {
                let mut second = crate::database::open(&path).unwrap();
                let t = history(&second)
                    .unwrap()
                    .into_iter()
                    .find(|t| t.unresolved)
                    .unwrap();
                assert!(review(
                    &mut second,
                    &t.key,
                    &t.evidence,
                    "confirmed_not_sent",
                    "Concurrent review",
                    "test"
                )
                .is_err());
                assert!(super::dispatch(
                    &mut second,
                    1,
                    None,
                    &a,
                    |_| capture(&path, 1, false),
                    |_, _, _| panic!("concurrent SMTP")
                )
                .is_err());
                Err(ProductionSendError::Uncertain("timeout".into()))
            },
        )
        .unwrap_err();
        assert_eq!(
            f.db.query_row("SELECT count(*) FROM payroll_bundle_attempts", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn lock_failure_prevents_claim_and_transport() {
        let mut f = Fixture::new();
        let a = capture(&f.path(), 1, false).unwrap();
        std::fs::create_dir(f.path().with_extension("timesheet-delivery.lock")).unwrap();
        assert!(dispatch(&mut f, 1, &a, Ok(())).is_err());
        assert_eq!(
            f.db.query_row("SELECT count(*) FROM payroll_bundle_attempts", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn one_pa_uncertainty_does_not_block_other_pa_or_undo_partial_success() {
        let mut f = Fixture::new();
        let a = capture(&f.path(), 1, false).unwrap();
        dispatch(
            &mut f,
            1,
            &a,
            Err(ProductionSendError::Uncertain("error".into())),
        )
        .unwrap_err();
        let second = capture(&f.path(), 2, false).unwrap();
        dispatch(&mut f, 2, &second, Ok(())).unwrap();
        assert!(capture(&f.path(), 1, false).is_err());
        let targets = history(&f.db).unwrap();
        assert_eq!(targets.iter().filter(|t| t.unresolved).count(), 1);
        assert_eq!(targets.iter().filter(|t| !t.unresolved).count(), 1);
    }
    #[test]
    fn mixed_ordinary_p45_p60_bundle_reviews_atomically_and_retains_historical_settlement() {
        let mut f = Fixture::new();
        std::fs::write(
            f.dir.path().join("ordinary.pdf"),
            b"%PDF-1.4 ordinary payslip",
        )
        .unwrap();
        f.db.execute_batch("INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) VALUES(1,'2025/26',13,'payslip','historical-settlement');").unwrap();
        let a = capture(&f.path(), 1, true).unwrap();
        let path = f.path();
        assert_eq!(a.bytes.len(), 3);
        super::dispatch(
            &mut f.db,
            1,
            Some(PayslipDeliveryIdentity {
                personal_assistant_id: 1,
                payroll_year: "2026/27",
                cycle_number: 1,
            }),
            &a,
            |_| capture(&path, 1, true),
            |_, _, _| Err(ProductionSendError::Uncertain("DATA response lost".into())),
        )
        .unwrap_err();
        assert!(capture(&path, 1, true).is_err());
        let t = target(&f);
        review(
            &mut f.db,
            &t.key,
            &t.evidence,
            "accepted",
            "Server acceptance confirmed externally",
            "test",
        )
        .unwrap();
        assert_eq!(
            f.db.query_row(
                "SELECT sent_at FROM payroll_timesheet_email_status WHERE payroll_year='2025/26'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "historical-settlement"
        );
        assert!(f.db.query_row("SELECT sent_at NOT LIKE 'indeterminate:%' FROM payroll_timesheet_email_status WHERE payroll_year='2026/27'",[],|r|r.get::<_,bool>(0)).unwrap());
        assert_eq!(f.db.query_row("SELECT count(*) FROM imported_payroll_documents WHERE personal_assistant_id=1 AND sent_at IS NOT NULL AND sent_at NOT LIKE 'indeterminate:%'",[],|r|r.get::<_,i64>(0)).unwrap(),2);
        assert_eq!(
            std::fs::read(f.dir.path().join("ordinary.pdf")).unwrap(),
            a.bytes[0]
        );
    }
    #[test]
    fn accepted_transport_evidence_write_failure_leaves_claim_protected() {
        let mut f = Fixture::new();
        let a = capture(&f.path(), 1, false).unwrap();
        let path = f.path();
        super::dispatch(&mut f.db,1,None,&a,|_|capture(&path,1,false),|_,_,_| {
            crate::database::open(&path).unwrap().execute_batch("CREATE TRIGGER event_fail BEFORE INSERT ON payroll_bundle_events BEGIN SELECT RAISE(ABORT,'evidence write failed'); END;").unwrap();
            Ok(())
        }).unwrap_err();
        assert!(capture(&path, 1, false).is_err());
        let t = target(&f);
        assert!(t.label.contains("uncertain"));
        assert_eq!(
            f.db.query_row("SELECT count(*) FROM payroll_bundle_events", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        f.db.execute_batch("DROP TRIGGER event_fail").unwrap();
        review(
            &mut f.db,
            &t.key,
            &t.evidence,
            "uncertain",
            "Need external SMTP evidence",
            "test",
        )
        .unwrap();
    }
    #[test]
    fn previously_sent_supplement_and_its_bytes_survive_not_sent_resolution_and_retry() {
        let mut f = Fixture::new();
        f.db.execute("UPDATE imported_payroll_documents SET sent_at='previously-accepted' WHERE personal_assistant_id=1 AND document_type='p45'",[]).unwrap();
        let historical = std::fs::read(f.dir.path().join("1-p45.pdf")).unwrap();
        let a = capture(&f.path(), 1, false).unwrap();
        assert_eq!(a.bytes.len(), 1);
        dispatch(
            &mut f,
            1,
            &a,
            Err(ProductionSendError::Uncertain("timeout".into())),
        )
        .unwrap_err();
        let t = target(&f);
        review(
            &mut f.db,
            &t.key,
            &t.evidence,
            "confirmed_not_sent",
            "SMTP audit confirms no acceptance",
            "test",
        )
        .unwrap();
        let fresh = capture(&f.path(), 1, false).unwrap();
        dispatch(&mut f, 1, &fresh, Ok(())).unwrap();
        assert_eq!(f.db.query_row("SELECT sent_at FROM imported_payroll_documents WHERE personal_assistant_id=1 AND document_type='p45'",[],|r|r.get::<_,String>(0)).unwrap(),"previously-accepted");
        assert_eq!(
            std::fs::read(f.dir.path().join("1-p45.pdf")).unwrap(),
            historical
        );
    }

    #[test]
    fn legacy_retired_revision_review_preserves_later_settlement_and_other_pa_eligibility() {
        let mut f = Fixture::new();
        let path = f.dir.path().join("old.pdf");
        std::fs::write(&path, b"%PDF-1.4 retained historical").unwrap();
        f.db.execute("INSERT INTO payslip_revisions(id,personal_assistant_id,payroll_year,cycle_number,stored_path,sha256,sent_at,is_current) VALUES(1,1,'2025/26',13,?1,?2,'indeterminate:old-revision',0)",params![path.to_string_lossy(),digest(&std::fs::read(&path).unwrap())]).unwrap();
        f.db.execute_batch("INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) VALUES(1,'2025/26',13,'payslip','later-settlement');").unwrap();
        assert!(crate::database_recovery::unresolved(&f.db).unwrap());
        assert!(crate::payroll_archive_service::ensure_pa_not_uncertain(&f.db, 1).is_err());
        assert!(capture(&f.path(), 2, false).is_ok());
        let t = target(&f);
        assert_eq!(t.key, "revision:1");
        review(
            &mut f.db,
            &t.key,
            &t.evidence,
            "confirmed_not_sent",
            "Old revision never accepted; later revision separately settled",
            "test",
        )
        .unwrap();
        assert_eq!(
            f.db.query_row(
                "SELECT sent_at FROM payroll_timesheet_email_status",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "later-settlement"
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"%PDF-1.4 retained historical"
        );
        assert!(!crate::database_recovery::unresolved(&f.db).unwrap());
        assert_eq!(
            f.db.query_row("SELECT count(*) FROM payroll_bundle_attempts", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn orphan_current_revision_uncertainty_blocks_retry_and_accepted_review_restores_known_status()
    {
        let mut f = Fixture::new();
        let path = f.dir.path().join("ordinary.pdf");
        std::fs::write(&path, b"%PDF-1.4 current retained").unwrap();
        f.db.execute("INSERT INTO payslip_revisions(id,personal_assistant_id,payroll_year,cycle_number,stored_path,sha256,sent_at,is_current) VALUES(1,1,'2026/27',1,?1,?2,'indeterminate:orphan',1)",params![path.to_string_lossy(),digest(&std::fs::read(&path).unwrap())]).unwrap();
        assert!(capture(&f.path(), 1, true).is_err());
        let t = target(&f);
        review(
            &mut f.db,
            &t.key,
            &t.evidence,
            "accepted",
            "External audit confirms current revision accepted",
            "test",
        )
        .unwrap();
        let a = capture(&f.path(), 1, true).unwrap();
        assert!(!a.bundle.email_types.contains(&"payslip"));
        assert!(f
            .db
            .query_row(
                "SELECT sent_at NOT LIKE 'indeterminate:%' FROM payroll_timesheet_email_status",
                [],
                |r| r.get::<_, bool>(0)
            )
            .unwrap());
        assert_eq!(
            f.db.query_row("SELECT count(*) FROM payroll_bundle_attempts", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn startup_requires_stage8_retention_guards_and_old_writers_fail_closed() {
        let f = Fixture::new();
        f.db.create_scalar_function(
            "dpt_schema_version",
            0,
            rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
            |_| Ok(40_i64),
        )
        .unwrap();
        assert!(f
            .db
            .execute("UPDATE personal_assistants SET surname='old writer'", [])
            .is_err());
        crate::database::register_connection(&f.db).unwrap();
        f.db.execute_batch("DROP TRIGGER stage8_payroll_bundle_attachments_UPDATE")
            .unwrap();
        let before = crate::database_recovery::fingerprint(&f.db).unwrap();
        assert!(crate::database::initialise_database(&f.path()).is_err());
        assert_eq!(
            crate::database_recovery::fingerprint(&f.db).unwrap(),
            before
        );
    }

    #[test]
    fn schema40_migration_preserves_legacy_markers_settlement_and_is_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.sqlite");
        let db = crate::database::open(&path).unwrap();
        crate::database::create_legacy_schema(&db, 40).unwrap();
        db.execute_batch("INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) VALUES(1,'2025/26',1,'payslip','accepted-historical'),(2,'2025/26',1,'payslip','indeterminate:legacy');").unwrap();
        db.execute_batch("CREATE TRIGGER migration_fail BEFORE UPDATE ON schema_version WHEN NEW.version=41 BEGIN SELECT RAISE(ABORT,'test migration failure'); END;").unwrap();
        assert!(crate::database::create_schema(&db).is_err());
        assert!(!db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='payroll_bundle_attempts')",
                [],
                |r| r.get::<_, bool>(0)
            )
            .unwrap());
        db.execute_batch("DROP TRIGGER migration_fail").unwrap();
        drop(db);
        crate::database::initialise_database(&path).unwrap();
        let db = crate::database::open(&path).unwrap();
        assert_eq!(
            db.query_row("SELECT version FROM schema_version", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            41
        );
        assert_eq!(
            db.query_row(
                "SELECT sent_at FROM payroll_timesheet_email_status WHERE personal_assistant_id=1",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "accepted-historical"
        );
        assert_eq!(history(&db).unwrap().len(), 1);
        assert_eq!(
            db.query_row("SELECT count(*) FROM payroll_bundle_attempts", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
