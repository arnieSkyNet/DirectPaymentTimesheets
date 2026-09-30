//! Explicit correction: immutable old bytes and evidence, a new current attachment.
use crate::{
    app::Application, payroll_document_repository::file_digest, payroll_file_naming as naming,
};
use rusqlite::{params, OptionalExtension};
use std::{error::Error, fs, path::PathBuf};
type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Clone, Debug)]
pub struct Target {
    pub pa: i64,
    pub kind: String,
    pub year: Option<String>,
    pub cycle: Option<i64>,
    pub document_id: Option<i64>,
    pub path: PathBuf,
    pub digest: String,
    pub sent_at: Option<String>,
    pub history: String,
    pub label: String,
}

pub fn current_payslip(
    db: &rusqlite::Connection,
    pa: i64,
    year: &str,
    cycle: i64,
) -> Result<Option<(PathBuf, String)>> {
    Ok(db.query_row("SELECT stored_path,sha256 FROM payslip_revisions WHERE personal_assistant_id=?1 AND payroll_year=?2 AND cycle_number=?3 AND is_current=1", params![pa,year,cycle], |r| Ok((PathBuf::from(r.get::<_,String>(0)?), r.get(1)?))).optional()?)
}

pub fn targets(app: &Application, pa: i64) -> Result<Vec<Target>> {
    let db = &app.payroll_timesheet_email_repository.connection;
    let assistant = app
        .personal_assistant_repository
        .get_all()?
        .into_iter()
        .find(|p| p.id == pa)
        .ok_or("PA no longer exists")?;
    let mut result = Vec::new();
    for doc in app
        .payroll_timesheet_email_repository
        .documents_for_pa(pa)?
    {
        let sent_at = db.query_row(
            "SELECT sent_at FROM imported_payroll_documents WHERE id=?1",
            [doc.id],
            |r| r.get(0),
        )?;
        result.push(Target {
            pa,
            label: format!(
                "{} — {} — document {}",
                doc.document_type.to_uppercase(),
                doc.document_year.as_deref().unwrap_or("year unknown"),
                doc.id
            ),
            kind: doc.document_type,
            year: doc.document_year,
            cycle: None,
            document_id: Some(doc.id),
            path: doc.path,
            digest: doc.sha256,
            sent_at,
            history: doc.history_state,
        });
    }
    let root = crate::paths::expand_path(&app.context.config.folders.payslip_folder);
    for schedule in app.payroll_schedule_repository.get_all()? {
        let current = current_payslip(db, pa, &schedule.payroll_year, schedule.cycle_number)?;
        let path = match &current {
            Some((path, _)) => path.clone(),
            None => naming::existing_payslip_path(
                &root,
                &format!("{} {}", assistant.first_name, assistant.surname),
                &schedule,
            )?,
        };
        if !path.is_file() {
            continue;
        }
        let digest = match current {
            Some((_, digest)) => digest,
            None => file_digest(&path)?,
        };
        let sent_at = db.query_row("SELECT sent_at FROM payroll_timesheet_email_status WHERE personal_assistant_id=?1 AND payroll_year=?2 AND cycle_number=?3 AND email_type='payslip'",params![pa,schedule.payroll_year,schedule.cycle_number],|r|r.get::<_,Option<String>>(0)).optional()?.flatten();
        result.push(Target {
            pa,
            label: format!(
                "Payslip  {}  Week {}",
                schedule.payroll_year,
                naming::paye_week(&schedule)?
            ),
            kind: "payslip".into(),
            year: Some(schedule.payroll_year),
            cycle: Some(schedule.cycle_number),
            document_id: None,
            path,
            digest,
            sent_at,
            history: "cycle delivery evidence".into(),
        });
    }
    Ok(result)
}

#[derive(Clone)]
pub struct Review {
    pub target: Target,
    pub incoming: PathBuf,
    pub digest: String,
}

pub fn review(app: &Application, target: Target, incoming: PathBuf) -> Result<Review> {
    crate::archive::validate_payslip_pdf(&incoming)?;
    crate::payroll_archive_service::checked_path(&incoming)?;
    let name = incoming
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Invalid incoming filename")?;
    let (kind, pa) =
        crate::archive::classify_filename(name, &app.personal_assistant_repository.get_all()?)?;
    let kind = match kind {
        crate::archive::PlannedKind::Payslip => "payslip",
        crate::archive::PlannedKind::P45 => "p45",
        crate::archive::PlannedKind::P60 => "p60",
        _ => return Err("Choose an individual PA payroll PDF".into()),
    };
    if pa != Some(target.pa) || kind != target.kind {
        return Err(
            "Incoming PDF filename does not identify the selected PA and document type".into(),
        );
    }
    if naming::document_year(name).is_some_and(|y| Some(y) != target.year) {
        return Err("Incoming document year conflicts with selected document".into());
    }
    if let Some(cycle) = target.cycle {
        let schedule = app
            .payroll_schedule_repository
            .get_all()?
            .into_iter()
            .find(|s| Some(&s.payroll_year) == target.year.as_ref() && s.cycle_number == cycle)
            .ok_or("Payroll period no longer exists")?;
        if naming::provider_payslip_week(name)
            .is_some_and(|w| Some(w) != naming::paye_week(&schedule).ok())
        {
            return Err("Incoming payslip week conflicts with selected period".into());
        }
    }
    let digest = file_digest(&incoming)?;
    if digest == target.digest {
        return Err("These bytes are identical; no replacement is needed".into());
    }
    Ok(Review {
        target,
        incoming,
        digest,
    })
}

