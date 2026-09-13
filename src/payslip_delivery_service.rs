use std::fmt;

use crate::payroll_schedule_repository::PayrollScheduleRepository;
use crate::payroll_timesheet_email_repository::{
    EmailDeliveryState, PayrollTimesheetEmailRepository,
};

#[derive(Debug)]
pub enum PayslipDeliveryError {
    Refused(String),
    Transport(String),
    Indeterminate(String),
    Operation(String),
}

impl fmt::Display for PayslipDeliveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(message) | Self::Transport(message) | Self::Operation(message) => {
                formatter.write_str(message)
            }
            Self::Indeterminate(message) => write!(
                formatter,
                "INDETERMINATE PAYSLIP DELIVERY: {message} Delivery may have occurred. Automatic resend is refused; verify delivery outside the application before any future recovery action."
            ),
        }
    }
}

impl std::error::Error for PayslipDeliveryError {}

pub struct PayslipDeliveryIdentity<'a> {
    pub personal_assistant_id: i64,
    pub payroll_year: &'a str,
    pub cycle_number: i64,
}

#[derive(Debug, Default)]
pub struct PayslipEmailBundle {
    pub paths: Vec<std::path::PathBuf>,
    pub email_types: Vec<&'static str>,
    pub document_ids: Vec<i64>,
}

/// Build the same attachment set for preview and production. Sent types are
/// excluded; an uncertain delivery is never silently retried.
#[cfg(test)]
pub fn select_unsent_payslip_documents(
    repository: &PayrollTimesheetEmailRepository,
    identity: PayslipDeliveryIdentity<'_>,
    payslip_path: &std::path::Path,
) -> Result<PayslipEmailBundle, Box<dyn std::error::Error>> {
    select_payroll_documents(
        repository,
        identity.personal_assistant_id,
        Some((identity, payslip_path)),
    )
}

