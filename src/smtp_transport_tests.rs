//! Loopback-only synthetic SMTP servers. No production addresses or files.
use super::*;
use crate::config::{EmailConfig, SmtpSecurity};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

trait Wire: Read + Write {}
impl<T: Read + Write> Wire for T {}

struct Fixture {
    port: u16,
    commands: Arc<Mutex<Vec<(bool, String)>>>,
    worker: Option<std::thread::JoinHandle<()>>,
    events: Arc<Mutex<Vec<String>>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
}
impl Fixture {
    fn new(mode: SmtpSecurity, advertise_tls: bool, reject_auth: bool, lose_ack: bool) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let commands = Arc::new(Mutex::new(Vec::new()));
        let captured = commands.clone();
        let events = Arc::new(Mutex::new(Vec::new()));
        let trace = events.clone();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopping = stop.clone();
        listener.set_nonblocking(true).unwrap();
        let identity = native_tls::Identity::from_pkcs12(
            include_bytes!("../tests/fixtures/smtp/server.p12"),
            "synthetic-test-only",
        )
        .unwrap();
        let acceptor = native_tls::TlsAcceptor::new(identity).unwrap();
        let worker = std::thread::spawn(move || {
            let socket = loop {
                if stopping.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(10))
                    }
                    Err(e) => {
                        trace.lock().unwrap().push(format!("accept: {e}"));
                        return;
                    }
                }
            };
            // Accepted sockets must be blocking regardless of listener flag inheritance.
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(15)))
                .unwrap();
            socket
                .set_write_timeout(Some(std::time::Duration::from_secs(15)))
                .unwrap();
            let mut encrypted = mode == SmtpSecurity::ImplicitTls;
            let mut wire: Box<dyn Wire> = if encrypted {
                trace
                    .lock()
                    .unwrap()
                    .push("implicit TLS server handshake".into());
                match acceptor.accept(socket) {
                    Ok(stream) => {
                        trace
                            .lock()
                            .unwrap()
                            .push("TLS server handshake complete".into());
                        Box::new(stream)
                    }
                    Err(e) => {
                        trace.lock().unwrap().push(handshake_detail(e));
                        return;
                    }
                }
            } else {
                Box::new(socket)
            };
            trace.lock().unwrap().push("greeting".into());
            if let Err(e) = wire
                .write_all(b"220 synthetic SMTP\r\n")
                .and_then(|_| wire.flush())
            {
                trace.lock().unwrap().push(format!("greeting write: {e}"));
                return;
            }
            let mut in_data = false;
            loop {
                let mut line = Vec::new();
                loop {
                    let mut byte = [0];
                    match wire.read(&mut byte) {
                        Ok(1) => {}
                        Ok(_) => {
                            trace.lock().unwrap().push("peer EOF".into());
                            return;
                        }
                        Err(e) => {
                            trace.lock().unwrap().push(format!("read: {e}"));
                            return;
                        }
                    }
                    line.push(byte[0]);
                    if byte[0] == b'\n' {
                        break;
                    }
                }
                let line = String::from_utf8(line).unwrap();
                if !in_data || line == ".\r\n" {
                    captured
                        .lock()
                        .unwrap()
                        .push((encrypted, command_phase(&line).into()));
                }
                if in_data && line != ".\r\n" {
                    continue;
                }
                let response: &[u8] = if line.starts_with("EHLO") {
                    if advertise_tls && !encrypted {
                        b"250-synthetic\r\n250-STARTTLS\r\n250 AUTH PLAIN LOGIN\r\n"
                    } else {
                        b"250-synthetic\r\n250 AUTH PLAIN LOGIN\r\n"
                    }
                } else if line.starts_with("STARTTLS") {
                    trace
                        .lock()
                        .unwrap()
                        .push("STARTTLS server handshake".into());
                    if let Err(e) = wire
                        .write_all(b"220 upgrade\r\n")
                        .and_then(|_| wire.flush())
                    {
                        trace
                            .lock()
                            .unwrap()
                            .push(format!("STARTTLS response: {e}"));
                        return;
                    }
                    wire = match acceptor.accept(wire) {
                        Ok(stream) => Box::new(stream),
                        Err(e) => {
                            trace.lock().unwrap().push(handshake_detail(e));
                            return;
                        }
                    };
                    trace
                        .lock()
                        .unwrap()
                        .push("TLS server handshake complete".into());
                    encrypted = true;
                    continue;
                } else if line.starts_with("AUTH") {
                    if reject_auth {
                        b"535 password=SECRET pa@example.test rejected\r\n"
                    } else {
                        b"235 authenticated\r\n"
                    }
                } else if line.starts_with("DATA") {
                    in_data = true;
                    b"354 content\r\n"
                } else if line == ".\r\n" {
                    if lose_ack {
                        trace
                            .lock()
                            .unwrap()
                            .push("DATA received; acknowledgement deliberately withheld".into());
                        return;
                    }
                    in_data = false;
                    b"250 queued\r\n"
                } else {
                    b"250 ok\r\n"
                };
                trace.lock().unwrap().push(format!(
                    "response status {}",
                    std::str::from_utf8(&response[..3]).unwrap()
                ));
                if let Err(e) = wire.write_all(response).and_then(|_| wire.flush()) {
                    trace.lock().unwrap().push(format!("response write: {e}"));
                    return;
                }
            }
        });
        Self {
            port,
            commands,
            worker: Some(worker),
            events,
            stop,
        }
    }
    fn join(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let outcome = if worker.join().is_ok() {
                "worker completed"
            } else {
                "worker panicked"
            };
            self.events.lock().unwrap().push(outcome.into());
        }
    }
    fn finish(mut self) -> Vec<(bool, String)> {
        self.join();
        assert!(
            !self
                .events
                .lock()
                .unwrap()
                .iter()
                .any(|event| event == "worker panicked"),
            "SMTP fixture worker panicked"
        );
        self.commands.lock().unwrap().clone()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.join();
        eprintln!(
            "SMTP fixture: events={:?}; command phases={:?}",
            self.events.lock().unwrap(),
            self.commands.lock().unwrap()
        );
    }
}
// Never retain AUTH payloads, addresses, headers or DATA contents.
fn command_phase(line: &str) -> &'static str {
    for phase in ["EHLO", "STARTTLS", "AUTH", "MAIL", "RCPT", "DATA", "QUIT"] {
        if line.starts_with(phase) {
            return phase;
        }
    }
    if line == ".\r\n" {
        "DATA terminator"
    } else {
        "other command [redacted]"
    }
}

