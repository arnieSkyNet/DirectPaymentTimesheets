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

pub fn send_production_payslip<F>(
    repository: &PayrollTimesheetEmailRepository,
    identity: PayslipDeliveryIdentity<'_>,
    attempted_at: &str,
    send: F,
) -> Result<(), PayslipDeliveryError>
where
    F: FnOnce() -> Result<(), Box<dyn std::error::Error>>,
{
    if let Some(status) = repository
        .get_for_pa_and_cycle(
            identity.personal_assistant_id,
            identity.payroll_year,
            identity.cycle_number,
            "payslip",
        )
        .map_err(operation_error)?
    {
        match status.delivery_state {
            EmailDeliveryState::Sent { .. } => {
                return Err(PayslipDeliveryError::Refused(
                    "This payslip is already recorded as sent and will not be sent again."
                        .to_string(),
                ));
            }
            EmailDeliveryState::Indeterminate { attempted_at } => {
                return Err(PayslipDeliveryError::Refused(format!(
                    "This payslip has an indeterminate production delivery attempt recorded at {attempted_at}. Delivery may already have occurred, so automatic resend is unsafe."
                )));
            }
            EmailDeliveryState::Unsent => {}
        }
    }

    if !repository
        .protect_payslip_for_send(
            identity.personal_assistant_id,
            identity.payroll_year,
            identity.cycle_number,
            attempted_at,
        )
        .map_err(operation_error)?
    {
        return Err(PayslipDeliveryError::Refused(
            "The payslip could not be protected with a durable indeterminate delivery state before SMTP; nothing was sent."
                .to_string(),
        ));
    }

    if let Err(error) = send() {
        match repository.restore_unsent_after_failed_payslip_send(
            identity.personal_assistant_id,
            identity.payroll_year,
            identity.cycle_number,
            attempted_at,
        ) {
            Ok(true) => {
                return Err(PayslipDeliveryError::Transport(format!(
                    "Payslip SMTP failed before successful delivery was confirmed: {error}"
                )));
            }
            Ok(false) => {
                return Err(PayslipDeliveryError::Indeterminate(format!(
                    "SMTP reported an error ({error}), but the protected delivery state could not be restored because it had changed unexpectedly."
                )));
            }
            Err(restore_error) => {
                return Err(PayslipDeliveryError::Indeterminate(format!(
                    "SMTP reported an error ({error}), and the protected delivery state could not be restored: {restore_error}."
                )));
            }
        }
    }

    let sent_at = chrono::Local::now().to_rfc3339();
    match repository.mark_payslip_sent_from_indeterminate(
        identity.personal_assistant_id,
        identity.payroll_year,
        identity.cycle_number,
        attempted_at,
        &sent_at,
    ) {
        Ok(true) => Ok(()),
        Ok(false) => Err(PayslipDeliveryError::Indeterminate(
            "SMTP completed, but the protected delivery record changed before definitive sent state could be stored."
                .to_string(),
        )),
        Err(error) => Err(PayslipDeliveryError::Indeterminate(format!(
            "SMTP completed, but definitive sent state could not be persisted: {error}."
        ))),
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
