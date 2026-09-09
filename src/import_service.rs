use chrono::Local;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use crate::archive;
use crate::csv_import::{self, ParsedTimesheetRow};
use crate::models::TimesheetEntry;
use crate::repository::TimesheetRepository;

#[derive(Default)]
pub struct ImportSummary {
    pub affected_pa_dates: Vec<(i64, chrono::NaiveDate)>,
    pub files_discovered: i64,
    pub files_already_imported: i64,
    pub files_processed: i64,
    pub files_succeeded: i64,
    pub files_refused: i64,
    pub rows_processed: i64,
    pub rows_imported: i64,
    pub rows_skipped: i64,
    pub files_failed: i64,
    pub failure_messages: Vec<String>,
    pub orphaned_archives: Vec<PathBuf>,
    pub archived_paths: Vec<PathBuf>,
}

impl ImportSummary {
    pub fn has_failures(&self) -> bool {
        self.files_refused > 0 || self.files_failed > 0
    }
}

#[derive(Clone, Copy)]
enum FailureKind {
    Refused,
    Failed,
}

struct FileFailure {
    kind: FailureKind,
    message: String,
    rows_processed: i64,
    orphaned_archive: Option<PathBuf>,
}

struct FileImportResult {
    affected_pa_dates: Vec<(i64, chrono::NaiveDate)>,
    rows_processed: i64,
    rows_imported: i64,
    rows_skipped: i64,
    archive_path: PathBuf,
}

impl FileFailure {
    fn refused(error: impl std::fmt::Display, rows_processed: i64) -> Self {
        Self {
            kind: FailureKind::Refused,
            message: error.to_string(),
            rows_processed,
            orphaned_archive: None,
        }
    }

    fn failed(error: impl std::fmt::Display, rows_processed: i64) -> Self {
        Self {
            kind: FailureKind::Failed,
            message: error.to_string(),
            rows_processed,
            orphaned_archive: None,
        }
    }
}

pub struct ImportService<'a> {
    repository: &'a TimesheetRepository,
    archive_dir: &'a Path,
    import_dir: PathBuf,
}

impl<'a> ImportService<'a> {
    pub fn new(
        repository: &'a TimesheetRepository,
        import_dir: PathBuf,
        archive_dir: &'a Path,
    ) -> Self {
        Self {
            repository,
            archive_dir,
            import_dir,
        }
    }

    pub fn run(&self) -> Result<ImportSummary, Box<dyn Error>> {
        let mut summary = ImportSummary::default();
        let mut csv_files = Vec::new();
        for entry in fs::read_dir(&self.import_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("csv"))
            {
                csv_files.push(path);
            }
        }
        csv_files.sort();
        summary.files_discovered = csv_files.len() as i64;

        for path in csv_files {
            let filename = path.to_string_lossy().into_owned();
            match self.repository.has_successful_import(&filename) {
                Ok(true) => {
                    summary.files_already_imported += 1;
                    continue;
                }
                Ok(false) => {}
                Err(error) => {
                    summary.files_failed += 1;
                    summary.failure_messages.push(format!(
                        "{}: could not check import history: {error}",
                        path.display()
                    ));
                    continue;
                }
            }

            summary.files_processed += 1;
            match self.process_file(&path) {
                Ok(result) => {
                    summary.files_succeeded += 1;
                    summary.rows_processed += result.rows_processed;
                    summary.rows_imported += result.rows_imported;
                    summary.rows_skipped += result.rows_skipped;
                    summary.archived_paths.push(result.archive_path);
                    summary.affected_pa_dates.extend(result.affected_pa_dates);
                }
                Err(failure) => {
                    match failure.kind {
                        FailureKind::Refused => summary.files_refused += 1,
                        FailureKind::Failed => summary.files_failed += 1,
                    }
                    summary.rows_processed += failure.rows_processed;
                    if let Some(path) = &failure.orphaned_archive {
                        summary.orphaned_archives.push(path.clone());
                    }
                    let status = match failure.kind {
                        FailureKind::Refused => "REFUSED",
                        FailureKind::Failed => "FAILED",
                    };
                    let archive_filename = failure
                        .orphaned_archive
                        .as_deref()
                        .map(|path| path.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let audit_result = self.repository.add_import_audit(
                        &import_time(),
                        &filename,
                        &archive_filename,
                        failure.rows_processed,
                        0,
                        0,
                        status,
                        Some(&failure.message),
                    );
                    let mut message = format!("{}: {}", path.display(), failure.message);
                    if let Err(audit_error) = audit_result {
                        message
                            .push_str(&format!("; failed to record {status} audit: {audit_error}"));
                    }
                    summary.failure_messages.push(message);
                }
            }
        }

        Ok(summary)
    }