pub fn replace(app: &Application, reviewed: &Review) -> Result<PathBuf> {
    let db = &app.payroll_timesheet_email_repository.connection;
    let tx = rusqlite::Transaction::new_unchecked(db, rusqlite::TransactionBehavior::Immediate)?;
    let old = &reviewed.target;
    let now = targets(app, old.pa)?
        .into_iter()
        .find(|t| t.document_id == old.document_id && t.cycle == old.cycle && t.year == old.year)
        .ok_or("Document is no longer current")?;
    if now.path != old.path
        || now.digest != old.digest
        || now.sent_at != old.sent_at
        || now.history != old.history
    {
        return Err("Document or delivery history changed since review; review again".into());
    }
    let rechecked = review(app, now, reviewed.incoming.clone())?;
    if rechecked.digest != reviewed.digest {
        return Err("Incoming PDF changed since review".into());
    }
    crate::payroll_archive_service::verified(&old.path, &old.digest)?;
    let assistant = app
        .personal_assistant_repository
        .get_all()?
        .into_iter()
        .find(|p| p.id == old.pa)
        .ok_or("PA missing")?;
    let root = crate::payroll_archive_service::filing_base(&crate::paths::expand_path(
        &app.context.config.folders.payslip_folder,
    ))?;
    let year_dir = match &old.year {
        Some(y) => root.join(naming::payroll_year_directory_name(y)?),
        None => root.clone(),
    };
    let parent = crate::payroll_archive_service::normalised_path(
        old.path.parent().ok_or("Missing parent")?,
    )?;
    if parent != year_dir
        && parent != year_dir.join("Archived")
        && parent != year_dir.join(format!("PA {}", old.pa))
    {
        return Err("Existing document is outside its managed payroll area".into());
    }
    let directory = if naming::is_inactive(&assistant) || parent == year_dir.join("Archived") {
        year_dir.join("Archived")
    } else {
        year_dir
    };
    crate::payroll_archive_service::checked_path(&directory)?;
    fs::create_dir_all(&directory)?;
    // Hash-derived immutable destination is safe to reuse after a pre-commit crash.
    let name = naming::sanitise_filename(&format!(
        "{} {}",
        assistant.first_name.trim(),
        assistant.surname.trim()
    ));
    let heading = if let Some(cycle) = old.cycle {
        let schedule = app
            .payroll_schedule_repository
            .get_all()?
            .into_iter()
            .find(|s| Some(&s.payroll_year) == old.year.as_ref() && s.cycle_number == cycle)
            .ok_or("Missing schedule")?;
        format!("Payslip for Week {}", naming::paye_week(&schedule)?)
    } else {
        format!(
            "{}{}",
            old.kind.to_uppercase(),
            old.year
                .as_ref()
                .map(|y| format!(" for year {}", y.replace('/', "-")))
                .unwrap_or_default()
        )
    };
    let version: i64 = if old.document_id.is_some() {
        tx.query_row(
            "SELECT COALESCE(MAX(id),0)+1 FROM imported_payroll_documents",
            [],
            |r| r.get(0),
        )?
    } else {
        tx.query_row(
            "SELECT COALESCE(MAX(id),0)+1 FROM payslip_revisions",
            [],
            |r| r.get(0),
        )?
    };
    let destination = directory.join(format!(
        "{heading} - revision {} r{version} for {name}.pdf",
        reviewed.digest
    ));
    crate::payroll_archive_service::checked_path(&destination)?;
    for entry in fs::read_dir(&directory)? {
        let path = entry?.path();
        if path != destination
            && path.to_string_lossy().to_lowercase() == destination.to_string_lossy().to_lowercase()
        {
            return Err("Case-insensitive destination collision".into());
        }
    }
    let mut published = false;
    let outcome = (|| -> Result<()> {
        if destination.exists() {
            crate::payroll_archive_service::verified(&destination, &reviewed.digest)?;
        } else {
            let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
            std::io::copy(&mut fs::File::open(&reviewed.incoming)?, &mut temporary)?;
            temporary.as_file().sync_all()?;
            crate::payroll_archive_service::verified(temporary.path(), &reviewed.digest)?;
            fs::hard_link(temporary.path(), &destination)?;
            published = true;
        }
        crate::payroll_archive_service::sync_directory(&directory)?;
        let path = destination.to_str().ok_or("Non UTF-8 destination")?;
        if let Some(id) = old.document_id {
            // New bytes have unknown history, regardless of the superseded version's evidence.
            tx.execute("INSERT INTO imported_payroll_documents(personal_assistant_id,document_type,stored_path,sha256,document_year,history_state) VALUES(?1,?2,?3,?4,?5,'unknown')",params![old.pa,old.kind,path,reviewed.digest,old.year])?;
            let new_id = tx.last_insert_rowid();
            if tx.execute("UPDATE imported_payroll_documents SET superseded_by=?1 WHERE id=?2 AND superseded_by IS NULL",params![new_id,id])?!=1 {return Err("Document changed".into());}
        } else {
            if current_payslip(db, old.pa, old.year.as_deref().unwrap(), old.cycle.unwrap())?
                .is_none()
            {
                tx.execute("INSERT INTO payslip_revisions(personal_assistant_id,payroll_year,cycle_number,stored_path,sha256,sent_at,is_current) VALUES(?1,?2,?3,?4,?5,?6,1)",params![old.pa,old.year,old.cycle,old.path.to_str(),old.digest,old.sent_at])?;
            }
            tx.execute("UPDATE payslip_revisions SET is_current=0 WHERE personal_assistant_id=?1 AND payroll_year=?2 AND cycle_number=?3 AND is_current=1",params![old.pa,old.year,old.cycle])?;
            tx.execute("INSERT INTO payslip_revisions(personal_assistant_id,payroll_year,cycle_number,stored_path,sha256,is_current) VALUES(?1,?2,?3,?4,?5,1)",params![old.pa,old.year,old.cycle,path,reviewed.digest])?;
            tx.execute("INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) VALUES(?1,?2,?3,'payslip',NULL) ON CONFLICT(personal_assistant_id,payroll_year,cycle_number,email_type) DO UPDATE SET sent_at=NULL",params![old.pa,old.year,old.cycle])?;
            tx.execute("UPDATE payroll_schedules SET payslips_sent=0 WHERE payroll_year=?1 AND cycle_number=?2",params![old.year,old.cycle])?;
        }
        tx.commit()?;
        Ok(())
    })();
    if let Err(e) = outcome {
        if published {
            if let Err(cleanup) =
                crate::payroll_archive_service::verified(&destination, &reviewed.digest)
                    .and_then(|_| Ok(fs::remove_file(&destination)?))
            {
                return Err(format!(
                    "Replacement not committed: {e}; extra copy retained at {}: {cleanup}",
                    destination.display()
                )
                .into());
            }
        }
        return Err(e);
    }
    Ok(destination)
}

