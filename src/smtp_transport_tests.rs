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
    worker: std::thread::JoinHandle<()>,
}
impl Fixture {
    fn new(mode: SmtpSecurity, advertise_tls: bool, reject_auth: bool, lose_ack: bool) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let commands = Arc::new(Mutex::new(Vec::new()));
        let captured = commands.clone();
        let identity = native_tls::Identity::from_pkcs12(
            include_bytes!("../tests/fixtures/smtp/server.p12"),
            "synthetic-test-only",
        )
        .unwrap();
        let acceptor = native_tls::TlsAcceptor::new(identity).unwrap();
        let worker = std::thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            socket
                .set_write_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut encrypted = mode == SmtpSecurity::ImplicitTls;
            let mut wire: Box<dyn Wire> = if encrypted {
                match acceptor.accept(socket) {
                    Ok(stream) => Box::new(stream),
                    Err(_) => return,
                }
            } else {
                Box::new(socket)
            };
            if wire.write_all(b"220 synthetic SMTP\r\n").is_err() {
                return;
            }
            let mut in_data = false;
            loop {
                let mut line = Vec::new();
                loop {
                    let mut byte = [0];
                    if !matches!(wire.read(&mut byte), Ok(1)) {
                        return;
                    }
                    line.push(byte[0]);
                    if byte[0] == b'\n' {
                        break;
                    }
                }
                let line = String::from_utf8(line).unwrap();
                captured.lock().unwrap().push((encrypted, line.clone()));
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
                    wire.write_all(b"220 upgrade\r\n").unwrap();
                    wire = match acceptor.accept(wire) {
                        Ok(stream) => Box::new(stream),
                        Err(_) => return,
                    };
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
                        return;
                    }
                    in_data = false;
                    b"250 queued\r\n"
                } else {
                    b"250 ok\r\n"
                };
                if wire.write_all(response).is_err() {
                    return;
                }
            }
        });
        Self {
            port,
            commands,
            worker,
        }
    }
    fn finish(self) -> Vec<(bool, String)> {
        self.worker.join().unwrap();
        self.commands.lock().unwrap().clone()
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
fn trusted_starttls_and_implicit_tls_encrypt_authentication_and_message() {
    for mode in [SmtpSecurity::RequiredStarttls, SmtpSecurity::ImplicitTls] {
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
    fixture.finish();
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
