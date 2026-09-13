use lettre::message::{header::ContentType, Attachment, Mailbox, Message, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::SmtpTransport;
use lettre::Transport;
use std::error::Error;
use std::fs;
use std::path::Path;

#[derive(Debug)]
pub struct PayrollEmailPreview {
    pub from: String,
    pub to: String,
    pub cc: Option<String>,
    pub bcc: Option<String>,
    pub subject: String,
    pub body: String,
    pub attachment_path: String,
    pub additional_attachment_paths: Vec<std::path::PathBuf>,
}

pub fn preview_payroll_email(
    payroll_department_email: &str,
    employer_email: &str,
    personal_assistant_email: Option<&str>,
    personal_assistant_name: &str,
    personal_assistant_dob: Option<&str>,
    personal_assistant_ni: Option<&str>,
    payroll_period: &str,
    attachment_path: &Path,
    email_body: &str,
    additional_note: Option<&str>,
    email_signature: Option<&str>,
    subject_template: &str,
) -> Result<PayrollEmailPreview, Box<dyn Error>> {
    compose_payroll_email(
        employer_email,
        payroll_department_email,
        Some(employer_email),
        personal_assistant_email,
        personal_assistant_name,
        personal_assistant_dob,
        personal_assistant_ni,
        payroll_period,
        attachment_path,
        email_body,
        additional_note,
        email_signature,
        subject_template,
        "",
        "",
        "Payroll Department has no email address.",
    )
}

pub fn preview_test_payroll_email(
    sender_email: &str,
    payroll_test_email: &str,
    pa_test_email: Option<&str>,
    personal_assistant_name: &str,
    personal_assistant_dob: Option<&str>,
    personal_assistant_ni: Option<&str>,
    payroll_period: &str,
    attachment_path: &Path,
    email_body: &str,
    additional_note: Option<&str>,
    email_signature: Option<&str>,
    subject_template: &str,
) -> Result<PayrollEmailPreview, Box<dyn Error>> {
    compose_payroll_email(
        sender_email,
        payroll_test_email,
        None,
        pa_test_email,
        personal_assistant_name,
        personal_assistant_dob,
        personal_assistant_ni,
        payroll_period,
        attachment_path,
        email_body,
        additional_note,
        email_signature,
        subject_template,
        "TEST: ",
        "TEST:\n\n",
        "Payroll test email address is required.",
    )
}

pub fn preview_test_payslip_email(
    sender_email: &str,
    pa_test_email: &str,
    personal_assistant_name: &str,
    personal_assistant_dob: Option<&str>,
    personal_assistant_ni: Option<&str>,
    payroll_period: &str,
    attachment_path: &Path,
    email_body: &str,
    additional_note: Option<&str>,
    email_signature: Option<&str>,
    subject_template: &str,
) -> Result<PayrollEmailPreview, Box<dyn Error>> {
    compose_payroll_email(
        sender_email,
        pa_test_email,
        None,
        None,
        personal_assistant_name,
        personal_assistant_dob,
        personal_assistant_ni,
        payroll_period,
        attachment_path,
        email_body,
        additional_note,
        email_signature,
        subject_template,
        "TEST: ",
        "TEST:\n\n",
        "PA test email address is required.",
    )
}

/// Wording follows the selected bundle, independently of the Dashboard period.
pub fn payroll_document_wording<'a>(
    bundle: &crate::payslip_delivery_service::PayslipEmailBundle,
    configured_subject: &'a str,
    configured_body: &'a str,
) -> (&'a str, &'a str) {
    if bundle.email_types.contains(&"payslip") {
        (configured_subject, configured_body)
    } else {
        (
            "Payroll documents - {Personal Assistant Name}",
            if bundle.paths.len() == 1 {
                "Please find attached your payroll document."
            } else {
                "Please find attached your payroll documents."
            },
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn compose_payroll_email(
    sender_email: &str,
    to_email: &str,
    cc_email: Option<&str>,
    bcc_email: Option<&str>,
    personal_assistant_name: &str,
    personal_assistant_dob: Option<&str>,
    personal_assistant_ni: Option<&str>,
    payroll_period: &str,
    attachment_path: &Path,
    email_body: &str,
    additional_note: Option<&str>,
    email_signature: Option<&str>,
    subject_template: &str,
    subject_prefix: &str,
    body_prefix: &str,
    missing_to_error: &str,
) -> Result<PayrollEmailPreview, Box<dyn Error>> {
    let sender_email = sender_email.trim();

    if sender_email.is_empty() {
        return Err("Employer has no email address.".into());
    }

    let to_email = to_email.trim();

    if to_email.is_empty() {
        return Err(missing_to_error.into());
    }

    let cc_email = cc_email.map(str::trim).filter(|email| !email.is_empty());
    let bcc_email = bcc_email.map(str::trim).filter(|email| !email.is_empty());

    let subject = format!(
        "{}{}",
        subject_prefix,
        subject_template
            .replace("{Personal Assistant Name}", personal_assistant_name)
            .replace(
                "{Personal Assistant DOB}",
                personal_assistant_dob.unwrap_or(""),
            )
            .replace(
                "{Personal Assistant NI}",
                personal_assistant_ni.unwrap_or(""),
            )
            .replace("{YYYYMMwWW}", &payroll_period)
    );

    let mut body = format!("{}{}", body_prefix, email_body.trim_end());

    if let Some(additional_note) = additional_note
        .map(str::trim)
        .filter(|note| !note.is_empty())
    {
        if !body.is_empty() {
            body.push_str("\n\n");
        }
        body.push_str(additional_note);
    }

    if let Some(signature) = email_signature {
        let signature = signature.trim();

        if !signature.is_empty() {
            if !body.is_empty() {
                body.push_str("\n\n");
            }

            body.push_str(signature);
        }
    }

    Ok(PayrollEmailPreview {
        from: sender_email.to_string(),
        to: to_email.to_string(),
        cc: cc_email.map(str::to_string),
        bcc: bcc_email.map(str::to_string),
        subject,
        body,
        attachment_path: attachment_path.display().to_string(),
        additional_attachment_paths: Vec::new(),
    })
}

#[allow(dead_code)] // Retain the existing single-attachment API.
pub fn send_payroll_email(
    smtp_host: &str,
    smtp_port: u16,
    smtp_username: &str,
    smtp_password: &str,
    payroll_department_email: &str,
    employer_email: &str,
    personal_assistant_email: Option<&str>,
    personal_assistant_name: &str,
    personal_assistant_dob: Option<&str>,
    personal_assistant_ni: Option<&str>,
    payroll_period: &str,
    attachment_path: &Path,
    email_body: &str,
    additional_note: Option<&str>,
    email_signature: Option<&str>,
    subject_template: &str,
) -> Result<(), Box<dyn Error>> {
    send_payroll_email_with_attachments(
        smtp_host,
        smtp_port,
        smtp_username,
        smtp_password,
        payroll_department_email,
        employer_email,
        personal_assistant_email,
        personal_assistant_name,
        personal_assistant_dob,
        personal_assistant_ni,
        payroll_period,
        attachment_path,
        email_body,
        additional_note,
        email_signature,
        subject_template,
        &[],
    )
}

pub fn send_payroll_email_with_attachments(
    smtp_host: &str,
    smtp_port: u16,
    smtp_username: &str,
    smtp_password: &str,
    payroll_department_email: &str,
    employer_email: &str,
    personal_assistant_email: Option<&str>,
    personal_assistant_name: &str,
    personal_assistant_dob: Option<&str>,
    personal_assistant_ni: Option<&str>,
    payroll_period: &str,
    attachment_path: &Path,
    email_body: &str,
    additional_note: Option<&str>,
    email_signature: Option<&str>,
    subject_template: &str,
    additional_attachments: &[std::path::PathBuf],
) -> Result<(), Box<dyn Error>> {
    let mut preview = preview_payroll_email(
        payroll_department_email,
        employer_email,
        personal_assistant_email,
        personal_assistant_name,
        personal_assistant_dob,
        personal_assistant_ni,
        payroll_period,
        attachment_path,
        email_body,
        additional_note,
        email_signature,
        subject_template,
    )?;

    preview.additional_attachment_paths = additional_attachments.to_vec();
    send_preview(
        smtp_host,
        smtp_port,
        smtp_username,
        smtp_password,
        preview,
        attachment_path,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn send_test_payroll_email(
    smtp_host: &str,
    smtp_port: u16,
    smtp_username: &str,
    smtp_password: &str,
    sender_email: &str,
    payroll_test_email: &str,
    pa_test_email: Option<&str>,
    personal_assistant_name: &str,
    personal_assistant_dob: Option<&str>,
    personal_assistant_ni: Option<&str>,
    payroll_period: &str,
    attachment_path: &Path,
    email_body: &str,
    additional_note: Option<&str>,
    email_signature: Option<&str>,
    subject_template: &str,
) -> Result<(), Box<dyn Error>> {
    let preview = preview_test_payroll_email(
        sender_email,
        payroll_test_email,
        pa_test_email,
        personal_assistant_name,
        personal_assistant_dob,
        personal_assistant_ni,
        payroll_period,
        attachment_path,
        email_body,
        additional_note,
        email_signature,
        subject_template,
    )?;

    send_preview(
        smtp_host,
        smtp_port,
        smtp_username,
        smtp_password,
        preview,
        attachment_path,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn send_test_payslip_email(
    smtp_host: &str,
    smtp_port: u16,
    smtp_username: &str,
    smtp_password: &str,
    sender_email: &str,
    pa_test_email: &str,
    personal_assistant_name: &str,
    personal_assistant_dob: Option<&str>,
    personal_assistant_ni: Option<&str>,
    payroll_period: &str,
    attachment_paths: &[std::path::PathBuf],
    email_body: &str,
    additional_note: Option<&str>,
    email_signature: Option<&str>,
    subject_template: &str,
) -> Result<(), Box<dyn Error>> {
    let (attachment_path, additional_attachments) = attachment_paths
        .split_first()
        .ok_or("No unsent PA payroll documents are available for this test email.")?;
    let mut preview = preview_test_payslip_email(
        sender_email,
        pa_test_email,
        personal_assistant_name,
        personal_assistant_dob,
        personal_assistant_ni,
        payroll_period,
        attachment_path,
        email_body,
        additional_note,
        email_signature,
        subject_template,
    )?;

    preview.additional_attachment_paths = additional_attachments.to_vec();
    send_preview(
        smtp_host,
        smtp_port,
        smtp_username,
        smtp_password,
        preview,
        attachment_path,
    )
}

fn send_preview(
    smtp_host: &str,
    smtp_port: u16,
    smtp_username: &str,
    smtp_password: &str,
    preview: PayrollEmailPreview,
    attachment_path: &Path,
) -> Result<(), Box<dyn Error>> {
    let email = build_message(&preview, attachment_path)?;

    let mut builder = SmtpTransport::builder_dangerous(smtp_host.trim()).port(smtp_port);
    if !smtp_username.trim().is_empty() {
        builder = builder.credentials(Credentials::new(
            smtp_username.trim().to_string(),
            smtp_password.to_string(),
        ));
    }
    let mailer = builder.build();

    mailer.send(&email)?;

    Ok(())
}

fn build_message(
    preview: &PayrollEmailPreview,
    attachment_path: &Path,
) -> Result<Message, Box<dyn Error>> {
    let mut email = Message::builder()
        .from(preview.from.parse::<Mailbox>()?)
        .to(preview.to.parse::<Mailbox>()?)
        .subject(&preview.subject);
    if let Some(cc) = &preview.cc {
        email = email.cc(cc.parse::<Mailbox>()?);
    }
    if let Some(bcc) = &preview.bcc {
        email = email.bcc(bcc.parse::<Mailbox>()?);
    }
    let mut multipart = MultiPart::mixed().singlepart(SinglePart::plain(preview.body.clone()));
    for path in std::iter::once(attachment_path).chain(
        preview
            .additional_attachment_paths
            .iter()
            .map(|path| path.as_path()),
    ) {
        let data = fs::read(path)?;
        let filename = path
            .file_name()
            .ok_or("Invalid attachment filename.")?
            .to_string_lossy()
            .to_string();
        multipart = multipart.singlepart(
            Attachment::new(filename).body(data, ContentType::parse("application/pdf")?),
        );
    }
    Ok(email.multipart(multipart)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combined_pdf_mime_message_preserves_existing_content_and_addresses() {
        let directory = tempfile::TempDir::new().unwrap();
        let payslip = directory.path().join("payslip.pdf");
        fs::write(&payslip, b"%PDF-1.4 payslip").unwrap();
        for additional in [vec!["P60.pdf"], vec!["P45.pdf"], vec!["P60.pdf", "P45.pdf"]] {
            let mut preview = preview_payroll_email(
                "payroll@example.com",
                "employer@example.com",
                Some("pa@example.com"),
                "Alex Smith",
                None,
                None,
                "202608w22",
                &payslip,
                "Existing payslip body",
                Some("Note"),
                Some("Signature"),
                "{Personal Assistant Name} {YYYYMMwWW}",
            )
            .unwrap();
            for name in &additional {
                let path = directory.path().join(name);
                fs::write(&path, b"%PDF-1.4 supplement").unwrap();
                preview.additional_attachment_paths.push(path);
            }
            let message = build_message(&preview, &payslip).unwrap();
            let raw = String::from_utf8(message.formatted()).unwrap();
            assert_eq!(
                raw.matches("Content-Type: application/pdf").count(),
                additional.len() + 1
            );
            assert!(raw.contains("filename=\"payslip.pdf\""));
            for name in additional {
                assert!(raw.contains(&format!("filename=\"{name}\"")));
            }
            assert!(raw.contains("Existing payslip body"));
            assert!(raw.contains("Note"));
            assert!(raw.contains("Signature"));
            assert_eq!(preview.to, "payroll@example.com");
            assert_eq!(preview.cc.as_deref(), Some("employer@example.com"));
            assert_eq!(preview.bcc.as_deref(), Some("pa@example.com"));
            let recipients = message
                .envelope()
                .to()
                .iter()
                .map(|address| address.to_string())
                .collect::<Vec<_>>();
            assert!(recipients.contains(&"pa@example.com".to_string()));
            assert!(!raw.contains("P30"));
        }
    }

    #[test]
    fn preview_composes_recipients_subject_and_signature_without_an_attachment() {
        let preview = preview_payroll_email(
            "payroll@example.test",
            "employer@example.test",
            Some("pa@example.test"),
            "Alex Smith",
            None,
            None,
            "202604w02",
            Path::new("/not-created/timesheet.pdf"),
            "Timesheet attached.",
            None,
            Some("Kind regards"),
            "{Personal Assistant Name} {YYYYMMwWW}",
        )
        .unwrap();

        assert_eq!(preview.to, "payroll@example.test");
        assert_eq!(preview.from, "employer@example.test");
        assert_eq!(preview.cc.as_deref(), Some("employer@example.test"));
        assert_eq!(preview.bcc.as_deref(), Some("pa@example.test"));
        assert_eq!(preview.subject, "Alex Smith 202604w02");
        assert_eq!(preview.body, "Timesheet attached.\n\nKind regards");
        assert_eq!(preview.attachment_path, "/not-created/timesheet.pdf");
    }

    #[test]
    fn preview_omits_blank_personal_assistant_bcc() {
        let preview = preview_payroll_email(
            "payroll@example.test",
            "employer@example.test",
            Some("  "),
            "Alex Smith",
            None,
            None,
            "01/04/2026",
            Path::new("payslip.pdf"),
            "Payslip attached.",
            None,
            None,
            "{YYYYMMwWW}",
        )
        .unwrap();

        assert_eq!(preview.bcc, None);
        assert_eq!(preview.body, "Payslip attached.");
    }

    #[test]
    fn test_preview_uses_only_test_recipients_and_test_markers() {
        let preview = preview_test_payroll_email(
            "sender@example.test",
            "payroll-test@example.test",
            Some("pa-test@example.test"),
            "Alex Smith",
            None,
            None,
            "01/04/2026",
            Path::new("timesheet.pdf"),
            "Timesheet attached.",
            Some("Temporary note"),
            Some("Kind regards"),
            "{Personal Assistant Name} {YYYYMMwWW}",
        )
        .unwrap();

        assert_eq!(preview.to, "payroll-test@example.test");
        assert_eq!(preview.cc, None);
        assert_eq!(preview.bcc.as_deref(), Some("pa-test@example.test"));
        assert!(preview.subject.starts_with("TEST: "));
        assert_eq!(
            preview.body,
            "TEST:\n\nTimesheet attached.\n\nTemporary note\n\nKind regards"
        );
    }

    #[test]
    fn test_preview_requires_payroll_test_recipient() {
        let error = preview_test_payroll_email(
            "sender@example.test",
            " ",
            None,
            "Alex Smith",
            None,
            None,
            "01/04/2026",
            Path::new("timesheet.pdf"),
            "Timesheet attached.",
            None,
            None,
            "{YYYYMMwWW}",
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "Payroll test email address is required.");
    }

    #[test]
    fn test_payslip_preview_uses_only_pa_test_recipient_and_test_markers() {
        let preview = preview_test_payslip_email(
            "sender@example.test",
            "pa-test@example.test",
            "Alex Smith",
            None,
            None,
            "01/04/2026",
            Path::new("payslip.pdf"),
            "Payslip attached.",
            None,
            None,
            "{Personal Assistant Name} {YYYYMMwWW}",
        )
        .unwrap();

        assert_eq!(preview.to, "pa-test@example.test");
        assert_eq!(preview.cc, None);
        assert_eq!(preview.bcc, None);
        assert!(preview.subject.starts_with("TEST: "));
        assert_eq!(preview.body, "TEST:\n\nPayslip attached.");
    }

    #[test]
    fn test_payslip_preview_requires_pa_test_recipient() {
        let error = preview_test_payslip_email(
            "sender@example.test",
            " ",
            "Alex Smith",
            None,
            None,
            "01/04/2026",
            Path::new("payslip.pdf"),
            "Payslip attached.",
            None,
            None,
            "{YYYYMMwWW}",
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "PA test email address is required.");
    }
}