pub fn select_payroll_documents(
    repository: &PayrollTimesheetEmailRepository,
    pa: i64,
    ordinary: Option<(PayslipDeliveryIdentity<'_>, &std::path::Path)>,
) -> Result<PayslipEmailBundle, Box<dyn std::error::Error>> {
    let mut bundle = PayslipEmailBundle::default();
    let ordinary = match ordinary {
        Some((identity, path)) if path.try_exists()? => Some((identity, path)),
        _ => None,
    };
    if let Some((identity, payslip_path)) = ordinary {
        let status = repository.get_for_pa_and_cycle(
            identity.personal_assistant_id,
            identity.payroll_year,
            identity.cycle_number,
            "payslip",
        )?;
        match status.map(|s| s.delivery_state).unwrap_or(EmailDeliveryState::Unsent) {
            EmailDeliveryState::Sent { .. } => {},
            EmailDeliveryState::Indeterminate { attempted_at } => return Err(format!("Payslip delivery is indeterminate from {attempted_at}; automatic resend is refused.").into()),
            EmailDeliveryState::Unsent => {
                crate::archive::validate_payslip_pdf(payslip_path)?;
                bundle.paths.push(payslip_path.to_path_buf());
                bundle.email_types.push("payslip");
            }
        }
    }
    for document in repository.documents_for_pa(pa)? {
        if document.personal_assistant_id != pa {
            return Err("Payroll document PA identity changed.".into());
        }
        match document.delivery_state {
            EmailDeliveryState::Sent { .. } => continue,
            EmailDeliveryState::Indeterminate { attempted_at } => return Err(format!("{} document {} delivery is indeterminate from {attempted_at}; automatic resend is refused.", document.document_type, document.id).into()),
            EmailDeliveryState::Unsent => {}
        }
        crate::archive::validate_payslip_pdf(&document.path)?;
        if crate::payroll_document_repository::file_digest(&document.path)? != document.sha256 {
            return Err(format!(
                "Payroll document {} ({}) has changed since import; delivery refused.",
                document.id,
                document
                    .document_year
                    .as_deref()
                    .unwrap_or("year unspecified")
            )
            .into());
        }
        let kind = match document.document_type.as_str() {
            "p60" => "p60",
            "p45" => "p45",
            _ => return Err("Only P60/P45 documents can enter PA payroll delivery.".into()),
        };
        bundle.paths.push(document.path);
        bundle.document_ids.push(document.id);
        if !bundle.email_types.contains(&kind) {
            bundle.email_types.push(kind);
        }
    }
    Ok(bundle)
}

#[allow(dead_code)] // Retain the existing single-payslip API.
pub fn send_production_payslip<F>(
    repository: &PayrollTimesheetEmailRepository,
    identity: PayslipDeliveryIdentity<'_>,
    attempted_at: &str,
    send: F,
) -> Result<(), PayslipDeliveryError>
where
    F: FnOnce() -> Result<(), Box<dyn std::error::Error>>,
{
    send_production_payslip_bundle(
        repository,
        identity,
        &PayslipEmailBundle {
            email_types: vec!["payslip"],
            ..Default::default()
        },
        attempted_at,
        send,
    )
}

pub fn send_production_payslip_bundle<F>(
    repository: &PayrollTimesheetEmailRepository,
    identity: PayslipDeliveryIdentity<'_>,
    bundle: &PayslipEmailBundle,
    attempted_at: &str,
    send: F,
) -> Result<(), PayslipDeliveryError>
where
    F: FnOnce() -> Result<(), Box<dyn std::error::Error>>,
{
    send_payroll_bundle(
        repository,
        identity.personal_assistant_id,
        Some(identity),
        bundle,
        attempted_at,
        send,
    )
}

pub fn send_payroll_bundle<F>(
    repository: &PayrollTimesheetEmailRepository,
    pa: i64,
    identity: Option<PayslipDeliveryIdentity<'_>>,
    bundle: &PayslipEmailBundle,
    attempted_at: &str,
    send: F,
) -> Result<(), PayslipDeliveryError>
where
    F: FnOnce() -> Result<(), Box<dyn std::error::Error>>,
{
    let payslip = bundle.email_types.contains(&"payslip");
    if (!payslip && bundle.document_ids.is_empty())
        || bundle
            .email_types
            .iter()
            .any(|kind| !matches!(*kind, "payslip" | "p60" | "p45"))
    {
        return Err(PayslipDeliveryError::Refused(
            "Select at least one unsent PA payroll document.".into(),
        ));
    }
    let marker = format!("indeterminate:{attempted_at}");
    let transition = |expected: Option<&str>, next: Option<&str>| {
        repository.transition_payroll_bundle(
            pa,
            identity.as_ref(),
            payslip,
            &bundle.document_ids,
            expected,
            next,
        )
    };
    if !transition(None, Some(&marker)).map_err(operation_error)? {
        return Err(PayslipDeliveryError::Refused("The payslip attachments could not be protected with a durable indeterminate delivery state before SMTP; nothing was sent.".into()));
    }
    if let Err(error) = send() {
        return match transition(Some(&marker), None) {
            Ok(true) => Err(PayslipDeliveryError::Transport(format!("Payslip SMTP failed before successful delivery was confirmed: {error}"))),
            result => Err(PayslipDeliveryError::Indeterminate(format!("SMTP reported an error ({error}), but the protected attachment states could not all be restored: {result:?}"))),
        };
    }
    let sent_at = chrono::Local::now().to_rfc3339();
    match transition(Some(&marker), Some(&sent_at)) {
        Ok(true) => Ok(()),
        result => Err(PayslipDeliveryError::Indeterminate(format!("SMTP completed, but definitive sent state could not be persisted for all attachments: {result:?}"))),
    }
}

pub fn all_payslips_definitively_sent(
    repository: &PayrollTimesheetEmailRepository,
    identities: &[PayslipDeliveryIdentity<'_>],
) -> Result<bool, PayslipDeliveryError> {
    for identity in identities {
        let sent = repository
            .get_for_pa_and_cycle(
                identity.personal_assistant_id,
                identity.payroll_year,
                identity.cycle_number,
                "payslip",
            )
            .map_err(operation_error)?
            .is_some_and(|status| status.is_definitively_sent());
        if !sent {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn mark_schedule_sent_if_complete(
    email_repository: &PayrollTimesheetEmailRepository,
    schedule_repository: &PayrollScheduleRepository,
    schedule_id: i64,
    identities: &[PayslipDeliveryIdentity<'_>],
) -> Result<bool, PayslipDeliveryError> {
    if !all_payslips_definitively_sent(email_repository, identities)? {
        return Ok(false);
    }
    let marked = schedule_repository
        .mark_payslips_sent(schedule_id)
        .map_err(|error| {
            PayslipDeliveryError::Operation(format!(
                "Every required payslip is sent, but schedule completion could not be persisted: {error}"
            ))
        })?;
    if !marked {
        return Err(PayslipDeliveryError::Operation(
            "Every required payslip is sent, but the selected payroll schedule no longer exists and could not be marked complete."
                .to_string(),
        ));
    }
    Ok(true)
}

fn operation_error(error: rusqlite::Error) -> PayslipDeliveryError {
    PayslipDeliveryError::Operation(format!(
        "Could not read or update payslip delivery safety state: {error}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::create_schema;
    use crate::email_service::{preview_payroll_email, preview_test_payslip_email};
    use rusqlite::Connection;

    fn repository() -> (tempfile::TempDir, PayrollTimesheetEmailRepository) {
        let directory = tempfile::TempDir::new().unwrap();
        let database = directory.path().join("delivery.sqlite");
        let connection = Connection::open(database).unwrap();
        create_schema(&connection).unwrap();
        connection.execute_batch("INSERT INTO personal_assistants(id, first_name, surname) VALUES (1, 'Test', 'One'), (2, 'Test', 'Two');").unwrap();
        (directory, PayrollTimesheetEmailRepository::new(connection))
    }

    fn identity() -> PayslipDeliveryIdentity<'static> {
        PayslipDeliveryIdentity {
            personal_assistant_id: 1,
            payroll_year: "2026/27",
            cycle_number: 6,
        }
    }

    fn state(repository: &PayrollTimesheetEmailRepository) -> EmailDeliveryState {
        repository
            .get_for_pa_and_cycle(1, "2026/27", 6, "payslip")
            .unwrap()
            .map(|status| status.delivery_state)
            .unwrap_or(EmailDeliveryState::Unsent)
    }

    fn add_document(
        repository: &PayrollTimesheetEmailRepository,
        directory: &std::path::Path,
        filename: &str,
        kind: &str,
    ) -> i64 {
        let path = directory.join(filename);
        std::fs::write(&path, format!("%PDF-1.4 {filename}")).unwrap();
        repository
            .register_document(1, kind, &path, Some("2025/26"))
            .unwrap()
    }

    #[test]
    fn standalone_combined_and_later_documents_use_document_ids_across_cycles() {
        for (exists, sent) in [(false, false), (true, false), (true, true)] {
            let (dir, repository) = repository();
            let payslip = dir.path().join("payslip.pdf");
            if exists {
                std::fs::write(&payslip, b"%PDF-1.4 payslip").unwrap();
            }
            if sent {
                repository
                    .mark_sent(1, "2026/27", 6, "payslip", "previous")
                    .unwrap();
            }
            let ids = [
                add_document(&repository, dir.path(), "p60.pdf", "p60"),
                add_document(&repository, dir.path(), "p45.pdf", "p45"),
            ];
            let bundle =
                select_unsent_payslip_documents(&repository, identity(), &payslip).unwrap();
            assert_eq!(bundle.document_ids, ids);
            assert_eq!(bundle.paths.len(), 2 + usize::from(exists && !sent));
            send_production_payslip_bundle(&repository, identity(), &bundle, "attempt", || {
                assert!(repository
                    .documents_for_pa(1)?
                    .iter()
                    .all(|d| matches!(d.delivery_state, EmailDeliveryState::Indeterminate { .. })));
                Ok(())
            })
            .unwrap();
            assert!(repository
                .documents_for_pa(1)
                .unwrap()
                .iter()
                .all(|d| matches!(d.delivery_state, EmailDeliveryState::Sent { .. })));
            if sent {
                assert_eq!(
                    repository
                        .get_for_pa_and_cycle(1, "2026/27", 6, "payslip")
                        .unwrap()
                        .unwrap()
                        .sent_at
                        .as_deref(),
                    Some("previous")
                );
            }
            if !exists {
                assert_eq!(state(&repository), EmailDeliveryState::Unsent);
            }
            let other = PayslipDeliveryIdentity {
                personal_assistant_id: 1,
                payroll_year: "2025/26",
                cycle_number: 13,
            };
            assert!(select_unsent_payslip_documents(
                &repository,
                other,
                &dir.path().join("missing")
            )
            .unwrap()
            .paths
            .is_empty());
            let later = add_document(&repository, dir.path(), "p60 second.pdf", "p60");
            let next = select_unsent_payslip_documents(&repository, identity(), &payslip).unwrap();
            assert_eq!(next.document_ids, vec![later]);
            assert_eq!(next.paths.len(), 1);
            assert_eq!(repository.connection.query_row("SELECT COUNT(*) FROM payroll_timesheet_email_status WHERE email_type IN ('p60', 'p45')", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
        }
    }

    #[test]
    fn standalone_documents_send_and_retry_without_any_cycle_identity() {
        let (dir, repository) = repository();
        add_document(&repository, dir.path(), "p60.pdf", "p60");
        add_document(&repository, dir.path(), "p45.pdf", "p45");
        let bundle = select_payroll_documents(&repository, 1, None).unwrap();
        assert_eq!(bundle.paths.len(), 2);
        assert!(matches!(
            send_payroll_bundle(&repository, 1, None, &bundle, "failed", || Err(
                "SMTP refused".into()
            )),
            Err(PayslipDeliveryError::Transport(_))
        ));
        assert!(repository
            .documents_for_pa(1)
            .unwrap()
            .iter()
            .all(|d| d.delivery_state == EmailDeliveryState::Unsent));
        send_payroll_bundle(&repository, 1, None, &bundle, "success", || Ok(())).unwrap();
        assert!(select_payroll_documents(&repository, 1, None)
            .unwrap()
            .paths
            .is_empty());
        assert!(repository
            .get_for_pa_and_cycle(1, "2026/27", 6, "payslip")
            .unwrap()
            .is_none());
    }

    #[test]
    fn failed_mixed_email_restores_documents_and_payslip_to_unsent() {
        let (dir, repository) = repository();
        let id = add_document(&repository, dir.path(), "p60.pdf", "p60");
        let bundle = PayslipEmailBundle {
            email_types: vec!["payslip", "p60"],
            document_ids: vec![id],
            ..Default::default()
        };
        assert!(matches!(
            send_production_payslip_bundle(&repository, identity(), &bundle, "attempt", || Err(
                "SMTP refused".into()
            )),
            Err(PayslipDeliveryError::Transport(_))
        ));
        assert_eq!(state(&repository), EmailDeliveryState::Unsent);
        assert_eq!(
            repository.documents_for_pa(1).unwrap()[0].delivery_state,
            EmailDeliveryState::Unsent
        );
    }

    #[test]
    fn final_document_status_failure_rolls_back_entire_bundle_and_refuses_retry() {
        let (dir, repository) = repository();
        let id = add_document(&repository, dir.path(), "p45.pdf", "p45");
        let bundle = PayslipEmailBundle {
            email_types: vec!["payslip", "p45"],
            document_ids: vec![id],
            ..Default::default()
        };
        let result = send_production_payslip_bundle(
            &repository,
            identity(),
            &bundle,
            "attempt",
            || {
                repository.connection.execute_batch("CREATE TRIGGER refuse_document_sent BEFORE UPDATE OF sent_at ON imported_payroll_documents
                WHEN NEW.sent_at NOT LIKE 'indeterminate:%' BEGIN SELECT RAISE(FAIL, 'failure'); END;")?;
                Ok(())
            },
        );
        assert!(matches!(
            result,
            Err(PayslipDeliveryError::Indeterminate(_))
        ));
        assert!(matches!(
            state(&repository),
            EmailDeliveryState::Indeterminate { .. }
        ));
        assert!(matches!(
            repository.documents_for_pa(1).unwrap()[0].delivery_state,
            EmailDeliveryState::Indeterminate { .. }
        ));
        assert!(matches!(
            send_production_payslip_bundle(&repository, identity(), &bundle, "retry", || panic!(
                "must not resend"
            )),
            Err(PayslipDeliveryError::Refused(_))
        ));
    }

    #[test]
    fn stale_or_wrong_pa_document_rolls_back_payslip_protection() {
        let (dir, repository) = repository();
        let id = add_document(&repository, dir.path(), "p60.pdf", "p60");
        repository
            .connection
            .execute(
                "UPDATE imported_payroll_documents SET sent_at = 'previous' WHERE id = ?1",
                [id],
            )
            .unwrap();
        let bundle = PayslipEmailBundle {
            email_types: vec!["payslip", "p60"],
            document_ids: vec![id],
            ..Default::default()
        };
        assert!(matches!(
            send_production_payslip_bundle(&repository, identity(), &bundle, "attempt", || panic!(
                "must not send"
            )),
            Err(PayslipDeliveryError::Refused(_))
        ));
        assert_eq!(state(&repository), EmailDeliveryState::Unsent);
        repository.connection.execute("UPDATE imported_payroll_documents SET sent_at = NULL, personal_assistant_id = 2 WHERE id = ?1", [id]).unwrap();
        assert!(matches!(
            send_production_payslip_bundle(&repository, identity(), &bundle, "attempt", || panic!(
                "wrong PA"
            )),
            Err(PayslipDeliveryError::Refused(_))
        ));
        assert_eq!(state(&repository), EmailDeliveryState::Unsent);
    }

    #[test]
    fn selection_refuses_changed_or_indeterminate_document_and_never_general_files() {
        let (dir, repository) = repository();
        let id = add_document(&repository, dir.path(), "p60.pdf", "p60");
        let missing = dir.path().join("missing");
        repository.connection.execute("UPDATE imported_payroll_documents SET sent_at = 'indeterminate:attempt' WHERE id = ?1", [id]).unwrap();
        assert!(
            select_unsent_payslip_documents(&repository, identity(), &missing)
                .unwrap_err()
                .to_string()
                .contains("indeterminate")
        );
        repository
            .connection
            .execute("UPDATE imported_payroll_documents SET sent_at = NULL", [])
            .unwrap();
        std::fs::write(dir.path().join("p60.pdf"), b"%PDF-1.4 changed").unwrap();
        assert!(
            select_unsent_payslip_documents(&repository, identity(), &missing)
                .unwrap_err()
                .to_string()
                .contains("changed")
        );
        assert!(repository
            .register_document(1, "p30", &dir.path().join("p60.pdf"), None)
            .is_err());
    }

    #[test]
    fn successful_send_transitions_from_durable_protection_to_sent() {
        let (_directory, repository) = repository();
        send_production_payslip(&repository, identity(), "2026-09-01T10:00:00Z", || {
            assert!(matches!(
                state(&repository),
                EmailDeliveryState::Indeterminate { .. }
            ));
            Ok(())
        })
        .unwrap();
        assert!(matches!(
            state(&repository),
            EmailDeliveryState::Sent { .. }
        ));
    }

    #[test]
    fn smtp_failure_restores_unsent_and_allows_retry() {
        let (_directory, repository) = repository();
        let error =
            send_production_payslip(&repository, identity(), "2026-09-01T10:00:00Z", || {
                Err("simulated SMTP refusal".into())
            })
            .unwrap_err();
        assert!(matches!(error, PayslipDeliveryError::Transport(_)));
        assert_eq!(state(&repository), EmailDeliveryState::Unsent);
        send_production_payslip(&repository, identity(), "2026-09-01T10:05:00Z", || Ok(()))
            .unwrap();
    }

    #[test]
    fn status_write_failure_after_smtp_leaves_restart_safe_indeterminate_state() {
        let (directory, repository) = repository();
        let database = directory.path().join("delivery.sqlite");
        let error =
            send_production_payslip(&repository, identity(), "2026-09-01T10:00:00Z", || {
                Connection::open(&database).unwrap().execute_batch(
                    "CREATE TRIGGER refuse_payslip_sent
                     BEFORE UPDATE OF sent_at ON payroll_timesheet_email_status
                     WHEN OLD.sent_at LIKE 'indeterminate:%'
                      AND NEW.sent_at NOT LIKE 'indeterminate:%'
                     BEGIN
                       SELECT RAISE(FAIL, 'simulated final status failure');
                     END;",
                )?;
                Ok(())
            })
            .unwrap_err();
        assert!(matches!(error, PayslipDeliveryError::Indeterminate(_)));
        assert!(error.to_string().contains("Delivery may have occurred"));
        drop(repository);

        let reloaded = PayrollTimesheetEmailRepository::new(Connection::open(database).unwrap());
        assert!(matches!(
            state(&reloaded),
            EmailDeliveryState::Indeterminate { .. }
        ));
        let mut send_called = false;
        let retry = send_production_payslip(&reloaded, identity(), "2026-09-01T11:00:00Z", || {
            send_called = true;
            Ok(())
        })
        .unwrap_err();
        assert!(matches!(retry, PayslipDeliveryError::Refused(_)));
        assert!(!send_called);
    }

    #[test]
    fn restart_after_pre_send_protection_refuses_unknown_attempt_without_smtp() {
        let (directory, repository) = repository();
        let database = directory.path().join("delivery.sqlite");
        assert!(repository
            .protect_payslip_for_send(1, "2026/27", 6, "2026-09-01T10:00:00Z")
            .unwrap());
        drop(repository);

        let reloaded = PayrollTimesheetEmailRepository::new(Connection::open(database).unwrap());
        let mut send_called = false;
        let error = send_production_payslip(&reloaded, identity(), "2026-09-01T11:00:00Z", || {
            send_called = true;
            Ok(())
        })
        .unwrap_err();
        assert!(matches!(error, PayslipDeliveryError::Refused(_)));
        assert!(!send_called);
        assert!(matches!(
            state(&reloaded),
            EmailDeliveryState::Indeterminate { .. }
        ));
    }

    #[test]
    fn partial_batch_preserves_sent_progress_and_is_not_complete() {
        let (_directory, repository) = repository();
        send_production_payslip(&repository, identity(), "2026-09-01T10:00:00Z", || Ok(()))
            .unwrap();
        let second = PayslipDeliveryIdentity {
            personal_assistant_id: 2,
            payroll_year: "2026/27",
            cycle_number: 6,
        };
        assert!(
            send_production_payslip(&repository, second, "2026-09-01T10:01:00Z", || Err(
                "second PA SMTP failure".into()
            ))
            .is_err()
        );
        assert!(matches!(
            state(&repository),
            EmailDeliveryState::Sent { .. }
        ));
        assert!(!all_payslips_definitively_sent(
            &repository,
            &[
                identity(),
                PayslipDeliveryIdentity {
                    personal_assistant_id: 2,
                    payroll_year: "2026/27",
                    cycle_number: 6,
                },
            ]
        )
        .unwrap());
    }

    #[test]
    fn schedule_is_marked_only_after_every_required_pa_is_definitively_sent() {
        let (directory, repository) = repository();
        let database = directory.path().join("delivery.sqlite");
        let schedule_connection = Connection::open(&database).unwrap();
        schedule_connection
            .execute(
                "INSERT INTO payroll_schedules (
                    payroll_year, cycle_number, first_week_commencing,
                    latest_posting_date, pay_date, created_at, payslips_sent
                 ) VALUES ('2026/27', 6, '10/08/2026', '31/08/2026',
                           '04/09/2026', '2026-01-01T00:00:00Z', 0)",
                [],
            )
            .unwrap();
        let schedule_id = schedule_connection.last_insert_rowid();
        let schedule_repository = PayrollScheduleRepository::new(schedule_connection);
        let identities = [
            identity(),
            PayslipDeliveryIdentity {
                personal_assistant_id: 2,
                payroll_year: "2026/27",
                cycle_number: 6,
            },
        ];

        send_production_payslip(&repository, identity(), "2026-09-01T10:00:00Z", || Ok(()))
            .unwrap();
        assert!(!mark_schedule_sent_if_complete(
            &repository,
            &schedule_repository,
            schedule_id,
            &identities,
        )
        .unwrap());
        assert!(!schedule_repository.get_all_for_year("2026/27").unwrap()[0].payslips_sent);

        send_production_payslip(
            &repository,
            PayslipDeliveryIdentity {
                personal_assistant_id: 2,
                payroll_year: "2026/27",
                cycle_number: 6,
            },
            "2026-09-01T10:01:00Z",
            || Ok(()),
        )
        .unwrap();
        assert!(mark_schedule_sent_if_complete(
            &repository,
            &schedule_repository,
            schedule_id,
            &identities,
        )
        .unwrap());
        assert!(schedule_repository.get_all_for_year("2026/27").unwrap()[0].payslips_sent);
    }

    #[test]
    fn preview_and_test_preview_do_not_create_delivery_state() {
        let (directory, repository) = repository();
        let path = directory.path().join("payslip.pdf");
        std::fs::write(&path, b"%PDF-1.4\n").unwrap();
        let schedule = "Week 22";
        preview_payroll_email(
            "payroll@example.com",
            "employer@example.com",
            Some("pa@example.com"),
            "Test PA",
            None,
            None,
            schedule,
            &path,
            "Body",
            None,
            None,
            "Payslip {Personal Assistant Name}",
        )
        .unwrap();
        preview_test_payslip_email(
            "employer@example.com",
            "test@example.com",
            "Test PA",
            None,
            None,
            schedule,
            &path,
            "Body",
            None,
            None,
            "Payslip {Personal Assistant Name}",
        )
        .unwrap();
        assert!(repository
            .get_for_pa_and_cycle(1, "2026/27", 6, "payslip")
            .unwrap()
            .is_none());
    }
}
