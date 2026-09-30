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
    /// Recipient-visible names, parallel to the attachment paths.
    pub attachment_filenames: Vec<String>,
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

pub fn validate_pa_email(address: Option<&str>) -> Result<&str, Box<dyn Error>> {
    let address = address
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("Personal Assistant has no email address.")?;
    address.parse::<Mailbox>()?;
    Ok(address)
}

pub fn payslip_subject(
    bundle: &crate::payslip_delivery_service::PayslipEmailBundle,
    schedule: Option<&crate::payroll_schedule_repository::PayrollSchedule>,
) -> Result<String, Box<dyn Error>> {
    if bundle.email_types.contains(&"payslip") {
        Ok(format!(
            "Payslip for Week {}",
            crate::payroll_file_naming::paye_week(
                schedule.ok_or("Payslip requires a captured payroll period")?
            )?
        ))
    } else {
        Ok("Payroll documents - {Personal Assistant Name}".into())
    }
}

pub fn preview_payslip_email(
    employer_email: &str,
    pa_email: Option<&str>,
    name: &str,
    bundle: &crate::payslip_delivery_service::PayslipEmailBundle,
    schedule: Option<&crate::payroll_schedule_repository::PayrollSchedule>,
    body: &str,
    note: Option<&str>,
    signature: Option<&str>,
) -> Result<PayrollEmailPreview, Box<dyn Error>> {
    let pa_email = validate_pa_email(pa_email)?;
    employer_email.trim().parse::<Mailbox>()?;
    let subject = payslip_subject(bundle, schedule)?;
    let (subject, body) = payroll_document_wording(bundle, &subject, body);
    let mut preview = compose_payroll_email(
        employer_email,
        pa_email,
        None,
        Some(employer_email),
        name,
        None,
        None,
        "",
        bundle
            .paths
            .first()
            .ok_or("No eligible payslip documents")?,
        body,
        note,
        signature,
        subject,
        "",
        "",
        "Personal Assistant has no email address.",
    )?;
    preview.additional_attachment_paths = bundle.paths.iter().skip(1).cloned().collect();
    preview.attachment_filenames = canonical_attachment_filenames(bundle, name, schedule)?;
    Ok(preview)
}