fn config(port: u16, mode: SmtpSecurity) -> EmailConfig {
    let mut config = EmailConfig::default();
    config.smtp_transport = "SMTP Server".into();
    config.smtp_host = "localhost".into();
    config.smtp_port = port;
    config.smtp_security = Some(mode);
    config.smtp_username = "synthetic-user".into();
    config.smtp_password = "SECRET".into();
    config
}
fn message() -> Message {
    Message::builder()
        .from("from@example.test".parse().unwrap())
        .to("to@example.test".parse().unwrap())
        .subject("synthetic")
        .body("Synthetic attachment-free content".to_string())
        .unwrap()
}
fn trusted(domain: &str) -> TlsParameters {
    let cert = lettre::transport::smtp::client::Certificate::from_pem(include_bytes!(
        "../tests/fixtures/smtp/ca.pem"
    ))
    .unwrap();
    TlsParameters::builder(domain.into())
        .add_root_certificate(cert)
        .build()
        .unwrap()
}

#[test]
fn trusted_starttls_encrypts_authentication_and_message() {
    trusted_transport(SmtpSecurity::RequiredStarttls);
}
#[test]
fn trusted_implicit_tls_encrypts_authentication_and_message() {
    trusted_transport(SmtpSecurity::ImplicitTls);
}
fn trusted_transport(mode: SmtpSecurity) {
    let fixture = Fixture::new(mode, true, false, false);
    dispatch_message(
        &config(fixture.port, mode),
        &message(),
        Some(trusted("localhost")),
    )
    .unwrap();
    let commands = fixture.finish();
    assert!(commands
        .iter()
        .any(|(encrypted, line)| *encrypted && line.starts_with("AUTH")));
    assert!(commands
        .iter()
        .any(|(encrypted, line)| *encrypted && line.starts_with("MAIL")));
    assert!(!commands.iter().any(|(encrypted, line)| !encrypted
        && (line.starts_with("AUTH") || line.starts_with("MAIL") || line.starts_with("DATA"))));
}
#[test]
fn unauthenticated_remote_relay_keeps_starttls_and_implicit_tls() {
    for mode in [SmtpSecurity::RequiredStarttls, SmtpSecurity::ImplicitTls] {
        let fixture=Fixture::new(mode,true,true,false);
        let mut config=config(fixture.port,mode);
        config.smtp_username.clear();
        config.smtp_password.clear();
        config.test_sender_address="sender@example.invalid".into();
        config.payroll_test_email_address="recipient@example.invalid".into();
        assert!(crate::email_settings::validate_standalone_test(&config,Some(crate::email_settings::TestRecipient::Payroll)).is_ok());
        let message=prepare_smtp_test_message(&config,crate::email_settings::TestRecipient::Payroll).unwrap();
        dispatch_message(&config,&message,Some(trusted("localhost"))).unwrap();
        let commands=fixture.finish();
        assert!(!commands.iter().any(|(_,line)|line.starts_with("AUTH")));
        assert!(commands.iter().any(|(encrypted,line)|*encrypted && line.starts_with("MAIL")));
        assert!(!commands.iter().any(|(encrypted,line)|!*encrypted && (line.starts_with("MAIL")||line.starts_with("DATA"))));
    }
}
#[test]
fn untrusted_and_wrong_hostname_fail_without_plaintext_fallback() {
    for mode in [SmtpSecurity::RequiredStarttls, SmtpSecurity::ImplicitTls] {
        for wrong_hostname in [false, true] {
            let fixture = Fixture::new(mode, true, false, false);
            let tls = wrong_hostname.then(|| trusted("wrong.example.test"));
            let error = dispatch_message(&config(fixture.port, mode), &message(), tls).unwrap_err();
            assert!(matches!(error, ProductionSendError::NotSent(_)));
            assert!(!fixture
                .finish()
                .iter()
                .any(|(_, line)| line.starts_with("AUTH") || line.starts_with("MAIL")));
        }
    }
}
#[test]
fn unavailable_starttls_sends_no_credentials_or_mail() {
    let fixture = Fixture::new(SmtpSecurity::Plaintext, false, false, false);
    let error = dispatch_message(
        &config(fixture.port, SmtpSecurity::RequiredStarttls),
        &message(),
        Some(trusted("localhost")),
    )
    .unwrap_err();
    assert!(matches!(error, ProductionSendError::NotSent(_)));
    assert!(!fixture
        .finish()
        .iter()
        .any(|(_, line)| line.starts_with("AUTH") || line.starts_with("MAIL")));
}
#[test]
fn authentication_rejection_is_not_sent_and_server_details_are_redacted() {
    let fixture = Fixture::new(SmtpSecurity::RequiredStarttls, true, true, false);
    let error = dispatch_message(
        &config(fixture.port, SmtpSecurity::RequiredStarttls),
        &message(),
        Some(trusted("localhost")),
    )
    .unwrap_err();
    assert!(matches!(error, ProductionSendError::NotSent(_)));
    let diagnostic = error.to_string();
    assert!(diagnostic.contains("authentication") && diagnostic.contains("535"));
    assert!(!diagnostic.contains("SECRET") && !diagnostic.contains("pa@example.test"));
    assert!(!fixture
        .finish()
        .iter()
        .any(|(_, line)| line.starts_with("MAIL")));
}
#[test]
fn secure_lost_acceptance_acknowledgement_remains_uncertain() {
    let fixture = Fixture::new(SmtpSecurity::RequiredStarttls, true, false, true);
    assert!(matches!(
        dispatch_message(
            &config(fixture.port, SmtpSecurity::RequiredStarttls),
            &message(),
            Some(trusted("localhost"))
        ),
        Err(ProductionSendError::Uncertain(_))
    ));
    let commands = fixture.finish();
    assert!(commands
        .iter()
        .any(|(encrypted, phase)| *encrypted && phase == "DATA terminator"));
}
#[test]
fn legacy_plaintext_and_explicit_plaintext_remain_compatible() {
    for legacy in [false, true] {
        let fixture = Fixture::new(SmtpSecurity::Plaintext, false, false, false);
        let mut config = config(fixture.port, SmtpSecurity::Plaintext);
        if legacy {
            config.smtp_security = None;
        }
        send_prepared_timesheet(&config, &message()).unwrap();
        assert!(fixture
            .finish()
            .iter()
            .any(|(encrypted, line)| !encrypted && line.starts_with("MAIL")));
    }
}
#[test]
fn security_serialization_defaults_unknown_values_and_debug_redaction() {
    let config = config(587, SmtpSecurity::RequiredStarttls);
    let serialized = toml::to_string(&config).unwrap();
    assert!(serialized.contains("required_starttls"));
    let legacy = serialized
        .lines()
        .filter(|line| !line.starts_with("smtp_security"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        toml::from_str::<EmailConfig>(&legacy)
            .unwrap()
            .smtp_security,
        None
    );
    assert!(toml::from_str::<EmailConfig>(
        &serialized.replace("required_starttls", "unknown_mode")
    )
    .is_err());
    assert_eq!(
        EmailConfig::default().smtp_security,
        Some(SmtpSecurity::RequiredStarttls)
    );
    let debug = format!("{config:?}");
    assert!(!debug.contains("SECRET") && !debug.contains("synthetic-user"));
}
#[test]
fn unsupported_transport_is_rejected_before_connection() {
    let mut config = config(1, SmtpSecurity::Plaintext);
    config.smtp_transport = "unknown".into();
    assert!(matches!(
        send_prepared_timesheet(&config, &message()),
        Err(ProductionSendError::NotSent(_))
    ));
}

#[test]
fn first_remote_setup_defaults_secure_without_changing_legacy_remote_or_explicit_modes() {
    let mut config = EmailConfig::default();
    config.smtp_security = None;
    config.select_transport("SMTP Server".into());
    assert_eq!(config.smtp_security, Some(SmtpSecurity::RequiredStarttls));
    config.smtp_security = None;
    config.select_transport("SMTP Server".into());
    assert_eq!(config.smtp_security, None);
    config.select_transport("Local SMTP Server".into());
    config.smtp_security = Some(SmtpSecurity::Plaintext);
    config.select_transport("SMTP Server".into());
    assert_eq!(config.smtp_security, Some(SmtpSecurity::Plaintext));
}

#[test]
fn fixture_transcript_redacts_credentials_addresses_and_message_contents() {
    for line in [
        "AUTH PLAIN U0VDUkVU",
        "MAIL FROM:<private@example.test>",
        "RCPT TO:<private@example.test>",
        "Secret message body",
    ] {
        let phase = command_phase(line);
        assert!(
            !phase.contains("U0VDUkVU") && !phase.contains("private") && !phase.contains("Secret")
        );
    }
}
#[test]
fn fixture_worker_joins_without_a_client() {
    let fixture = Fixture::new(SmtpSecurity::Plaintext, false, false, false);
    assert!(fixture.finish().is_empty());
}

fn handshake_detail<S>(error: native_tls::HandshakeError<S>) -> String {
    match error {
        native_tls::HandshakeError::Failure(error) => {
            format!("TLS server handshake failure: {error:?}")
        }
        native_tls::HandshakeError::WouldBlock(_) => {
            "TLS server handshake interrupted (WouldBlock)".into()
        }
    }
}

#[test]
fn standalone_async_test_uses_empty_database_and_writes_no_delivery_history() {
    use crate::email_settings::{TestEmailTask, TestRecipient};
    let (_directory, application) = crate::payroll_timesheet_screen::tests::test_application();
    let db = &application.payroll_timesheet_email_repository.connection;
    let snapshot = || -> Vec<i64> {
        [
            "personal_assistants",
            "employers",
            "payroll_schedules",
            "payroll_timesheet_email_status",
            "imported_payroll_documents",
        ]
        .iter()
        .map(|table| {
            db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
        })
        .collect()
    };
    let before = snapshot();
    assert!(before.iter().all(|count| *count == 0));
    let fixture = Fixture::new(SmtpSecurity::Plaintext, false, false, false);
    let mut config = config(fixture.port, SmtpSecurity::Plaintext);
    config.test_sender_address = "from@example.test".into();
    config.payroll_test_email_address = "to@example.test".into();
    let (repaint_tx, repaint_rx) = std::sync::mpsc::channel();
    let mut task = TestEmailTask::default();
    task.start(config, TestRecipient::Payroll, move || {
        let _ = repaint_tx.send(());
    })
    .unwrap();
    repaint_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    assert!(task.poll().unwrap().is_ok());
    assert!(!task.pending());
    assert_eq!(before, snapshot());
    let commands = fixture.finish();
    assert_eq!(
        commands
            .iter()
            .filter(|(_, line)| line.starts_with("RCPT"))
            .count(),
        1
    );
    assert!(commands.iter().any(|(_, line)| line.starts_with("DATA")));
}

#[test]
fn standalone_message_reuses_required_tls_authentication_and_custom_ports() {
    for mode in [SmtpSecurity::RequiredStarttls, SmtpSecurity::ImplicitTls] {
        let fixture = Fixture::new(mode, true, false, false);
        let mut config = config(fixture.port, mode);
        config.test_sender_address = "from@example.test".into();
        config.pa_test_email_address = "to@example.test".into();
        let message =
            prepare_smtp_test_message(&config, crate::email_settings::TestRecipient::Pa).unwrap();
        dispatch_message(&config, &message, Some(trusted("localhost"))).unwrap();
        let commands = fixture.finish();
        assert!(commands
            .iter()
            .any(|(encrypted, line)| *encrypted && line.starts_with("AUTH")));
        assert_eq!(
            commands
                .iter()
                .filter(|(_, line)| line.starts_with("RCPT"))
                .count(),
            1
        );
        assert!(!commands.iter().any(|(encrypted, line)| !encrypted
            && (line.starts_with("AUTH") || line.starts_with("MAIL") || line.starts_with("DATA"))));
    }
}
