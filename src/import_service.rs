use std::error::Error;
use std::fs;

use crate::archive;
use crate::csv_import;
use crate::models::TimesheetEntry;
use crate::repository::TimesheetRepository;

use chrono::Local;

use std::path::{Path, PathBuf};

pub struct ImportSummary {
    pub files_processed: i64,
    pub rows_processed: i64,
    pub rows_imported: i64,
    pub rows_skipped: i64,
    pub files_failed: i64,
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
        let mut summary = ImportSummary {
            files_processed: 0,
            rows_processed: 0,
            rows_imported: 0,
            rows_skipped: 0,
            files_failed: 0,
        };

        let mut csv_files = Vec::new();

        for entry in fs::read_dir(&self.import_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("csv") {
                csv_files.push(path);
            }
        }

        csv_files.sort();

        for path in csv_files {
            summary.files_processed += 1;

            match self.process_file(&path) {
                Ok((processed, imported, skipped)) => {
                    summary.rows_processed += processed;
                    summary.rows_imported += imported;
                    summary.rows_skipped += skipped;
                }

                Err(error) => {
                    summary.files_failed += 1;

                    println!(
                        "Failed importing {:?}: {}",
                        path,
                        error
                    );

                    self.repository.add_import_audit(
                        &Local::now()
                            .format("%Y-%m-%d %H:%M:%S")
                            .to_string(),
                        path.to_string_lossy().as_ref(),
                        "",
                        0,
                        0,
                        0,
                        "FAILED",
                        Some(&error.to_string()),
                    )?;
                }
            }
        }

        Ok(summary)
    }

    fn process_file(
        &self,
        path: &Path,
    ) -> Result<(i64, i64, i64), Box<dyn Error>> {
        let entries: Vec<TimesheetEntry> =
            csv_import::import_csv(path)?;

        let rows_processed = entries.len() as i64;
        let mut rows_imported = 0;
        let mut rows_skipped = 0;

        for entry in entries {
            if self.repository.exists(&entry)? {
                rows_skipped += 1;
            } else {
                self.repository.insert(&entry)?;
                rows_imported += 1;
            }
        }

        let archive_file =
            archive::archive_csv(path, self.archive_dir)?;

        self.repository.add_import_audit(
            &Local::now()
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
            path.to_string_lossy().as_ref(),
            archive_file.to_string_lossy().as_ref(),
            rows_processed,
            rows_imported,
            rows_skipped,
            "SUCCESS",
            None,
        )?;

        Ok((
            rows_processed,
            rows_imported,
            rows_skipped,
        ))
    }
}