#[derive(Default)]
pub struct ReplacementUi {
    open: bool,
    choices: Vec<Target>,
    selected: Option<usize>,
    reviewed: Option<Review>,
    message: String,
}
impl ReplacementUi {
    pub fn open(&mut self, app: &Application, pa: i64) {
        self.open = true;
        self.selected = None;
        self.reviewed = None;
        match targets(app, pa) {
            Ok(choices) => {
                self.choices = choices;
                self.message = String::new();
            }
            Err(e) => {
                self.choices.clear();
                self.message = e.to_string();
            }
        }
    }
    fn target_selector(&mut self, ui: &mut eframe::egui::Ui) {
        ui.label("Select the existing document to replace:");
        if self.choices.is_empty() {
            ui.label("No replaceable documents found for this PA.");
        }
        eframe::egui::ScrollArea::vertical()
            .max_height(220.0)
            .show(ui, |ui| {
                for (index, target) in self.choices.iter().enumerate() {
                    if ui
                        .selectable_value(&mut self.selected, Some(index), &target.label)
                        .changed()
                    {
                        self.message.clear();
                    }
                }
            });
    }

    fn review_incoming(&mut self, app: &Application, path: PathBuf) -> Result<()> {
        let target = self
            .selected
            .and_then(|index| self.choices.get(index))
            .ok_or("Select the existing document before choosing its corrected PDF")?;
        self.reviewed = Some(review(app, target.clone(), path)?);
        self.message.clear();
        Ok(())
    }

