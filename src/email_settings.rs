//! Standalone setup actions deliberately have no payroll/database dependency.
use crate::{config::EmailConfig, email_service::ProductionSendError};
use std::sync::mpsc::{self, Receiver, TryRecvError};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum TestRecipient {
    #[default]
    Payroll,
    Pa,
}

#[derive(Default)]
pub(crate) struct TestEmailTask {
    pending: Option<Receiver<Result<(), ProductionSendError>>>,
    started: Option<std::time::Instant>,
    stalled_reported: bool,
}
// A watchdog reports uncertainty; it cannot safely cancel a Rust thread or
// prove non-acceptance. Keep duplicate/close protection until termination.
const TEST_SEND_WATCHDOG: std::time::Duration = std::time::Duration::from_secs(120);
struct WorkerCompletion<F: FnOnce()> {
    sender: Option<mpsc::Sender<Result<(), ProductionSendError>>>,
    repaint: Option<F>,
}
impl<F: FnOnce()> WorkerCompletion<F> {
    fn finish(&mut self, result: Result<(), ProductionSendError>) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(result);
        }
    }
}
impl<F: FnOnce()> Drop for WorkerCompletion<F> {
    fn drop(&mut self) {
        // Drop/send the channel before waking the main viewport, even on unwind.
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(Err(ProductionSendError::Uncertain(
                "SMTP test worker terminated unexpectedly; acceptance is unknown.".into(),
            )));
        }
        if let Some(repaint) = self.repaint.take() {
            repaint();
        }
    }
}
impl TestEmailTask {
    pub fn pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn stalled(&self) -> bool {
        self.pending() && self.stalled_reported
    }
    #[cfg(test)]
    pub(crate) fn start_mock(
        &mut self,
        send: impl FnOnce() -> Result<(), ProductionSendError> + Send + 'static,
        repaint: impl FnOnce() + Send + 'static,
    ) -> Result<(), ProductionSendError> {
        self.start_with(send, repaint)
    }
    #[cfg(test)]
    pub(crate) fn report_stall_for_test(&mut self) -> Option<Result<(), ProductionSendError>> {
        self.poll_at(self.started.unwrap() + TEST_SEND_WATCHDOG)
    }
    pub fn start(
        &mut self,
        config: EmailConfig,
        recipient: TestRecipient,
        repaint: impl FnOnce() + Send + 'static,
    ) -> Result<(), ProductionSendError> {
        if self.pending() {
            return Err(ProductionSendError::NotSent(
                "A test email is already in progress.".into(),
            ));
        }
        let message = crate::email_service::prepare_smtp_test_message(&config, recipient)?;
        self.start_with(
            move || crate::email_service::send_prepared_timesheet(&config, &message),
            repaint,
        )
    }
    fn start_with(
        &mut self,
        send: impl FnOnce() -> Result<(), ProductionSendError> + Send + 'static,
        repaint: impl FnOnce() + Send + 'static,
    ) -> Result<(), ProductionSendError> {
        if self.pending() {
            return Err(ProductionSendError::NotSent(
                "A test email is already in progress.".into(),
            ));
        }
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("smtp-test".into())
            .spawn(move || {
                let mut completion = WorkerCompletion {
                    sender: Some(tx),
                    repaint: Some(repaint),
                };
                let result = send();
                completion.finish(result);
            })
            .map_err(|_| {
                ProductionSendError::NotSent("Unable to start SMTP test worker.".into())
            })?;
        self.pending = Some(rx);
        self.started = Some(std::time::Instant::now());
        self.stalled_reported = false;
        Ok(())
    }
    pub fn poll(&mut self) -> Option<Result<(), ProductionSendError>> {
        self.poll_at(std::time::Instant::now())
    }
    fn poll_at(&mut self, now: std::time::Instant) -> Option<Result<(), ProductionSendError>> {
        let result = match self.pending.as_ref()?.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => {
                if !self.stalled_reported && self.started.is_some_and(|start| now.saturating_duration_since(start) >= TEST_SEND_WATCHDOG) {
                    self.stalled_reported = true;
                    return Some(Err(ProductionSendError::Uncertain("SMTP test exceeded its 120-second UI watchdog. Worker is still active; acceptance is unknown. Duplicate sends and normal closure remain blocked until it terminates.".into())));
                }
                None
            }
            Err(TryRecvError::Disconnected) => Some(Err(ProductionSendError::Uncertain("Test worker stopped; SMTP acceptance is unknown. Do not assume the email was not sent.".into()))),
        };
        if result.is_some() {
            self.pending = None;
            self.started = None;
            self.stalled_reported = false;
        }
        result
    }
}