    fn process_file(&self, path: &Path) -> Result<FileImportResult, FileFailure> {
        let bytes = fs::read(path).map_err(|error| FileFailure::failed(error, 0))?;
        let mut rows =
            csv_import::import_csv_bytes(&bytes).map_err(|error| FileFailure::refused(error, 0))?;
        let row_count = rows.len() as i64;
        self.resolve_personal_assistants(&mut rows, row_count)?;
        let affected_pa_dates = rows
            .iter()
            .filter_map(|r| r.entry.personal_assistant_id.map(|id| (id, r.start.date())))
            .collect();
        let (entries, rows_skipped) = self.classify_collisions(rows, row_count)?;

        let archive_path = archive::archive_csv_bytes(path, &bytes, self.archive_dir)
            .map_err(|error| FileFailure::failed(format!("archive failed: {error}"), row_count))?;
        if let Err(error) = self.repository.import_file_atomically(
            &entries,
            &import_time(),
            path.to_string_lossy().as_ref(),
            archive_path.to_string_lossy().as_ref(),
            row_count,
            rows_skipped,
        ) {
            return Err(FileFailure {
                kind: FailureKind::Failed,
                message: format!(
                    "database import rolled back after archiving; recoverable orphan archive: {} ({error})",
                    archive_path.display()
                ),
                rows_processed: row_count,
                orphaned_archive: Some(archive_path),
            });
        }
        Ok(FileImportResult {
            affected_pa_dates,
            rows_processed: row_count,
            rows_imported: entries.len() as i64,
            rows_skipped,
            archive_path,
        })
    }

    fn resolve_personal_assistants(
        &self,
        rows: &mut [ParsedTimesheetRow],
        row_count: i64,
    ) -> Result<(), FileFailure> {
        for row in rows {
            let matches = self
                .repository
                .resolve_personal_assistant_ids(&row.entry.pa_name)
                .map_err(|error| FileFailure::failed(error, row_count))?;
            match matches.as_slice() {
                [id] => row.entry.personal_assistant_id = Some(*id),
                [] => {
                    return Err(FileFailure::refused(
                        format!(
                            "CSV row {}: PA {:?} does not match a maintained Personal Assistant",
                            row.source_row, row.entry.pa_name
                        ),
                        row_count,
                    ));
                }
                _ => {
                    return Err(FileFailure::refused(
                        format!(
                            "CSV row {}: PA {:?} is ambiguous and matches {} maintained Personal Assistants",
                            row.source_row,
                            row.entry.pa_name,
                            matches.len()
                        ),
                        row_count,
                    ));
                }
            }
        }
        Ok(())
    }

    fn classify_collisions(
        &self,
        rows: Vec<ParsedTimesheetRow>,
        row_count: i64,
    ) -> Result<(Vec<TimesheetEntry>, i64), FileFailure> {
        let existing_entries = self
            .repository
            .get_all_raw()
            .map_err(|error| FileFailure::failed(error, row_count))?;
        let mut entries = Vec::new();
        let mut rows_skipped = 0;
        for row in rows {
            // Repeated exports of an exactly retained row are idempotent. New
            // within-file candidates, including identical intervals, are kept.
            if existing_entries
                .iter()
                .any(|existing| existing_row_is_materially_identical(existing, &row))
            {
                rows_skipped += 1;
            } else {
                entries.push(row.entry);
            }
        }
        Ok((entries, rows_skipped))
    }
}

