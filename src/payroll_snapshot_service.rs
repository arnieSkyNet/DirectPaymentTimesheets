use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::payroll_worked_item_repository::{
    PayrollWorkedItemRepository, SnapshotState, WorkedItemSnapshot,
};

#[derive(Debug)]
pub enum SnapshotSafetyError {
    Refused(String),
    Transport(String),
    Indeterminate(String),
    Operation(String),
}

impl fmt::Display for SnapshotSafetyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(message) | Self::Transport(message) | Self::Operation(message) => {
                formatter.write_str(message)
            }
            Self::Indeterminate(message) => write!(
                formatter,
                "INDETERMINATE DELIVERY SAFETY CONDITION: {message} Delivery may have succeeded. Do not regenerate or resend this payroll timesheet until its state has been reconciled."
            ),
        }
    }
}

impl std::error::Error for SnapshotSafetyError {}

pub struct CandidatePublication<'a> {
    pub payroll_timesheet_id: i64,
    pub items: &'a [WorkedItemSnapshot],
    pub final_pdf_path: &'a Path,
    pub generated_at: &'a str,
    pub previous_cycle_minutes: i64,
    pub week_ids: &'a [i64; 4],
    pub week_totals_minutes: &'a [i64; 4],
}

pub fn publish_candidate<F>(
    repository: &PayrollWorkedItemRepository,
    publication: CandidatePublication<'_>,
    write_pdf: F,
) -> Result<(), SnapshotSafetyError>
where
    F: FnOnce(&Path) -> Result<(), Box<dyn std::error::Error>>,
{
    if let Some(metadata) = repository
        .snapshot_metadata(publication.payroll_timesheet_id)
        .map_err(operation_error)?
    {
        match metadata.state {
            SnapshotState::Candidate => {}
            SnapshotState::Submitted => {
                return Err(SnapshotSafetyError::Refused(
                    "This payroll timesheet has already been submitted. Its frozen worked-item baseline cannot be regenerated or replaced.".to_string(),
                ));
            }
            SnapshotState::Indeterminate => {
                return Err(SnapshotSafetyError::Refused(
                    "This payroll timesheet has an indeterminate delivery state and is protected from regeneration until reconciled.".to_string(),
                ));
            }
        }
    }

    if let Some(parent) = publication
        .final_pdf_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            SnapshotSafetyError::Operation(format!(
                "Could not create payroll PDF output directory {}: {error}",
                parent.display()
            ))
        })?;
    }

    let temporary_path = temporary_pdf_path(publication.final_pdf_path);
    if let Err(error) = write_pdf(&temporary_path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(SnapshotSafetyError::Operation(format!(
            "Could not generate temporary payroll PDF: {error}"
        )));
    }
    let digest = match sha256_file(&temporary_path) {
        Ok(digest) => digest,
        Err(error) => {
            let _ = fs::remove_file(&temporary_path);
            return Err(error);
        }
    };

    let final_path = publication.final_pdf_path.to_string_lossy().to_string();
    if let Err(error) = repository.replace_candidate(
        publication.payroll_timesheet_id,
        publication.items,
        &final_path,
        &digest,
        publication.generated_at,
        publication.previous_cycle_minutes,
        publication.week_ids,
        publication.week_totals_minutes,
    ) {
        let _ = fs::remove_file(&temporary_path);
        return Err(operation_error(error));
    }

    if let Err(error) = fs::rename(&temporary_path, publication.final_pdf_path) {
        let _ = repository.discard_candidate(publication.payroll_timesheet_id);
        let _ = fs::remove_file(&temporary_path);
        return Err(SnapshotSafetyError::Operation(format!(
            "Could not publish the generated payroll PDF; its candidate snapshot was invalidated: {error}"
        )));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn send_production_candidate<F>(
    repository: &PayrollWorkedItemRepository,
    payroll_timesheet_id: i64,
    personal_assistant_id: i64,
    payroll_year: &str,
    cycle_number: i64,
    attachment_path: &Path,
    attempted_at: &str,
    send: F,
) -> Result<(), SnapshotSafetyError>
where
    F: FnOnce() -> Result<(), Box<dyn std::error::Error>>,
{
    verify_candidate(repository, payroll_timesheet_id, attachment_path)?;
    if !repository
        .protect_for_send(payroll_timesheet_id, attempted_at)
        .map_err(operation_error)?
    {
        return Err(SnapshotSafetyError::Refused(
            "The generated candidate could not be protected for production sending.".to_string(),
        ));
    }

    if let Err(error) = send() {
        repository
            .restore_candidate_after_failed_send(payroll_timesheet_id)
            .map_err(|restore_error| {
                SnapshotSafetyError::Indeterminate(format!(
                    "SMTP reported an error ({error}), and the protected state could not be restored: {restore_error}."
                ))
            })?;
        return Err(SnapshotSafetyError::Transport(format!(
            "Timesheet email failed before successful delivery was confirmed: {error}"
        )));
    }

    let sent_at = chrono::Local::now().to_rfc3339();
    repository
        .mark_submitted_and_email_sent(
            payroll_timesheet_id,
            personal_assistant_id,
            payroll_year,
            cycle_number,
            &sent_at,
        )
        .map_err(|error| {
            SnapshotSafetyError::Indeterminate(format!(
                "SMTP transport completed, but the submitted/frozen state could not be persisted: {error}."
            ))
        })
}

pub fn verify_candidate(
    repository: &PayrollWorkedItemRepository,
    payroll_timesheet_id: i64,
    attachment_path: &Path,
) -> Result<(), SnapshotSafetyError> {
    let metadata = repository
        .snapshot_metadata(payroll_timesheet_id)
        .map_err(operation_error)?
        .ok_or_else(|| {
            SnapshotSafetyError::Refused(
                "No generated candidate snapshot exists for this payroll timesheet.".to_string(),
            )
        })?;
    match metadata.state {
        SnapshotState::Candidate => {}
        SnapshotState::Submitted => {
            return Err(SnapshotSafetyError::Refused(
                "This payroll timesheet has already been submitted.".to_string(),
            ));
        }
        SnapshotState::Indeterminate => {
            return Err(SnapshotSafetyError::Indeterminate(
                "The prior production-send result has not been reconciled.".to_string(),
            ));
        }
    }
    if metadata.pdf_path != attachment_path.to_string_lossy() {
        return Err(SnapshotSafetyError::Refused(
            "The selected PDF path does not match the generated candidate snapshot.".to_string(),
        ));
    }
    if !attachment_path.is_file() {
        return Err(SnapshotSafetyError::Refused(format!(
            "The generated candidate PDF is missing: {}",
            attachment_path.display()
        )));
    }
    let actual_digest = sha256_file(attachment_path)?;
    if actual_digest != metadata.pdf_sha256 {
        return Err(SnapshotSafetyError::Refused(
            "The payroll PDF has changed since its candidate snapshot was generated. Regenerate it before production sending.".to_string(),
        ));
    }
    Ok(())
}

pub fn sha256_file(path: &Path) -> Result<String, SnapshotSafetyError> {
    let bytes = fs::read(path).map_err(|error| {
        SnapshotSafetyError::Operation(format!(
            "Could not read payroll PDF {}: {error}",
            path.display()
        ))
    })?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn temporary_pdf_path(final_path: &Path) -> PathBuf {
    let file_name = final_path.file_name().unwrap_or_default().to_string_lossy();
    final_path.with_file_name(format!(
        ".{file_name}.{}.tmp",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ))
}

fn operation_error(error: rusqlite::Error) -> SnapshotSafetyError {
    SnapshotSafetyError::Operation(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::create_schema;
    use crate::email_service::{
        preview_payroll_email, preview_test_payroll_email, send_test_payroll_email,
    };
    use crate::payroll_worked_item_repository::{SnapshotState, WorkedItemSnapshot};
    use rusqlite::Connection;
    use tempfile::TempDir;

    fn repository() -> (TempDir, PayrollWorkedItemRepository) {
        let directory = TempDir::new().unwrap();
        let database = directory.path().join("test.sqlite");
        let connection = Connection::open(&database).unwrap();
        create_schema(&connection).unwrap();
        drop(connection);
        (
            directory,
            PayrollWorkedItemRepository::new(Connection::open(database).unwrap()),
        )
    }

    fn item(id: i64) -> WorkedItemSnapshot {
        WorkedItemSnapshot {
            week_number: 1,
            source_type: "imported_shift".to_string(),
            timesheet_id: Some(id),
            work_date: Some("2027-03-01".to_string()),
            worked_minutes: 60,
            pay_rate_id: Some(1),
            pay_rate_effective_date: Some("01/01/2027".to_string()),
            total_hourly_rate: Some(12.21),
            reason: None,
        }
    }

    fn schedule(
        payroll_year: &str,
        first_week_commencing: &str,
        pay_date: &str,
    ) -> crate::payroll_schedule_repository::PayrollSchedule {
        crate::payroll_schedule_repository::PayrollSchedule {
            id: 1,
            payroll_year: payroll_year.to_string(),
            cycle_number: 1,
            first_week_commencing: first_week_commencing.to_string(),
            latest_posting_date: String::new(),
            pay_date: pay_date.to_string(),
            created_at: String::new(),
            payslips_sent: false,
        }
    }

    fn publish(
        repository: &PayrollWorkedItemRepository,
        path: &Path,
        item_id: i64,
    ) -> Result<(), SnapshotSafetyError> {
        publish_candidate(
            repository,
            CandidatePublication {
                payroll_timesheet_id: 10,
                items: &[item(item_id)],
                final_pdf_path: path,
                generated_at: "2027-03-01T10:00:00Z",
                previous_cycle_minutes: 0,
                week_ids: &[0; 4],
                week_totals_minutes: &[60, 0, 0, 0],
            },
            |temporary| {
                assert_ne!(temporary, path);
                fs::write(temporary, format!("PDF {item_id}"))?;
                Ok(())
            },
        )
    }

    #[test]
    fn candidate_can_be_replaced_before_send_and_uses_temporary_publication() {
        let (directory, repository) = repository();
        let path = directory.path().join("timesheet.pdf");
        publish(&repository, &path, 1).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "PDF 1");
        publish_candidate(
            &repository,
            CandidatePublication {
                payroll_timesheet_id: 10,
                items: &[item(2)],
                final_pdf_path: &path,
                generated_at: "2027-03-01T11:00:00Z",
                previous_cycle_minutes: 0,
                week_ids: &[0; 4],
                week_totals_minutes: &[60, 0, 0, 0],
            },
            |temporary| {
                assert_ne!(temporary, path);
                fs::write(temporary, "PDF 2")?;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "PDF 2");
        assert_eq!(
            repository.get_snapshot(10).unwrap()[0].timesheet_id,
            Some(2)
        );
    }

    #[test]
    fn year_suffixed_root_creates_future_year_sibling_only_when_publishing() {
        let (directory, repository) = repository();
        let configured_root = directory.path().join("Timesheets/2026 to 2027");
        fs::create_dir_all(&configured_root).unwrap();
        let future_schedule = schedule("2027/28", "22/03/2027", "16/04/2027");
        let path = crate::payroll_file_naming::timesheet_path(
            &configured_root,
            "Alex Smith",
            &future_schedule,
        )
        .unwrap();
        let future_year_directory = directory.path().join("Timesheets/2027 to 2028");

        assert_eq!(path.parent(), Some(future_year_directory.as_path()));
        assert!(!future_year_directory.exists());

        publish(&repository, &path, 1).unwrap();

        assert!(future_year_directory.is_dir());
        assert!(path.is_file());
        assert!(configured_root.is_dir());
    }

    #[test]
    fn generic_root_remains_flat_when_publishing() {
        let (directory, repository) = repository();
        let configured_root = directory.path().join("Timesheets");
        let future_schedule = schedule("2027/28", "22/03/2027", "16/04/2027");
        let path = crate::payroll_file_naming::timesheet_path(
            &configured_root,
            "Alex Smith",
            &future_schedule,
        )
        .unwrap();

        publish(&repository, &path, 1).unwrap();

        assert_eq!(path.parent(), Some(configured_root.as_path()));
        assert!(path.is_file());
        assert!(!configured_root.join("2027 to 2028").exists());
    }

    #[test]
    fn publishing_into_an_existing_destination_directory_still_works() {
        let (directory, repository) = repository();
        let destination = directory.path().join("existing");
        fs::create_dir_all(&destination).unwrap();
        let path = destination.join("timesheet.pdf");

        publish(&repository, &path, 1).unwrap();

        assert_eq!(fs::read_to_string(path).unwrap(), "PDF 1");
    }

    #[test]
    fn directory_creation_failure_does_not_create_candidate_state() {
        let (directory, repository) = repository();
        let blocked_parent = directory.path().join("not-a-directory");
        fs::write(&blocked_parent, "file").unwrap();
        let path = blocked_parent.join("timesheet.pdf");

        let error = publish(&repository, &path, 1).unwrap_err();

        assert!(error
            .to_string()
            .contains("Could not create payroll PDF output directory"));
        assert!(repository.snapshot_metadata(10).unwrap().is_none());
    }

    #[test]
    fn matching_candidate_submits_and_then_cannot_be_replaced() {
        let (directory, repository) = repository();
        let path = directory.path().join("timesheet.pdf");
        publish(&repository, &path, 1).unwrap();
        send_production_candidate(
            &repository,
            10,
            1,
            "2026/27",
            1,
            &path,
            "2027-03-01T12:00:00Z",
            || Ok(()),
        )
        .unwrap();
        assert_eq!(
            repository.snapshot_metadata(10).unwrap().unwrap().state,
            SnapshotState::Submitted
        );
        let error = publish(&repository, &path, 2).unwrap_err().to_string();
        assert!(error.contains("already been submitted"));
        assert_eq!(
            repository.get_snapshot(10).unwrap()[0].timesheet_id,
            Some(1)
        );
    }

    #[test]
    fn production_send_requires_candidate_and_rejects_altered_pdf() {
        let (directory, repository) = repository();
        let path = directory.path().join("timesheet.pdf");
        fs::write(&path, "untracked").unwrap();
        assert!(verify_candidate(&repository, 10, &path)
            .unwrap_err()
            .to_string()
            .contains("No generated candidate"));
        fs::remove_file(&path).unwrap();
        publish(&repository, &path, 1).unwrap();
        fs::write(&path, "altered").unwrap();
        assert!(verify_candidate(&repository, 10, &path)
            .unwrap_err()
            .to_string()
            .contains("has changed"));
        assert_eq!(
            repository.snapshot_metadata(10).unwrap().unwrap().state,
            SnapshotState::Candidate
        );
    }

    #[test]
    fn stale_pdf_from_an_older_candidate_is_rejected() {
        let (directory, repository) = repository();
        let path = directory.path().join("timesheet.pdf");
        publish(&repository, &path, 1).unwrap();
        let old_pdf = fs::read(&path).unwrap();
        publish(&repository, &path, 2).unwrap();
        fs::write(&path, old_pdf).unwrap();

        let error = verify_candidate(&repository, 10, &path)
            .unwrap_err()
            .to_string();
        assert!(error.contains("has changed"));
        assert_eq!(
            repository.get_snapshot(10).unwrap()[0].timesheet_id,
            Some(2)
        );
    }

    #[test]
    fn smtp_failure_restores_replaceable_candidate() {
        let (directory, repository) = repository();
        let path = directory.path().join("timesheet.pdf");
        publish(&repository, &path, 1).unwrap();
        let error = send_production_candidate(
            &repository,
            10,
            1,
            "2026/27",
            1,
            &path,
            "2027-03-01T12:00:00Z",
            || Err("simulated SMTP failure".into()),
        )
        .unwrap_err();
        assert!(matches!(error, SnapshotSafetyError::Transport(_)));
        assert_eq!(
            repository.snapshot_metadata(10).unwrap().unwrap().state,
            SnapshotState::Candidate
        );
        publish(&repository, &path, 2).unwrap();
    }

    #[test]
    fn database_failure_after_transport_leaves_protected_indeterminate_state() {
        let (directory, repository) = repository();
        let path = directory.path().join("timesheet.pdf");
        publish(&repository, &path, 1).unwrap();
        let database = directory.path().join("test.sqlite");
        Connection::open(database)
            .unwrap()
            .execute("DROP TABLE payroll_timesheet_email_status", [])
            .unwrap();

        let error = send_production_candidate(
            &repository,
            10,
            1,
            "2026/27",
            1,
            &path,
            "2027-03-01T12:00:00Z",
            || Ok(()),
        )
        .unwrap_err();
        assert!(matches!(error, SnapshotSafetyError::Indeterminate(_)));
        assert!(error.to_string().contains("Delivery may have succeeded"));
        assert_eq!(
            repository.snapshot_metadata(10).unwrap().unwrap().state,
            SnapshotState::Indeterminate
        );
        assert!(publish(&repository, &path, 2).is_err());
        assert!(send_production_candidate(
            &repository,
            10,
            1,
            "2026/27",
            1,
            &path,
            "2027-03-01T13:00:00Z",
            || Ok(())
        )
        .is_err());
    }

    #[test]
    fn preview_and_test_preview_do_not_freeze_candidate() {
        let (directory, repository) = repository();
        let path = directory.path().join("timesheet.pdf");
        publish(&repository, &path, 1).unwrap();
        preview_payroll_email(
            "payroll@example.com",
            "employer@example.com",
            None,
            "Test PA",
            None,
            None,
            "01/03/2027",
            &path,
            "Body",
            None,
            None,
            "Timesheet {Personal Assistant Name}",
        )
        .unwrap();
        preview_test_payroll_email(
            "employer@example.com",
            "test@example.com",
            None,
            "Test PA",
            None,
            None,
            "01/03/2027",
            &path,
            "Body",
            None,
            None,
            "Timesheet {Personal Assistant Name}",
        )
        .unwrap();
        let missing_attachment = directory.path().join("missing.pdf");
        assert!(send_test_payroll_email(
            "localhost",
            1,
            "",
            "",
            "employer@example.com",
            "test@example.com",
            None,
            "Test PA",
            None,
            None,
            "01/03/2027",
            &missing_attachment,
            "Body",
            None,
            None,
            "Timesheet {Personal Assistant Name}",
        )
        .is_err());
        assert_eq!(
            repository.snapshot_metadata(10).unwrap().unwrap().state,
            SnapshotState::Candidate
        );
    }

    #[test]
    fn database_reconciliation_failure_leaves_only_the_old_published_pdf() {
        let (directory, repository) = repository();
        let path = directory.path().join("timesheet.pdf");
        fs::write(&path, "old published PDF").unwrap();
        let database = directory.path().join("test.sqlite");
        Connection::open(database)
            .unwrap()
            .execute("DROP TABLE payroll_timesheet_weeks", [])
            .unwrap();

        let result = publish_candidate(
            &repository,
            CandidatePublication {
                payroll_timesheet_id: 10,
                items: &[item(1)],
                final_pdf_path: &path,
                generated_at: "2027-03-01T10:00:00Z",
                previous_cycle_minutes: 0,
                week_ids: &[1, 2, 3, 4],
                week_totals_minutes: &[60, 0, 0, 0],
            },
            |temporary| {
                fs::write(temporary, "new unpublished PDF")?;
                Ok(())
            },
        );

        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "old published PDF");
        assert!(repository.snapshot_metadata(10).unwrap().is_none());
        assert!(!directory
            .path()
            .read_dir()
            .unwrap()
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().ends_with(".tmp")));
    }
}
