//! Clipboard entries are ordinary captured CSV files, never a second import engine.
use crate::{app::Application, import_service::ImportSummary, models::PersonalAssistant};
use std::{error::Error, io::Write, path::PathBuf};

pub(crate) struct Session {
    pub outstanding: Vec<PersonalAssistant>,
    pub accepting: bool,
    pub selected: i64,
    pub error: String,
    pub paths: Vec<PathBuf>,
    import_dir: PathBuf,
}
impl Session {
    pub fn new(app: &Application, summary: &ImportSummary) -> Result<Self, Box<dyn Error>> {
        let outstanding: Vec<_> = app
            .personal_assistant_repository
            .get_active()?
            .into_iter()
            .filter(|pa| !summary.imported_pa_ids.contains(&pa.id))
            .collect();
        Ok(Self {
            selected: outstanding.first().map_or(0, |pa| pa.id),
            outstanding,
            accepting: false,
            error: String::new(),
            paths: Vec::new(),
            import_dir: crate::paths::expand_path(&app.context.config.folders.csv_import),
        })
    }
    pub fn save_paste(&mut self, app: &Application, text: &str) -> Result<(), Box<dyn Error>> {
        if !self.accepting {
            return Err("Choose Yes before pasting CSV.".into());
        }
        if self.import_dir != crate::paths::expand_path(&app.context.config.folders.csv_import) {
            return Err("Import folder changed; cancel and start Import Hours CSV again.".into());
        }
        let pa = self
            .outstanding
            .iter()
            .find(|pa| pa.id == self.selected)
            .ok_or("Select an outstanding active Personal Assistant.")?;
        let active = app.personal_assistant_repository.get_active()?;
        let current = active
            .iter()
            .find(|p| p.id == pa.id)
            .ok_or("PA is no longer active; cancel and restart import.")?;
        let rows = crate::csv_import::import_csv_bytes(text.as_bytes())?;
        if rows.is_empty() {
            return Err("Clipboard CSV contains no shifts.".into());
        }
        for row in &rows {
            if app
                .repository
                .resolve_personal_assistant_ids(&row.entry.pa_name)?
                != [pa.id]
            {
                return Err(
                    "Every clipboard row must match the selected active PA unambiguously.".into(),
                );
            }
        }
        let first = rows.iter().map(|r| r.start.date()).min().unwrap();
        let last = rows.iter().map(|r| r.end.date()).max().unwrap();
        let name = crate::payroll_file_naming::sanitise_filename(&format!(
            "{} {}",
            current.first_name, current.surname
        ));
        let stem = format!(
            "{} to {} - {name} - Clipboard",
            first.format("%d-%m-%Y"),
            last.format("%d-%m-%Y")
        );
        let mut pending = crate::private_storage::tempfile_in(&self.import_dir)?;
        pending.write_all(text.as_bytes())?;
        pending.as_file().sync_all()?;
        for sequence in 0..1000 {
            let suffix = if sequence == 0 {
                String::new()
            } else {
                format!(" ({sequence})")
            };
            let path = self.import_dir.join(format!("{stem}{suffix}.csv"));
            match crate::private_storage::publish_tempfile(pending, &path, false) {
                Ok(_) => {
                    self.paths.push(path);
                    self.outstanding.retain(|p| p.id != self.selected);
                    self.selected = self.outstanding.first().map_or(0, |p| p.id);
                    self.error.clear();
                    return Ok(());
                }
                Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => pending = e.file,
                Err(e) => return Err(e.error.into()),
            }
        }
        Err("Could not allocate a nonconflicting clipboard CSV filename.".into())
    }
    pub fn finish(&self, app: &Application) -> Result<ImportSummary, Box<dyn Error>> {
        if self.import_dir != crate::paths::expand_path(&app.context.config.folders.csv_import) {
            return Err(
                "Import folder changed; saved clipboard CSVs remain in the original folder.".into(),
            );
        }
        app.create_import_service().run_paths(&self.paths)
    }
}