fn existing_row_is_materially_identical(
    existing: &TimesheetEntry,
    incoming: &ParsedTimesheetRow,
) -> bool {
    let Some(start) = csv_import::parse_supported_timestamp(&existing.start_time) else {
        return false;
    };
    let Some(end) = csv_import::parse_supported_timestamp(&existing.end_time) else {
        return false;
    };
    material_fields_are_identical(
        existing,
        start,
        end,
        &incoming.entry,
        incoming.start,
        incoming.end,
    )
}

fn material_fields_are_identical(
    left: &TimesheetEntry,
    left_start: chrono::NaiveDateTime,
    left_end: chrono::NaiveDateTime,
    right: &TimesheetEntry,
    right_start: chrono::NaiveDateTime,
    right_end: chrono::NaiveDateTime,
) -> bool {
    left.personal_assistant_id.is_some()
        && left.personal_assistant_id == right.personal_assistant_id
        && left_start == right_start
        && left_end == right_end
        && left.break_minutes == right.break_minutes
        && left.worked_minutes == right.worked_minutes
        && left.hourly_rate.to_bits() == right.hourly_rate.to_bits()
        && left.amount.to_bits() == right.amount.to_bits()
        && left.notes == right.notes
}

fn import_time() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::create_schema;
    use crate::payroll_worked_item_repository::PayrollWorkedItemRepository;
    use rusqlite::{params, Connection};
    use tempfile::TempDir;

    fn setup(names: &[(&str, &str)]) -> (TempDir, PathBuf, TimesheetRepository) {
        let directory = TempDir::new().unwrap();
        fs::create_dir(directory.path().join("import")).unwrap();
        fs::create_dir(directory.path().join("archive")).unwrap();
        let database_path = directory.path().join("database.sqlite");
        let connection = Connection::open(&database_path).unwrap();
        create_schema(&connection).unwrap();
        for (first_name, surname) in names {
            connection
                .execute(
                    "INSERT INTO personal_assistants (
                        first_name, surname, sick_pay_enabled, mileage_enabled
                     ) VALUES (?1, ?2, 0, 0)",
                    params![first_name, surname],
                )
                .unwrap();
        }
        (
            directory,
            database_path,
            TimesheetRepository::new(connection),
        )
    }

    fn source(rows: &[&str]) -> Vec<u8> {
        format!(
            "period\nPA,Start,End,Break,Worked,Rate,Amount,Note\n{}\n",
            rows.join("\n")
        )
        .into_bytes()
    }

    fn write_source(directory: &TempDir, name: &str, rows: &[&str]) -> PathBuf {
        let path = directory.path().join("import").join(name);
        fs::write(&path, source(rows)).unwrap();
        path
    }

    fn run<'a>(directory: &'a TempDir, repository: &'a TimesheetRepository) -> ImportSummary {
        ImportService::new(
            repository,
            directory.path().join("import"),
            &directory.path().join("archive"),
        )
        .run()
        .unwrap()
    }

    fn count(database_path: &Path, table: &str) -> i64 {
        Connection::open(database_path)
            .unwrap()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
    }

    #[test]
    fn valid_mixed_pa_file_imports_atomically_and_archives_exact_bytes() {
        let (directory, database_path, repository) = setup(&[("Alex", "Smith"), ("Jo", "Jones")]);
        let path = write_source(
            &directory,
            "valid.csv",
            &[
                "  ALEX   SMITH ,27 July 2026 at 23:00:00,28 July 2026 at 01:00:00,0h 00m,1h 45m,£12.21,£21.37,",
                "Jo Jones,28 July 2026 at 09:00:00,28 July 2026 at 10:00:00,0h 00m,1h 00m,£13.00,£13.00,",
            ],
        );
        let expected = fs::read(path).unwrap();

        let summary = run(&directory, &repository);

        assert_eq!(summary.files_discovered, 1);
        assert_eq!(summary.files_succeeded, 1);
        assert_eq!(summary.rows_imported, 2);
        let entries = repository.get_all_raw().unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries
            .iter()
            .all(|entry| entry.personal_assistant_id.is_some()));
        assert_eq!(entries[0].worked_minutes, 105);
        let archived: String = Connection::open(&database_path)
            .unwrap()
            .query_row(
                "SELECT archive_filename FROM import_audit WHERE status = 'SUCCESS'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fs::read(archived).unwrap(), expected);
        assert_eq!(count(&database_path, "import_audit"), 1);
    }

    #[test]
    fn invalid_later_row_refuses_whole_file_and_later_file_still_succeeds() {
        let (directory, database_path, repository) = setup(&[("Alex", "Smith")]);
        write_source(
            &directory,
            "01_bad.csv",
            &[
                "Alex Smith,27 July 2026 at 09:00:00,27 July 2026 at 10:00:00,0h 00m,1h 00m,£12.00,£12.00,",
                "Alex Smith,bad,28 July 2026 at 10:00:00,0h 00m,1h 00m,£12.00,£12.00,",
            ],
        );
        write_source(
            &directory,
            "02_good.csv",
            &["Alex Smith,29 July 2026 at 09:00:00,29 July 2026 at 10:00:00,0h 00m,1h 00m,£12.00,£12.00,"],
        );

        let summary = run(&directory, &repository);

        assert_eq!(summary.files_refused, 1);
        assert_eq!(summary.files_succeeded, 1);
        assert_eq!(count(&database_path, "timesheets"), 1);
    }

    #[test]
    fn missing_ambiguous_pas_refused_but_conflicting_candidates_retained() {
        let (missing_dir, missing_db, missing_repo) = setup(&[("Alex", "Smith")]);
        write_source(
            &missing_dir,
            "missing.csv",
            &["Nobody Here,27 July 2026 at 09:00:00,27 July 2026 at 10:00:00,0h 00m,1h 00m,£12.00,£12.00,"],
        );
        assert_eq!(run(&missing_dir, &missing_repo).files_refused, 1);
        assert_eq!(count(&missing_db, "timesheets"), 0);

        let (ambiguous_dir, ambiguous_db, ambiguous_repo) =
            setup(&[("Alex", "Smith"), (" alex ", " smith ")]);
        write_source(
            &ambiguous_dir,
            "ambiguous.csv",
            &["Alex Smith,27 July 2026 at 09:00:00,27 July 2026 at 10:00:00,0h 00m,1h 00m,£12.00,£12.00,"],
        );
        assert_eq!(run(&ambiguous_dir, &ambiguous_repo).files_refused, 1);
        assert_eq!(count(&ambiguous_db, "timesheets"), 0);

        let (collision_dir, collision_db, collision_repo) = setup(&[("Alex", "Smith")]);
        write_source(
            &collision_dir,
            "collision.csv",
            &[
                "Alex Smith,27 July 2026 at 09:00:00,27 July 2026 at 10:00:00,0h 00m,1h 00m,£12.00,£12.00,",
                "Alex Smith,27 July 2026 at 09:00:00,27 July 2026 at 11:00:00,0h 00m,2h 00m,£12.00,£24.00,corrected",
            ],
        );
        assert_eq!(run(&collision_dir, &collision_repo).files_succeeded, 1);
        assert_eq!(count(&collision_db, "timesheets"), 2);
    }

    #[test]
    fn overlapping_export_skips_exact_existing_shift_and_imports_new_shift() {
        let (directory, database_path, repository) = setup(&[("Alex", "Smith")]);
        let existing = TimesheetEntry {
            id: 0,
            pa_name: "Alex Smith".to_string(),
            personal_assistant_id: Some(1),
            start_time: "27 July 2026 at 09:00:00".to_string(),
            end_time: "27 July 2026 at 10:00:00".to_string(),
            break_minutes: 0,
            worked_minutes: 60,
            hourly_rate: 12.0,
            amount: 12.0,
            notes: Some("historical".to_string()),
        };
        repository.insert(&existing).unwrap();
        let existing_id = repository.get_all_raw().unwrap()[0].id;
        write_source(
            &directory,
            "overlap.csv",
            &[
                " ALEX  SMITH ,27 July 2026 at 09:00:00,27 July 2026 at 10:00:00,0h 00m,1h 00m,£12.00,£12.00,historical",
                "Alex Smith,28 July 2026 at 09:00:00,28 July 2026 at 10:30:00,0h 00m,1h 30m,£12.00,£18.00,new",
            ],
        );

        let summary = run(&directory, &repository);

        assert_eq!(summary.files_succeeded, 1);
        assert_eq!(summary.rows_processed, 2);
        assert_eq!(summary.rows_imported, 1);
        assert_eq!(summary.rows_skipped, 1);
        let entries = repository.get_all_raw().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, existing_id);
        let audit: (i64, i64, i64) = Connection::open(&database_path)
            .unwrap()
            .query_row(
                "SELECT rows_processed, rows_imported, rows_skipped
                 FROM import_audit WHERE status = 'SUCCESS'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(audit, (2, 1, 1));
    }

    #[test]
    fn corrected_effective_state_does_not_change_raw_import_collision_identity() {
        use crate::repository::{TimesheetCorrectionProposal, LOCAL_CORRECTION_ACTOR_ID};

        let (directory, _database_path, repository) = setup(&[("Alex", "Smith")]);
        let raw_row = "Alex Smith,27 July 2026 at 09:00:00,27 July 2026 at 10:00:00,0h 00m,1h 00m,£12.00,£12.00,source";
        write_source(&directory, "original.csv", &[raw_row]);
        assert_eq!(run(&directory, &repository).rows_imported, 1);
        let raw = repository.get_all_raw().unwrap().remove(0);
        repository
            .append_correction(
                raw.id,
                &TimesheetCorrectionProposal {
                    start_time: "2026-07-27T09:00".to_string(),
                    end_time: "2026-07-27T11:00".to_string(),
                    break_minutes: 15,
                    worked_minutes: 90,
                    notes: Some("effective correction".to_string()),
                },
                LOCAL_CORRECTION_ACTOR_ID,
                "2026-07-28T10:00:00Z",
                None,
            )
            .unwrap();
        write_source(&directory, "renamed-copy.csv", &[raw_row]);

        let summary = run(&directory, &repository);

        assert_eq!(summary.files_already_imported, 1);
        assert_eq!(summary.files_succeeded, 1);
        assert_eq!(summary.rows_imported, 0);
        assert_eq!(summary.rows_skipped, 1);
        assert_eq!(repository.get_all_raw().unwrap(), vec![raw]);
        assert_eq!(
            repository.get_all_effective().unwrap()[0]
                .effective
                .worked_minutes,
            90
        );
    }

    #[test]
    fn exact_duplicate_candidates_within_file_are_retained_for_review() {
        let (directory, database_path, repository) = setup(&[("Alex", "Smith")]);
        let row = "Alex Smith,27 July 2026 at 09:00:00,27 July 2026 at 10:00:00,0h 00m,1h 00m,£12.00,£12.00,same";
        write_source(&directory, "duplicates.csv", &[row, row]);

        let summary = run(&directory, &repository);

        assert_eq!(summary.files_succeeded, 1);
        assert_eq!(summary.rows_processed, 2);
        assert_eq!(summary.rows_imported, 2);
        assert_eq!(summary.rows_skipped, 0);
        assert_eq!(count(&database_path, "timesheets"), 2);
    }

    #[test]
    fn archive_failure_writes_no_database_rows() {
        let (directory, database_path, repository) = setup(&[("Alex", "Smith")]);
        write_source(
            &directory,
            "valid.csv",
            &["Alex Smith,27 July 2026 at 09:00:00,27 July 2026 at 10:00:00,0h 00m,1h 00m,£12.00,£12.00,"],
        );
        fs::remove_dir(directory.path().join("archive")).unwrap();
        fs::write(directory.path().join("archive"), b"not a directory").unwrap();

        let summary = run(&directory, &repository);

        assert_eq!(summary.files_failed, 1);
        assert_eq!(count(&database_path, "timesheets"), 0);

        fs::remove_file(directory.path().join("archive")).unwrap();
        fs::create_dir(directory.path().join("archive")).unwrap();
        let retry = run(&directory, &repository);
        assert_eq!(retry.files_succeeded, 1);
        assert_eq!(count(&database_path, "timesheets"), 1);
    }

    #[test]
    fn database_audit_failure_rolls_back_rows_leaves_orphan_and_retry_is_safe() {
        let (directory, database_path, repository) = setup(&[("Alex", "Smith")]);
        write_source(
            &directory,
            "valid.csv",
            &["Alex Smith,27 July 2026 at 09:00:00,27 July 2026 at 10:00:00,0h 00m,1h 00m,£12.00,£12.00,"],
        );
        Connection::open(&database_path)
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER fail_success_audit BEFORE INSERT ON import_audit
                 WHEN NEW.status = 'SUCCESS' BEGIN SELECT RAISE(ABORT, 'test audit failure'); END;",
            )
            .unwrap();

        let failed = run(&directory, &repository);
        assert_eq!(failed.files_failed, 1);
        assert_eq!(failed.orphaned_archives.len(), 1);
        assert_eq!(count(&database_path, "timesheets"), 0);

        Connection::open(&database_path)
            .unwrap()
            .execute_batch("DROP TRIGGER fail_success_audit")
            .unwrap();
        let retried = run(&directory, &repository);
        assert_eq!(retried.files_succeeded, 1);
        assert_eq!(count(&database_path, "timesheets"), 1);
    }

    #[test]
    fn later_insert_failure_rolls_back_every_row() {
        let (directory, database_path, repository) = setup(&[("Alex", "Smith")]);
        write_source(
            &directory,
            "01_fails.csv",
            &[
                "Alex Smith,27 July 2026 at 09:00:00,27 July 2026 at 10:00:00,0h 00m,1h 00m,£12.00,£12.00,",
                "Alex Smith,28 July 2026 at 09:00:00,28 July 2026 at 10:00:00,0h 00m,1h 00m,£12.00,£12.00,FAIL",
            ],
        );
        write_source(
            &directory,
            "02_succeeds.csv",
            &["Alex Smith,29 July 2026 at 09:00:00,29 July 2026 at 10:00:00,0h 00m,1h 00m,£12.00,£12.00,"],
        );
        Connection::open(&database_path)
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER fail_second_row BEFORE INSERT ON timesheets
                 WHEN NEW.notes = 'FAIL' BEGIN SELECT RAISE(ABORT, 'test insert failure'); END;",
            )
            .unwrap();

        let summary = run(&directory, &repository);

        assert_eq!(summary.files_failed, 1);
        assert_eq!(summary.files_succeeded, 1);
        assert_eq!(count(&database_path, "timesheets"), 1);

        Connection::open(&database_path)
            .unwrap()
            .execute_batch("DROP TRIGGER fail_second_row")
            .unwrap();
        let retry = run(&directory, &repository);
        assert_eq!(retry.files_succeeded, 1);
        assert_eq!(retry.files_already_imported, 1);
        assert_eq!(count(&database_path, "timesheets"), 3);
    }

    #[test]
    fn existing_collision_preserves_entry_id_and_submitted_snapshot_membership() {
        let (directory, database_path, repository) = setup(&[("Alex", "Smith")]);
        let mut existing = TimesheetEntry {
            id: 0,
            pa_name: "Alex Smith".to_string(),
            personal_assistant_id: Some(1),
            start_time: "27 July 2026 at 09:00:00".to_string(),
            end_time: "27 July 2026 at 10:00:00".to_string(),
            break_minutes: 0,
            worked_minutes: 60,
            hourly_rate: 12.0,
            amount: 12.0,
            notes: None,
        };
        repository.insert(&existing).unwrap();
        existing.id = repository.get_all_raw().unwrap()[0].id;
        let connection = Connection::open(&database_path).unwrap();
        connection.execute("INSERT INTO payroll_timesheets (id, personal_assistant_id, payroll_year, cycle_number, created_at, updated_at) VALUES (10, 1, '2026 to 2027', 1, 'now', 'now')", []).unwrap();
        connection.execute("INSERT INTO payroll_timesheet_snapshot_states (payroll_timesheet_id, state, pdf_path, pdf_sha256, generated_at) VALUES (10, 'submitted', 'x', 'digest', 'now')", []).unwrap();
        connection.execute("INSERT INTO payroll_timesheet_worked_item_snapshots (payroll_timesheet_id, week_number, source_type, timesheet_id, work_date, worked_minutes, pay_rate_id, pay_rate_effective_date, total_hourly_rate, captured_at) VALUES (10, 1, 'imported_shift', ?1, '27/07/2026', 60, 1, '01/01/2026', 12.0, 'now')", [existing.id]).unwrap();
        write_source(
            &directory,
            "collision.csv",
            &[
                "Alex Smith,27 July 2026 at 09:00:00,27 July 2026 at 11:00:00,0h 00m,2h 00m,£12.00,£24.00,changed",
                "Alex Smith,28 July 2026 at 09:00:00,28 July 2026 at 10:00:00,0h 00m,1h 00m,£12.00,£12.00,new",
            ],
        );

        let summary = run(&directory, &repository);

        assert_eq!(summary.files_refused, 0);
        assert_eq!(summary.rows_imported, 2);
        assert_eq!(repository.get_all_raw().unwrap().len(), 3);
        assert_eq!(repository.get_all_raw().unwrap()[0].id, existing.id);
        let snapshots = PayrollWorkedItemRepository::new(Connection::open(&database_path).unwrap());
        assert!(snapshots
            .submitted_snapshot_timesheet_ids(10)
            .unwrap()
            .contains(&existing.id));
    }

    #[test]
    fn failed_audit_is_best_effort_and_changed_successful_path_is_conservatively_skipped() {
        let (directory, database_path, repository) = setup(&[("Alex", "Smith")]);
        write_source(
            &directory,
            "01_bad.csv",
            &["Nobody,bad,bad,broken,broken,bad,bad,"],
        );
        let good_path = write_source(
            &directory,
            "02_good.csv",
            &["Alex Smith,27 July 2026 at 09:00:00,27 July 2026 at 10:00:00,0h 00m,1h 00m,£12.00,£12.00,"],
        );
        Connection::open(&database_path)
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER fail_refused_audit BEFORE INSERT ON import_audit
             WHEN NEW.status = 'REFUSED' BEGIN SELECT RAISE(ABORT, 'test failed audit'); END;",
            )
            .unwrap();

        let summary = run(&directory, &repository);
        assert_eq!(summary.files_refused, 1);
        assert_eq!(summary.files_succeeded, 1);
        assert!(summary.failure_messages[0].contains("failed to record REFUSED audit"));
        assert_eq!(count(&database_path, "timesheets"), 1);

        fs::write(
            good_path,
            source(&["Alex Smith,28 July 2026 at 09:00:00,28 July 2026 at 10:00:00,0h 00m,1h 00m,£12.00,£12.00,"]),
        ).unwrap();
        let retry = run(&directory, &repository);
        assert_eq!(retry.files_already_imported, 1);
        assert_eq!(count(&database_path, "timesheets"), 1);
    }
}