pub fn canonical_attachment_filenames(
    bundle: &crate::payslip_delivery_service::PayslipEmailBundle,
    name: &str,
    schedule: Option<&crate::payroll_schedule_repository::PayrollSchedule>,
) -> Result<Vec<String>, Box<dyn Error>> {
    bundle
        .paths
        .iter()
        .enumerate()
        .map(|(index, _path)| {
            let (kind, year) = bundle
                .attachment_identity
                .get(index)
                .ok_or("Payroll attachment is missing structured identity")?;
            match *kind {
                "payslip" => crate::payroll_file_naming::payslip_filename(
                    name,
                    schedule.ok_or("Payslip filename requires its payroll schedule")?,
                ),
                "p45" | "p60" => {
                    let kind = kind.to_ascii_uppercase();
                    let year = year
                        .as_deref()
                        .map(|year| format!(" for year {}", year.replace('/', "-")))
                        .unwrap_or_default();
                    Ok(format!(
                        "{kind}{year} for {}.pdf",
                        crate::payroll_file_naming::sanitise_filename(name)
                    ))
                }
                _ => Err("Unsupported payroll attachment type".into()),
            }
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()
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
        attachment_filenames: vec![attachment_path
            .file_name()
            .ok_or("Invalid attachment filename")?
            .to_string_lossy()
            .into_owned()],
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
    attachment_filenames: &[String],
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
    preview.attachment_filenames = attachment_filenames.to_vec();
    send_preview(
        smtp_host,
        smtp_port,
        smtp_username,
        smtp_password,
        preview,
        attachment_path,
    )
}

pub(crate) fn send_preview(
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
    for (index, path) in std::iter::once(attachment_path)
        .chain(
            preview
                .additional_attachment_paths
                .iter()
                .map(|path| path.as_path()),
        )
        .enumerate()
    {
        let data = fs::read(path)?;
        let filename = match preview.attachment_filenames.get(index) {
            Some(filename) => filename.clone(),
            None => path
                .file_name()
                .ok_or("Invalid attachment filename.")?
                .to_string_lossy()
                .to_string(),
        };
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
    #[test]
    fn payslip_preview_and_mime_have_pa_to_employer_blind_copy_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p45.pdf");
        std::fs::write(&path, b"%PDF-1.4 fixture").unwrap();
        let bundle = crate::payslip_delivery_service::PayslipEmailBundle {
            ordinary_attachment: None,
            paths: vec![path.clone()],
            email_types: vec!["p45"],
            document_ids: vec![1],
            attachment_identity: vec![("p45", Some("2026/27".into()))],
        };
        let preview = preview_payslip_email(
            "employer@example.test",
            Some("PA <pa@example.test>"),
            "Fixture PA",
            &bundle,
            None,
            "Configured payslip body",
            Some("Private note"),
            Some("Signature"),
        )
        .unwrap();
        assert_eq!(preview.to, "PA <pa@example.test>");
        assert_eq!(preview.bcc.as_deref(), Some("employer@example.test"));
        assert_eq!(preview.cc, None);
        assert_eq!(preview.subject, "Payroll documents - Fixture PA");
        assert_eq!(
            preview.body,
            "Please find attached your payroll document.\n\nPrivate note\n\nSignature"
        );
        let message = build_message(&preview, &path).unwrap();
        let mime = String::from_utf8(message.formatted()).unwrap();
        assert!(mime.contains("To: PA <pa@example.test>"));
        assert!(!mime.contains("Cc:") && !mime.contains("Bcc:"));
        assert!(!mime.contains("payroll@example.test"));
        let recipients: Vec<_> = message
            .envelope()
            .to()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(recipients.len(), 2);
        assert!(recipients.contains(&"pa@example.test".to_string()));
        assert!(recipients.contains(&"employer@example.test".to_string()));
        for invalid in [
            None,
            Some(""),
            Some("bad-address"),
            Some("one@example.test, two@example.test"),
        ] {
            assert!(preview_payslip_email(
                "employer@example.test",
                invalid,
                "Fixture PA",
                &bundle,
                None,
                "body",
                None,
                None
            )
            .is_err());
        }
    }

    #[test]
    fn mime_uses_canonical_names_for_ordinary_and_replacement_payroll_files() {
        use crate::payroll_schedule_repository::PayrollSchedule;
        use crate::payslip_delivery_service::PayslipEmailBundle;

        let dir = tempfile::tempdir().unwrap();
        let schedule = PayrollSchedule {
            id: 1,
            payroll_year: "2026/27".into(),
            cycle_number: 26,
            first_week_commencing: "01/09/2026".into(),
            latest_posting_date: "01/09/2026".into(),
            pay_date: "02/10/2026".into(),
            created_at: "fixture".into(),
            payslips_sent: false,
        };
        let pa = "Fictional Middletest Samplepa";
        let cases = [
            ("ordinary-week-26.pdf", "payslip", None, true),
            (
                "payslip-revision-4-sha256-deadbeef.pdf",
                "payslip",
                None,
                true,
            ),
            (
                "replacement-revision-2-sha256-cafebabe.pdf",
                "p45",
                Some("2026/27"),
                false,
            ),
            (
                "replacement-revision-3-sha256-facefeed.pdf",
                "p60",
                Some("2026/27"),
                false,
            ),
        ];
        for (stored_name, kind, year, has_schedule) in cases {
            let path = dir.path().join(stored_name);
            std::fs::write(&path, b"%PDF-1.4 synthetic").unwrap();
            let bundle = PayslipEmailBundle {
                paths: vec![path.clone()],
                email_types: vec![kind],
                attachment_identity: vec![(kind, year.map(str::to_owned))],
                ..Default::default()
            };
            let preview = preview_payslip_email(
                "employer@example.test",
                Some("pa@example.test"),
                pa,
                &bundle,
                has_schedule.then_some(&schedule),
                "body",
                None,
                None,
            )
            .unwrap();
            let mime =
                String::from_utf8(build_message(&preview, &path).unwrap().formatted()).unwrap();
            let expected = match kind {
                "payslip" => crate::payroll_file_naming::payslip_filename(pa, &schedule).unwrap(),
                "p45" => "P45 for year 2026-27 for Fictional Middletest Samplepa.pdf".into(),
                "p60" => "P60 for year 2026-27 for Fictional Middletest Samplepa.pdf".into(),
                _ => unreachable!(),
            };
            assert!(mime.contains(&expected), "missing {expected} in {mime}");
            assert!(
                !mime.contains("revision")
                    && !mime.contains("deadbeef")
                    && !mime.contains("cafebabe")
                    && !mime.contains("facefeed")
            );
            assert!(path.exists(), "stored path must remain unchanged");
        }
    }

    #[test]
    fn test_email_payslip_preview_uses_canonical_replacement_name() {
        use crate::payroll_schedule_repository::PayrollSchedule;
        use crate::payslip_delivery_service::PayslipEmailBundle;

        let dir = tempfile::tempdir().unwrap();
        let path = dir
            .path()
            .join("payslip-revision-7-sha256-012345abcdef.pdf");
        std::fs::write(&path, b"%PDF-1.4 synthetic").unwrap();
        let schedule = PayrollSchedule {
            id: 1,
            payroll_year: "2026/27".into(),
            cycle_number: 26,
            first_week_commencing: "01/09/2026".into(),
            latest_posting_date: "01/09/2026".into(),
            pay_date: "02/10/2026".into(),
            created_at: "fixture".into(),
            payslips_sent: false,
        };
        let bundle = PayslipEmailBundle {
            paths: vec![path.clone()],
            email_types: vec!["payslip"],
            attachment_identity: vec![("payslip", Some("2026/27".into()))],
            ..Default::default()
        };
        let mut preview = preview_test_payslip_email(
            "employer@example.test",
            "test@example.test",
            "Fictional Middletest Samplepa",
            None,
            None,
            "202610w26",
            &path,
            "body",
            None,
            None,
            "Payslip for {Personal Assistant Name}",
        )
        .unwrap();
        preview.attachment_filenames =
            canonical_attachment_filenames(&bundle, "Fictional Middletest Samplepa", Some(&schedule))
                .unwrap();
        let mime = String::from_utf8(build_message(&preview, &path).unwrap().formatted()).unwrap();
        let expected =
            crate::payroll_file_naming::payslip_filename("Fictional Middletest Samplepa", &schedule).unwrap();
        assert!(mime.contains(&expected), "expected {expected} in {mime}");
        assert!(!mime.contains("revision") && !mime.contains("012345abcdef"));
        assert!(path.exists());
    }
}