pub(crate) fn combine(mut first: ImportSummary, second: ImportSummary) -> ImportSummary {
    first.imported_pa_ids.extend(second.imported_pa_ids);
    first.affected_pa_dates.extend(second.affected_pa_dates);
    first.files_discovered += second.files_discovered;
    first.files_already_imported += second.files_already_imported;
    first.files_processed += second.files_processed;
    first.files_succeeded += second.files_succeeded;
    first.files_refused += second.files_refused;
    first.rows_processed += second.rows_processed;
    first.rows_imported += second.rows_imported;
    first.rows_skipped += second.rows_skipped;
    first.files_failed += second.files_failed;
    first.failure_messages.extend(second.failure_messages);
    first.orphaned_archives.extend(second.orphaned_archives);
    first.archived_paths.extend(second.archived_paths);
    first
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (crate::private_storage::PrivateTempDir, Application) {
        let (dir, app) = crate::payroll_timesheet_screen::tests::test_application();
        crate::private_storage::ensure_private_dir(&app.context.config.folders.csv_import).unwrap();
        app.repository.connection().execute_batch("INSERT INTO personal_assistants(id,first_name,surname,employment_status) VALUES
            (1,'First Middle','Example','Active'),(2,'Second','Example','Active'),(3,'Dormant','Example','Inactive');").unwrap();
        (dir, app)
    }
    fn csv(name: &str) -> String {
        format!("Fictional export\nPA,Start,End,Break,Worked,Rate,Amount,Note\n{name},27 July 2026 at 09:00:00,27 July 2026 at 10:00:00,0h 00m,1h 00m,12.00,12.00,synthetic\n")
    }
    fn folder(app: &Application, name: &str, pa: &str) {
        crate::private_storage::write_new(
            &app.context.config.folders.csv_import.join(name),
            csv(pa).as_bytes(),
        )
        .unwrap();
    }
    #[test]
    fn folder_only_covers_active_pas_and_already_imported_files() {
        let (_dir, app) = setup();
        folder(&app, "first.csv", "First Middle Example");
        folder(&app, "second.csv", "Second Example");
        let first = app.import_csv().unwrap();
        assert_eq!(first.files_succeeded, 2);
        assert!(Session::new(&app, &first).unwrap().outstanding.is_empty());
        let repeat = app.import_csv().unwrap();
        assert_eq!(repeat.files_already_imported, 2);
        assert!(Session::new(&app, &repeat).unwrap().outstanding.is_empty());
        assert_eq!(app.get_timesheets().unwrap().len(), 2);
    }
    #[test]
    fn clipboard_only_publishes_exact_bytes_full_name_and_dates_then_imports() {
        let (_dir, app) = setup();
        let first = app.import_csv().unwrap();
        let mut session = Session::new(&app, &first).unwrap();
        assert_eq!(session.outstanding.len(), 2);
        session.accepting = true;
        let text = csv("First Middle Example");
        session.save_paste(&app, &text).unwrap();
        assert_eq!(session.outstanding.len(), 1);
        assert_eq!(std::fs::read(&session.paths[0]).unwrap(), text.as_bytes());
        assert_eq!(
            session.paths[0].file_name().unwrap(),
            "27-07-2026 to 27-07-2026 - First Middle Example - Clipboard.csv"
        );
        assert!(
            app.get_timesheets().unwrap().is_empty(),
            "save precedes the existing importer"
        );
        session.save_paste(&app, &csv("Second Example")).unwrap();
        assert!(session.outstanding.is_empty());
        let summary = combine(first, session.finish(&app).unwrap());
        assert_eq!(summary.files_succeeded, 2);
        assert_eq!(summary.rows_imported, 2);
        assert_eq!(app.get_timesheets().unwrap().len(), 2);
    }
    #[test]
    fn mixed_import_keeps_folder_results_and_imports_only_new_clipboard_files() {
        let (_dir, app) = setup();
        folder(&app, "first.csv", "First Middle Example");
        let first = app.import_csv().unwrap();
        let mut session = Session::new(&app, &first).unwrap();
        assert_eq!(
            session.outstanding.iter().map(|p| p.id).collect::<Vec<_>>(),
            vec![2]
        );
        session.accepting = true;
        session.save_paste(&app, &csv("Second Example")).unwrap();
        let summary = combine(first, session.finish(&app).unwrap());
        assert_eq!(summary.files_discovered, 2);
        assert_eq!(summary.files_succeeded, 2);
        assert_eq!(summary.rows_imported, 2);
        assert_eq!(summary.files_already_imported, 0);
        assert_eq!(session.finish(&app).unwrap().files_already_imported, 1);
        assert_eq!(app.get_timesheets().unwrap().len(), 2);
    }
    #[test]
    fn inactive_pas_are_never_offered_and_status_is_rechecked_on_paste() {
        let (_dir, app) = setup();
        let mut session = Session::new(&app, &ImportSummary::default()).unwrap();
        assert!(!session.outstanding.iter().any(|p| p.id == 3));
        session.accepting = true;
        app.repository
            .connection()
            .execute(
                "UPDATE personal_assistants SET employment_status='Inactive' WHERE id=1",
                [],
            )
            .unwrap();
        assert!(session
            .save_paste(&app, &csv("First Middle Example"))
            .is_err());
        assert!(session.paths.is_empty());
    }
    #[test]
    fn declining_or_cancelling_retains_folder_work_and_imports_saved_pastes() {
        let (_dir, app) = setup();
        folder(&app, "first.csv", "First Middle Example");
        let first = app.import_csv().unwrap();
        let mut declined = Session::new(&app, &first).unwrap();
        assert!(declined.save_paste(&app, &csv("Second Example")).is_err());
        drop(declined);
        assert_eq!(app.get_timesheets().unwrap().len(), 1);
        let mut session = Session::new(&app, &ImportSummary::default()).unwrap();
        session.accepting = true;
        session
            .save_paste(&app, &csv("First Middle Example"))
            .unwrap();
        assert_eq!(session.outstanding.len(), 1);
        let cancelled = session.finish(&app).unwrap();
        assert_eq!(cancelled.rows_skipped, 1);
        assert_eq!(app.get_timesheets().unwrap().len(), 1);
    }
    #[test]
    fn malformed_empty_or_wrong_pa_clipboard_never_creates_a_file_or_advances() {
        let (_dir, app) = setup();
        let mut session = Session::new(&app, &ImportSummary::default()).unwrap();
        session.accepting = true;
        for text in [
            String::new(),
            "not CSV".into(),
            csv("Second Example"),
            csv("First Middle Example").replace("1h 00m", "broken"),
            "period\nPA,Start,End,Break,Worked,Rate,Amount,Note\n".into(),
        ] {
            assert!(session.save_paste(&app, &text).is_err());
            assert_eq!(session.outstanding.len(), 2);
            assert!(session.paths.is_empty());
        }
        assert_eq!(
            std::fs::read_dir(&app.context.config.folders.csv_import)
                .unwrap()
                .count(),
            0
        );
    }
    #[test]
    fn destination_conflicts_never_overwrite_and_save_failure_retains_pa() {
        let (_dir, app) = setup();
        let first = app.import_csv().unwrap();
        let mut session = Session::new(&app, &first).unwrap();
        session.accepting = true;
        let target = app
            .context
            .config
            .folders
            .csv_import
            .join("27-07-2026 to 27-07-2026 - First Middle Example - Clipboard.csv");
        crate::private_storage::write_new(&target, b"existing").unwrap();
        session
            .save_paste(&app, &csv("First Middle Example"))
            .unwrap();
        assert_eq!(std::fs::read(target).unwrap(), b"existing");
        assert!(session.paths[0]
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("Clipboard (1).csv"));
        std::fs::rename(
            &app.context.config.folders.csv_import,
            app.context.environment.data_dir.join("moved-import"),
        )
        .unwrap();
        assert!(session.save_paste(&app, &csv("Second Example")).is_err());
        assert_eq!(session.outstanding.len(), 1);
    }
    #[test]
    fn missing_saved_csv_or_changed_folder_is_reported_without_losing_folder_import() {
        let (_dir, mut app) = setup();
        folder(&app, "first.csv", "First Middle Example");
        let first = app.import_csv().unwrap();
        let mut session = Session::new(&app, &first).unwrap();
        session.accepting = true;
        session.save_paste(&app, &csv("Second Example")).unwrap();
        std::fs::remove_file(&session.paths[0]).unwrap();
        assert!(session
            .finish(&app)
            .err()
            .unwrap()
            .to_string()
            .contains("missing"));
        assert_eq!(app.get_timesheets().unwrap().len(), 1);
        app.context.config.folders.csv_import =
            app.context.environment.data_dir.join("different-import");
        assert!(session
            .finish(&app)
            .err()
            .unwrap()
            .to_string()
            .contains("changed"));
        assert!(session
            .save_paste(&app, &csv("Second Example"))
            .err()
            .unwrap()
            .to_string()
            .contains("changed"));
    }

    #[test]
    fn refused_folder_files_do_not_hide_outstanding_pas_or_disappear_from_summary() {
        let (_dir, app) = setup();
        folder(&app, "bad.csv", "Unknown Example");
        let first = app.import_csv().unwrap();
        assert_eq!(first.files_refused, 1);
        let mut session = Session::new(&app, &first).unwrap();
        assert_eq!(session.outstanding.len(), 2);
        session.accepting = true;
        session
            .save_paste(&app, &csv("First Middle Example"))
            .unwrap();
        let summary = combine(first, session.finish(&app).unwrap());
        assert_eq!(summary.files_refused, 1);
        assert_eq!(summary.files_succeeded, 1);
        assert_eq!(summary.failure_messages.len(), 1);
    }
}