    pub fn show(&mut self, ctx: &eframe::egui::Context, app: &Application) {
        if !self.open {
            return;
        }
        let mut close = false;
        eframe::egui::Window::new("Replace payroll document").collapsible(false).show(ctx,|ui| {
            ui.label("Choose the existing document, then an individual corrected PDF. Original bytes and delivery evidence will be retained. ZIP replacements must be extracted and reviewed individually.");
            if self.reviewed.is_none() {
                self.target_selector(ui);
                if let Some(target)=self.selected.and_then(|index|self.choices.get(index)) {
                    ui.label(format!("Selected target: {}",target.label));
                    ui.label(format!("Existing: {}",target.path.display()));
                    if ui.button("Choose corrected PDF…").clicked() {
                        if let Some(path)=rfd::FileDialog::new().add_filter("PDF", &["pdf"]).pick_file() {
                            if let Err(error)=self.review_incoming(app,path) {self.message=error.to_string();}
                        }
                    }
                } else {
                    ui.label("Select a target above to enable corrected PDF selection.");
                }
            }
            if let Some(review)=self.reviewed.clone() {
                ui.label(&review.target.label);
                ui.label(format!("PA ID: {} — old delivery history: {} — sent_at: {}",review.target.pa,review.target.history,review.target.sent_at.as_deref().unwrap_or("none")));
                ui.label(format!("Existing: {}\nSHA-256: {}",review.target.path.display(),review.target.digest));
                ui.label(format!("Incoming: {}\nSHA-256: {}",review.incoming.display(),review.digest));
                ui.label("Confirm that the PDF contents belong to this PA and selected payroll identity. Filename checks cannot verify PDF contents. Missing year/week metadata uses the identity you deliberately selected.");
                ui.label(if review.target.cycle.is_some() {"The corrected payslip becomes the current unsent attachment. Any old sent/uncertain evidence remains against the old revision. No email is sent."} else {"The corrected supplement becomes current with UNKNOWN delivery history and no sent timestamp. Review its delivery history separately before emailing. No email is sent."});
                if ui.button("Confirm replacement — retain old evidence").clicked() {
                    match replace(app,&review) {Ok(path)=>{self.message=format!("Replacement saved: {}. Original retained.",path.display());self.reviewed=None;self.choices.clear();},Err(e)=>self.message=format!("Replacement refused: {e}")}
                }
                if ui.button("Back to document selection").clicked() {self.reviewed=None;}
            }
            ui.label(&self.message);
            if ui.button("Close").clicked() {close=true;}
        });
        if close {
            self.open = false;
            self.reviewed = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payslip_delivery_service::{select_payroll_documents, PayslipDeliveryIdentity};
    fn fixture() -> (tempfile::TempDir, Application) {
        let (dir, app) = crate::payroll_timesheet_screen::tests::test_application();
        app.payroll_timesheet_email_repository.connection.execute_batch("INSERT INTO personal_assistants(id,first_name,surname,employment_status) VALUES(1,'Synthetic Middle','Example','Active'); INSERT INTO payroll_schedules(id,payroll_year,cycle_number,first_week_commencing,latest_posting_date,pay_date,created_at) VALUES(1,'2026/27',7,'07/09/2026','28/09/2026','02/10/2026','fixture');").unwrap();
        (dir, app)
    }
    fn pdf(path: &std::path::Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fn seed(app: &Application, kind: &str) -> Target {
        let path = app
            .context
            .config
            .folders
            .payslip_folder
            .join("2026 to 2027")
            .join(if kind == "payslip" {
                "Payslip for Week 26 for Synthetic Middle Example.pdf".into()
            } else {
                format!(
                    "{} for year 2026-27 for Synthetic Middle Example.pdf",
                    kind.to_uppercase()
                )
            });
        pdf(&path, b"%PDF-1.4 original");
        if kind != "payslip" {
            app.payroll_timesheet_email_repository
                .register_document(1, kind, &path, Some("2026/27"))
                .unwrap();
        }
        targets(app, 1)
            .unwrap()
            .into_iter()
            .find(|t| t.kind == kind)
            .unwrap()
    }
    fn incoming(dir: &std::path::Path, kind: &str) -> PathBuf {
        let path = dir.join(if kind == "payslip" {
            "Corrected Payslip for Week 26 for Synthetic Example.pdf".into()
        } else {
            format!(
                "{} corrected for year 2026-27 for Synthetic Example.pdf",
                kind.to_uppercase()
            )
        });
        pdf(&path, b"%PDF-1.4 corrected");
        path
    }
    #[test]
    fn corrected_payslip_is_current_unsent_and_old_delivery_evidence_survives() {
        for marker in [
            None,
            Some("sent-before"),
            Some("indeterminate:earlier-attempt"),
        ] {
            let (dir, app) = fixture();
            seed(&app, "payslip");
            let db = &app.payroll_timesheet_email_repository.connection;
            db.execute("INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) VALUES(1,'2026/27',7,'payslip',?1)",[marker]).unwrap();
            let old = targets(&app, 1).unwrap().remove(0);
            let reviewed = review(&app, old.clone(), incoming(dir.path(), "payslip")).unwrap();
            let new = replace(&app, &reviewed).unwrap();
            assert_eq!(fs::read(&old.path).unwrap(), b"%PDF-1.4 original");
            assert_eq!(fs::read(&new).unwrap(), b"%PDF-1.4 corrected");
            let history: Option<String> = db
                .query_row(
                    "SELECT sent_at FROM payslip_revisions WHERE is_current=0",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(history.as_deref(), marker);
            let bundle = select_payroll_documents(
                &app.payroll_timesheet_email_repository,
                1,
                Some((
                    PayslipDeliveryIdentity {
                        personal_assistant_id: 1,
                        payroll_year: "2026/27",
                        cycle_number: 7,
                    },
                    &old.path,
                )),
            )
            .unwrap();
            assert_eq!(bundle.paths, vec![new]);
            assert!(db
                .query_row::<Option<String>, _, _>(
                    "SELECT sent_at FROM payroll_timesheet_email_status",
                    [],
                    |r| r.get(0)
                )
                .unwrap()
                .is_none());
            assert!(replace(&app, &reviewed).is_err());
        }
    }
    #[test]
    fn supplement_replacement_preserves_unknown_and_all_old_history_and_p60() {
        for (history, marker) in [
            ("unknown", None),
            ("external", None),
            ("needs_sending", None),
            ("application", Some("sent")),
            ("needs_sending", Some("indeterminate:old")),
        ] {
            let (dir, app) = fixture();
            let old = seed(&app, "p45");
            let p60 = seed(&app, "p60");
            let db = &app.payroll_timesheet_email_repository.connection;
            db.execute(
                "UPDATE imported_payroll_documents SET history_state=?1,sent_at=?2 WHERE id=?3",
                params![history, marker, old.document_id],
            )
            .unwrap();
            let old = targets(&app, 1)
                .unwrap()
                .into_iter()
                .find(|t| t.kind == "p45")
                .unwrap();
            let new = replace(
                &app,
                &review(&app, old.clone(), incoming(dir.path(), "p45")).unwrap(),
            )
            .unwrap();
            let current = app
                .payroll_timesheet_email_repository
                .documents_for_pa(1)
                .unwrap();
            assert_eq!(current.len(), 2);
            let corrected = current.iter().find(|d| d.document_type == "p45").unwrap();
            assert_eq!(corrected.path, new);
            assert_eq!(corrected.history_state, "unknown");
            assert_eq!(
                corrected.delivery_state,
                crate::payroll_timesheet_email_repository::EmailDeliveryState::Unsent
            );
            let evidence:(String,Option<String>,i64)=db.query_row("SELECT history_state,sent_at,superseded_by FROM imported_payroll_documents WHERE id=?1",[old.document_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
            assert_eq!(
                evidence,
                (history.into(), marker.map(str::to_owned), corrected.id)
            );
            assert!(old.path.exists() && p60.path.exists());
            assert!(
                select_payroll_documents(&app.payroll_timesheet_email_repository, 1, None)
                    .unwrap()
                    .paths
                    .is_empty()
            );
        }
    }
    #[test]
    fn stale_source_identity_or_database_failure_cannot_replace() {
        let (dir, app) = fixture();
        let old = seed(&app, "p45");
        let source = incoming(dir.path(), "p45");
        let reviewed = review(&app, old.clone(), source.clone()).unwrap();
        pdf(&source, b"%PDF-1.4 changed after review");
        assert!(replace(&app, &reviewed).is_err());
        assert!(review(&app, old.clone(), incoming(dir.path(), "p60")).is_err());
        let source = incoming(dir.path(), "p45");
        let reviewed = review(&app, old.clone(), source).unwrap();
        app.payroll_timesheet_email_repository.connection.execute_batch("CREATE TRIGGER fail_replacement BEFORE UPDATE OF superseded_by ON imported_payroll_documents BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
        assert!(replace(&app, &reviewed).is_err());
        assert_eq!(targets(&app, 1).unwrap()[0].path, old.path);
        assert_eq!(fs::read_dir(old.path.parent().unwrap()).unwrap().count(), 1);
    }
    #[test]
    fn migration35_is_atomic_and_preserves_existing_history() {
        let (_dir, app) = fixture();
        seed(&app, "p45");
        let db = &app.payroll_timesheet_email_repository.connection;
        db.execute_batch("DROP TABLE payslip_revisions; ALTER TABLE imported_payroll_documents DROP COLUMN superseded_by; UPDATE imported_payroll_documents SET history_state='unknown'; UPDATE schema_version SET version=34; CREATE TRIGGER refuse35 BEFORE UPDATE ON schema_version BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
        assert!(crate::database::create_schema(db).is_err());
        assert!(db
            .prepare("SELECT superseded_by FROM imported_payroll_documents")
            .is_err());
        assert!(db.prepare("SELECT * FROM payslip_revisions").is_err());
        db.execute_batch("DROP TRIGGER refuse35").unwrap();
        crate::database::create_schema(db).unwrap();
        crate::database::create_schema(db).unwrap();
        assert_eq!(
            db.query_row::<String, _, _>(
                "SELECT history_state FROM imported_payroll_documents",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            "unknown"
        );
    }
    #[test]
    fn archive_all_revisions_timesheets_and_reactivate_without_restore() {
        let (dir, app) = fixture();
        let old = seed(&app, "payslip");
        let source = incoming(dir.path(), "payslip");
        replace(&app, &review(&app, old, source).unwrap()).unwrap();
        let old = seed(&app, "p45");
        replace(
            &app,
            &review(&app, old, incoming(dir.path(), "p45")).unwrap(),
        )
        .unwrap();
        seed(&app, "p60");
        let db = &app.payroll_timesheet_email_repository.connection;
        let record = app
            .payroll_timesheet_repository
            .insert("2026/27", 7, 1, None, "fixture")
            .unwrap();
        let schedule = app.payroll_schedule_repository.get_all().unwrap().remove(0);
        let sheet = naming::timesheet_path(
            &app.context.config.folders.pdf_output,
            "Synthetic Middle Example",
            &schedule,
        )
        .unwrap();
        pdf(&sheet, b"%PDF-1.4 current timesheet");
        let historic = app
            .context
            .config
            .folders
            .pdf_output
            .join("historical-evidence.pdf");
        pdf(&historic, b"%PDF-1.4 historical");
        db.execute("INSERT INTO payroll_timesheet_snapshot_states(payroll_timesheet_id,state,pdf_path,pdf_sha256,generated_at) VALUES(?1,'candidate',?2,?3,'fixture')",params![record,sheet.to_str(),file_digest(&sheet).unwrap()]).unwrap();
        db.execute("INSERT INTO payroll_submissions(payroll_timesheet_id,submitted_at,pdf_path,pdf_sha256,pdf_bytes) VALUES(?1,'previous',?2,?3,?4)",params![record,historic.to_str(),file_digest(&historic).unwrap(),fs::read(&historic).unwrap()]).unwrap();
        let shared = app
            .context
            .config
            .folders
            .payroll_information_folder
            .join("P30.pdf");
        pdf(&shared, b"%PDF-1.4 shared");
        let signature = dir.path().join("external-signature.png");
        fs::write(&signature, b"signature").unwrap();
        let mut pa = app
            .personal_assistant_repository
            .get_all()
            .unwrap()
            .remove(0);
        pa.signature = Some(signature.to_str().unwrap().into());
        pa.employment_status = Some("Inactive".into());
        crate::payroll_archive_service::apply(&app, &pa, true).unwrap();
        assert!(!sheet.exists() && !historic.exists());
        assert!(shared.exists() && signature.exists());
        let archived = naming::existing_timesheet_path(
            &app.context.config.folders.pdf_output,
            "Synthetic Middle Example",
            &schedule,
        )
        .unwrap();
        assert!(archived
            .parent()
            .unwrap()
            .ends_with("2026 to 2027/Archived"));
        crate::payroll_snapshot_service::verify_preview_or_test_attachment(
            &app.payroll_worked_item_repository,
            record,
            &archived,
        )
        .unwrap();
        let (path, bytes, at): (String, Vec<u8>, String) = db
            .query_row(
                "SELECT pdf_path,pdf_bytes,submitted_at FROM payroll_submissions",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert!(PathBuf::from(path).is_file());
        assert_eq!(bytes, b"%PDF-1.4 historical");
        assert_eq!(at, "previous");
        for target in app
            .payroll_timesheet_email_repository
            .all_documents_for_pa(1)
            .unwrap()
        {
            assert_eq!(
                target.path.parent().unwrap().file_name().unwrap(),
                "Archived"
            );
            assert!(target.path.exists());
        }
        let paths = db
            .prepare("SELECT stored_path FROM payslip_revisions")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().all(|p| PathBuf::from(p).is_file()));
        let late = app
            .context
            .config
            .folders
            .payslip_folder
            .join("2026 to 2027/Payslip for Week 22 for Synthetic Middle Example.pdf");
        pdf(&late, b"%PDF-1.4 late");
        crate::payroll_archive_service::apply(&app, &pa, true).unwrap();
        assert!(late.exists());
        pa.employment_status = Some("Active".into());
        crate::payroll_archive_service::apply(&app, &pa, true).unwrap();
        assert!(archived.exists());
        assert!(!sheet.exists());
    }
    #[test]
    fn inactive_import_replacement_and_missing_year_cannot_duplicate_current_supplement() {
        let (dir, app) = fixture();
        let old = seed(&app, "p45");
        let yearless = dir.path().join("P45 correction for Synthetic Example.pdf");
        pdf(&yearless, b"%PDF-1.4 different");
        let imported = app.import_payroll_documents(&yearless, None).unwrap();
        assert_eq!(imported.supplements_imported, 0);
        assert_eq!(imported.failures.len(), 1);
        let mut pa = app
            .personal_assistant_repository
            .get_all()
            .unwrap()
            .remove(0);
        pa.employment_status = Some("Inactive".into());
        crate::payroll_archive_service::apply(&app, &pa, true).unwrap();
        let target = targets(&app, 1)
            .unwrap()
            .into_iter()
            .find(|t| t.kind == "p45")
            .unwrap();
        assert!(!old.path.exists());
        let new = replace(&app, &review(&app, target, yearless).unwrap()).unwrap();
        assert_eq!(new.parent().unwrap().file_name().unwrap(), "Archived");
        let source = incoming(dir.path(), "p60");
        assert_eq!(
            app.import_payroll_documents(&source, None)
                .unwrap()
                .supplements_imported,
            1
        );
        assert!(app
            .payroll_timesheet_email_repository
            .documents_for_pa(1)
            .unwrap()
            .iter()
            .all(|d| d.path.parent().unwrap().file_name().unwrap() == "Archived"));
        assert_eq!(
            app.personal_assistant_repository.get_all().unwrap()[0]
                .employment_status
                .as_deref(),
            Some("Inactive")
        );
    }
    #[test]
    fn stale_selected_payslip_cannot_send_after_replacement_and_hash_change_is_refused() {
        let (dir, app) = fixture();
        let target = seed(&app, "payslip");
        let bundle = select_payroll_documents(
            &app.payroll_timesheet_email_repository,
            1,
            Some((
                PayslipDeliveryIdentity {
                    personal_assistant_id: 1,
                    payroll_year: "2026/27",
                    cycle_number: 7,
                },
                &target.path,
            )),
        )
        .unwrap();
        let corrected = replace(
            &app,
            &review(&app, target.clone(), incoming(dir.path(), "payslip")).unwrap(),
        )
        .unwrap();
        let mut invoked = false;
        assert!(
            crate::payslip_delivery_service::send_production_payslip_bundle(
                &app.payroll_timesheet_email_repository,
                PayslipDeliveryIdentity {
                    personal_assistant_id: 1,
                    payroll_year: "2026/27",
                    cycle_number: 7
                },
                &bundle,
                "fixture",
                || {
                    invoked = true;
                    Ok(())
                }
            )
            .is_err()
        );
        assert!(!invoked);
        pdf(&corrected, b"%PDF-1.4 tampered");
        assert!(select_payroll_documents(
            &app.payroll_timesheet_email_repository,
            1,
            Some((
                PayslipDeliveryIdentity {
                    personal_assistant_id: 1,
                    payroll_year: "2026/27",
                    cycle_number: 7
                },
                &target.path
            ))
        )
        .is_err());
    }
    #[test]
    fn timesheet_registry_failure_rolls_back_all_files_and_employment() {
        let (_dir, app) = fixture();
        seed(&app, "p45");
        let db = &app.payroll_timesheet_email_repository.connection;
        let record = app
            .payroll_timesheet_repository
            .insert("2026/27", 7, 1, None, "fixture")
            .unwrap();
        let sheet = app
            .context
            .config
            .folders
            .pdf_output
            .join("Timesheet - Synthetic Middle Example - 202609w26.pdf");
        pdf(&sheet, b"%PDF-1.4 sheet");
        db.execute("INSERT INTO payroll_timesheet_snapshot_states(payroll_timesheet_id,state,pdf_path,pdf_sha256,generated_at) VALUES(?1,'candidate',?2,?3,'fixture')",params![record,sheet.to_str(),file_digest(&sheet).unwrap()]).unwrap();
        db.execute_batch("CREATE TRIGGER fail_filing BEFORE UPDATE OF pdf_path ON payroll_timesheet_snapshot_states BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
        let mut pa = app
            .personal_assistant_repository
            .get_all()
            .unwrap()
            .remove(0);
        pa.employment_status = Some("Inactive".into());
        assert!(crate::payroll_archive_service::apply(&app, &pa, true).is_err());
        assert!(sheet.exists());
        assert_eq!(
            app.personal_assistant_repository.get_all().unwrap()[0]
                .employment_status
                .as_deref(),
            Some("Active")
        );
        assert_eq!(
            db.query_row::<String, _, _>(
                "SELECT pdf_path FROM payroll_timesheet_snapshot_states",
                [],
                |r| r.get(0)
            )
            .unwrap(),
            sheet.to_str().unwrap()
        );
        assert_eq!(
            db.query_row::<i64, _, _>("SELECT COUNT(*) FROM payroll_file_moves", [], |r| r.get(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn safe_unknown_year_and_unassociated_files_are_flat_archived_and_assets_excluded() {
        let (_dir, app) = fixture();
        let sheet = app
            .context
            .config
            .folders
            .pdf_output
            .join("Timesheet - Synthetic Middle Example - 202001w40.pdf");
        pdf(&sheet, b"%PDF-1.4 legacy timesheet");
        let ordinary = app
            .context
            .config
            .folders
            .payslip_folder
            .join("PA 1/Synthetic Example.pdf");
        pdf(&ordinary, b"%PDF-1.4 unassociated");
        let unrelated = app
            .context
            .config
            .folders
            .pdf_output
            .join("Other file for Synthetic Example.pdf");
        pdf(&unrelated, b"%PDF-1.4 unmanaged");
        let mut pa = app
            .personal_assistant_repository
            .get_all()
            .unwrap()
            .remove(0);
        pa.employment_status = Some("Inactive".into());
        crate::payroll_archive_service::apply(&app, &pa, true).unwrap();
        assert!(app
            .context
            .config
            .folders
            .pdf_output
            .join("Archived")
            .join(sheet.file_name().unwrap())
            .is_file());
        assert!(app
            .context
            .config
            .folders
            .payslip_folder
            .join("Archived/Unassociated - Synthetic Example.pdf")
            .is_file());
        assert!(!sheet.exists() && !ordinary.exists());
        assert!(unrelated.exists());
    }
    #[test]
    fn genuine_provider_revised_p45_and_week26_replacements_use_shared_matching() {
        // Exact reported filenames, with synthetic bytes and an isolated database.
        let (dir, app) = fixture();
        app.payroll_timesheet_email_repository.connection.execute_batch("UPDATE personal_assistants SET first_name='Fictional Middletest',surname='Samplepa' WHERE id=1;
            INSERT INTO personal_assistants(id,first_name,surname) VALUES(2,'Other','Person');").unwrap();
        let base = app
            .context
            .config
            .folders
            .payslip_folder
            .join("2026 to 2027");
        let p45 = base.join("P45 for year 2026-27 for Fictional Middletest Samplepa.pdf");
        let payslip = base.join("Payslip for Week 26 for Fictional Middletest Samplepa.pdf");
        pdf(&p45, b"%PDF-1.4 synthetic original P45");
        pdf(&payslip, b"%PDF-1.4 synthetic original payslip");
        app.payroll_timesheet_email_repository
            .register_document(1, "p45", &p45, Some("2026/27"))
            .unwrap();
        let choices = targets(&app, 1).unwrap();
        let p45_target = choices.iter().find(|t| t.kind == "p45").unwrap().clone();
        let payslip_target = choices
            .iter()
            .find(|t| t.kind == "payslip")
            .unwrap()
            .clone();
        for (name,target) in [
            ("Test Employer - Employee Leaving Statement (P60) for year 2026-27 for Fictional Samplepa REVISED.pdf", &p45_target),
            ("P45 for year 2026-27 for Other Person REVISED.pdf", &p45_target),
            ("P45 for year 2025-26 for Fictional Samplepa REVISED.pdf", &p45_target),
            ("P45 for Fictional Samplepa Jones REVISED.pdf", &p45_target),
            ("Test Employer - Employee Payslip for Week 22 for Fictional Samplepa.pdf", &payslip_target),
        ] {
            let source=dir.path().join(name);pdf(&source,b"%PDF-1.4 synthetic incorrect identity");
            assert!(review(&app,target.clone(),source).is_err(),"{name}");
        }
        let exact_p45=dir.path().join("Test Employer - Employee Leaving Statement (P45) for year 2026-27 for Fictional Samplepa REVISED.pdf");
        let exact_payslip = dir
            .path()
            .join("Test Employer - Employee Payslip for Week 26 for Fictional Samplepa.pdf");
        pdf(&exact_p45, b"%PDF-1.4 synthetic corrected P45");
        pdf(&exact_payslip, b"%PDF-1.4 synthetic corrected payslip");
        let p45_review = review(&app, p45_target.clone(), exact_p45.clone()).unwrap();
        let payslip_review = review(&app, payslip_target.clone(), exact_payslip.clone()).unwrap();
        app.payroll_timesheet_email_repository.connection.execute_batch("INSERT INTO personal_assistants(id,first_name,surname) VALUES(3,'Fictional Other','Samplepa');").unwrap();
        assert!(review(&app, p45_target, exact_p45).is_err());
        assert!(review(&app, payslip_target, exact_payslip).is_err());
        assert!(replace(&app, &p45_review).is_err());
        app.payroll_timesheet_email_repository
            .connection
            .execute("DELETE FROM personal_assistants WHERE id=3", [])
            .unwrap();
        let new_p45 = replace(&app, &p45_review).unwrap();
        let new_payslip = replace(&app, &payslip_review).unwrap();
        assert!(p45.exists() && payslip.exists() && new_p45.exists() && new_payslip.exists());
        let current = app
            .payroll_timesheet_email_repository
            .documents_for_pa(1)
            .unwrap();
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].history_state, "unknown");
        assert_eq!(
            current[0].delivery_state,
            crate::payroll_timesheet_email_repository::EmailDeliveryState::Unsent
        );
        let bundle = select_payroll_documents(
            &app.payroll_timesheet_email_repository,
            1,
            Some((
                PayslipDeliveryIdentity {
                    personal_assistant_id: 1,
                    payroll_year: "2026/27",
                    cycle_number: 7,
                },
                &payslip,
            )),
        )
        .unwrap();
        assert_eq!(bundle.paths, vec![new_payslip]);
        assert!(replace(&app, &p45_review).is_err());
    }
    #[test]
    fn selector_lists_and_selects_unsent_week26_after_p45_replacement() {
        use eframe::egui;
        fn labels(shape: &egui::Shape, out: &mut Vec<(String, egui::Pos2)>) {
            match shape {
                egui::Shape::Text(text) => out.push((text.galley.job.text.clone(), text.pos)),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        labels(shape, out);
                    }
                }
                _ => {}
            }
        }
        for archived in [false, true] {
            let (dir, app) = fixture();
            let db = &app.payroll_timesheet_email_repository.connection;
            db.execute_batch("UPDATE personal_assistants SET first_name='Fictional Middletest',surname='Samplepa' WHERE id=1;").unwrap();
            let mut base = app
                .context
                .config
                .folders
                .payslip_folder
                .join("2026 to 2027");
            if archived {
                base = base.join("Archived");
            }
            let payslip = base.join("Payslip for Week 26 for Fictional Middletest Samplepa.pdf");
            let p45 = base.join("P45 for year 2026-27 for Fictional Middletest Samplepa.pdf");
            pdf(&payslip, b"%PDF-1.4 synthetic old Week 26");
            pdf(&p45, b"%PDF-1.4 synthetic old P45");
            let id = app
                .payroll_timesheet_email_repository
                .register_document(1, "p45", &p45, Some("2026/27"))
                .unwrap();
            db.execute(
                "UPDATE imported_payroll_documents SET id=4 WHERE id=?1",
                [id],
            )
            .unwrap();
            db.execute_batch("INSERT INTO payroll_timesheet_email_status(personal_assistant_id,payroll_year,cycle_number,email_type,sent_at) VALUES(1,'2026/27',7,'payslip',NULL);").unwrap();
            let corrected_p45=dir.path().join("Test Employer - Employee Leaving Statement (P45) for year 2026-27 for Fictional Samplepa REVISED.pdf");
            pdf(&corrected_p45, b"%PDF-1.4 synthetic new P45");
            let target = targets(&app, 1)
                .unwrap()
                .into_iter()
                .find(|t| t.kind == "p45")
                .unwrap();
            replace(&app, &review(&app, target, corrected_p45).unwrap()).unwrap();
            let before_p45 = app
                .payroll_timesheet_email_repository
                .documents_for_pa(1)
                .unwrap()
                .remove(0);
            assert_eq!(before_p45.id, 5);
            let before_bundle = select_payroll_documents(
                &app.payroll_timesheet_email_repository,
                1,
                Some((
                    PayslipDeliveryIdentity {
                        personal_assistant_id: 1,
                        payroll_year: "2026/27",
                        cycle_number: 7,
                    },
                    &payslip,
                )),
            )
            .unwrap();
            assert_eq!(before_bundle.paths, vec![payslip.clone()]);

            let mut screen = ReplacementUi::default();
            screen.open(&app, 1);
            assert_eq!(screen.choices.len(), 2);
            assert_eq!(
                screen.selected, None,
                "Reopening must not silently select P45 document 5"
            );
            assert!(screen.choices.iter().any(|t| t.document_id == Some(5)));
            let index = screen
                .choices
                .iter()
                .position(|t| t.kind == "payslip")
                .unwrap();
            assert_eq!(screen.choices[index].label, "Payslip  2026/27  Week 26");
            assert_eq!(screen.choices[index].path, before_bundle.paths[0]);
            let corrected = dir
                .path()
                .join("Test Employer - Employee Payslip for Week 26 for Fictional Samplepa.pdf");
            pdf(&corrected, b"%PDF-1.4 synthetic corrected Week 26");
            assert!(screen.review_incoming(&app, corrected.clone()).is_err());
            let ctx = egui::Context::default();
            let mut frame = |events| {
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(640.0, 480.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| screen.target_selector(ui));
                    },
                );
                let mut rendered = Vec::new();
                for shape in output.shapes {
                    labels(&shape.shape, &mut rendered);
                }
                rendered
            };
            frame(vec![]);
            let rendered = frame(vec![]);
            assert!(rendered
                .iter()
                .any(|(text, _)| text.contains("P45") && text.contains("document 5")));
            let pos = rendered
                .iter()
                .find(|(text, _)| text == "Payslip  2026/27  Week 26")
                .unwrap()
                .1
                + egui::vec2(5.0, 5.0);
            frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            frame(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }]);
            assert_eq!(screen.selected, Some(index));
            screen.review_incoming(&app, corrected).unwrap();
            let reviewed = screen.reviewed.as_ref().unwrap();
            assert_eq!(reviewed.target.cycle, Some(7));
            assert_eq!(reviewed.target.year.as_deref(), Some("2026/27"));
            let new_path = replace(&app, reviewed).unwrap();
            assert_eq!(
                fs::read(&payslip).unwrap(),
                b"%PDF-1.4 synthetic old Week 26"
            );
            assert_eq!(
                fs::read(&new_path).unwrap(),
                b"%PDF-1.4 synthetic corrected Week 26"
            );
            let bundle = select_payroll_documents(
                &app.payroll_timesheet_email_repository,
                1,
                Some((
                    PayslipDeliveryIdentity {
                        personal_assistant_id: 1,
                        payroll_year: "2026/27",
                        cycle_number: 7,
                    },
                    &payslip,
                )),
            )
            .unwrap();
            assert_eq!(bundle.paths, vec![new_path]);
            assert!(db
                .query_row::<Option<String>, _, _>(
                    "SELECT sent_at FROM payroll_timesheet_email_status WHERE email_type='payslip'",
                    [],
                    |r| r.get(0)
                )
                .unwrap()
                .is_none());
            let after_p45 = app
                .payroll_timesheet_email_repository
                .documents_for_pa(1)
                .unwrap()
                .remove(0);
            assert_eq!(
                (
                    after_p45.id,
                    after_p45.path,
                    after_p45.sha256,
                    after_p45.history_state,
                    after_p45.delivery_state
                ),
                (
                    before_p45.id,
                    before_p45.path,
                    before_p45.sha256,
                    before_p45.history_state,
                    before_p45.delivery_state
                )
            );
        }
    }
}