pub(crate) fn example_preview(
    config: &crate::config::AppConfig,
    signature: Option<&str>,
) -> String {
    let defaults = crate::config::AppConfig::default();
    let body = |draft: &str, fallback: &str| {
        if draft.trim().is_empty() {
            fallback.to_string()
        } else {
            draft.to_string()
        }
    };
    let subject = body(
        &config.payroll.email_subject_format,
        &defaults.payroll.email_subject_format,
    )
    .replace("{Personal Assistant Name}", "Example Assistant")
    .replace("{Personal Assistant DOB}", "Example DOB")
    .replace("{Personal Assistant NI}", "Example NI")
    .replace("{YYYYMMwWW}", "EXAMPLE-PERIOD");
    let signature = signature
        .filter(|text| !text.trim().is_empty())
        .unwrap_or("Example Employer (fictional signature)");
    format!("EXAMPLE PREVIEW — fictional details; nothing is sent or read from attachments.\nCurrent draft templates/signature are used where available.\n\nFrom: Example Employer <employer@example.invalid>\nTo: Example Recipient <recipient@example.invalid>\nCC/BCC: None\n\nTimesheet subject: {subject}\nTimesheet attachment: Example Timesheet.pdf (fictional, not opened)\n\n{}\n\n{}\n\nPayslip subject: Payslip for Week EXAMPLE\nPayslip attachment: Example Payslip.pdf (fictional, not opened)\n\n{}\n\n{}", body(&config.payroll.timesheet_email_body, &defaults.payroll.timesheet_email_body), signature, body(&config.payroll.payslip_email_body, &defaults.payroll.payslip_email_body), signature)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panicking_worker_notifies_repaint_and_reports_uncertainty() {
        let mut task = TestEmailTask::default();
        let (wake, notification) = mpsc::channel();
        task.start_with(
            || panic!("synthetic worker failure"),
            move || {
                wake.send(()).unwrap();
            },
        )
        .unwrap();
        notification
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        assert!(matches!(
            task.poll(),
            Some(Err(ProductionSendError::Uncertain(_)))
        ));
        assert!(!task.pending());
        assert!(task.poll().is_none());
    }
    #[test]
    fn stalled_worker_reports_once_stays_protected_and_can_later_accept() {
        let mut task = TestEmailTask::default();
        let (release, wait) = mpsc::channel();
        let (wake, notification) = mpsc::channel();
        task.start_with(
            move || {
                wait.recv().unwrap();
                Ok(())
            },
            move || {
                wake.send(()).unwrap();
            },
        )
        .unwrap();
        let late = task.started.unwrap() + TEST_SEND_WATCHDOG;
        assert!(matches!(
            task.poll_at(late),
            Some(Err(ProductionSendError::Uncertain(_)))
        ));
        assert!(task.stalled() && task.pending());
        assert!(task.poll_at(late).is_none());
        assert!(task
            .start_with(|| panic!("duplicate worker"), || {})
            .is_err());
        release.send(()).unwrap();
        notification
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        assert!(matches!(task.poll(), Some(Ok(()))));
        assert!(!task.pending() && !task.stalled());
    }
    #[test]
    fn standalone_message_has_only_selected_test_envelope() {
        let mut config = EmailConfig::default();
        config.smtp_username = "sender@example.invalid".into();
        config.payroll_test_email_address = "payroll-test@example.invalid".into();
        config.pa_test_email_address = "pa-test@example.invalid".into();
        for (target, address) in [
            (TestRecipient::Payroll, "payroll-test@example.invalid"),
            (TestRecipient::Pa, "pa-test@example.invalid"),
        ] {
            let message = crate::email_service::prepare_smtp_test_message(&config, target).unwrap();
            assert_eq!(message.envelope().to().len(), 1);
            assert_eq!(message.envelope().to()[0].to_string(), address);
            let mime = String::from_utf8(message.formatted()).unwrap();
            assert!(mime.contains("DirectPaymentTimesheets  SMTP Test"));
            assert!(
                !mime.contains("Cc:")
                    && !mime.contains("Bcc:")
                    && !mime.contains("Content-Disposition: attachment")
                    && !mime.contains("multipart/mixed")
            );
        }
        config.smtp_username = "non-email-login".into();
        assert!(
            crate::email_service::prepare_smtp_test_message(&config, TestRecipient::Payroll)
                .is_err()
        );
        config.test_sender_address = "explicit@example.invalid".into();
        assert!(
            crate::email_service::prepare_smtp_test_message(&config, TestRecipient::Payroll)
                .is_ok()
        );
        config.pa_test_email_address.clear();
        assert!(
            crate::email_service::prepare_smtp_test_message(&config, TestRecipient::Pa).is_err()
        );
    }
    #[test]
    fn invalid_sender_does_not_start_worker_and_valid_username_wins() {
        let mut task = TestEmailTask::default();
        let mut config = EmailConfig::default();
        config.payroll_test_email_address = "recipient@example.invalid".into();
        config.test_sender_address = "invalid sender".into();
        assert!(task
            .start(config.clone(), TestRecipient::Payroll, || panic!(
                "invalid test started"
            ))
            .is_err());
        assert!(!task.pending());
        config.smtp_username = "username@example.invalid".into();
        let message =
            crate::email_service::prepare_smtp_test_message(&config, TestRecipient::Payroll)
                .unwrap();
        assert_eq!(
            message.envelope().from().unwrap().to_string(),
            "username@example.invalid"
        );
        let encoded = toml::to_string(&config).unwrap();
        let decoded: EmailConfig = toml::from_str(&encoded).unwrap();
        assert_eq!(decoded.test_sender_address, "invalid sender");
        assert!(toml::from_str::<EmailConfig>("")
            .unwrap()
            .test_sender_address
            .is_empty());
    }
    #[test]
    fn lost_worker_is_uncertain_and_completion_is_consumed_once() {
        let mut task = TestEmailTask::default();
        let (sender, receiver) = mpsc::channel();
        task.pending = Some(receiver);
        drop(sender);
        assert!(matches!(
            task.poll(),
            Some(Err(ProductionSendError::Uncertain(_)))
        ));
        assert!(!task.pending());
        assert!(task.poll().is_none());
    }

    #[test]
    fn preview_requires_no_configuration_or_files() {
        let mut config = crate::config::AppConfig::default();
        let preview = example_preview(&config, None);
        assert!(preview.contains("example.invalid") && preview.contains("fictional"));
        config.payroll.timesheet_email_body = "Unsaved draft body".into();
        assert!(example_preview(&config, Some("Draft signature")).contains("Unsaved draft body"));
        assert!(example_preview(&config, Some("Draft signature")).contains("Draft signature"));
    }
    #[test]
    fn asynchronous_worker_prevents_duplicates_and_reports_uncertainty() {
        let mut task = TestEmailTask::default();
        let (release, wait) = mpsc::channel();
        task.start_with(
            move || {
                wait.recv().unwrap();
                Err(ProductionSendError::Uncertain(
                    "synthetic uncertain outcome".into(),
                ))
            },
            || {},
        )
        .unwrap();
        assert!(task.pending());
        assert!(task.poll().is_none());
        assert!(task
            .start_with(|| panic!("duplicate submitted"), || {})
            .is_err());
        release.send(()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if let Some(result) = task.poll() {
                assert!(matches!(result, Err(ProductionSendError::Uncertain(_))));
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(!task.pending());
    }
}
