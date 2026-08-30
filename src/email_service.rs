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
}

pub fn preview_payroll_email(
    payroll_department_email: &str,
    employer_email: &str,
    personal_assistant_email: Option<&str>,
    personal_assistant_name: &str,
    personal_assistant_dob: Option<&str>,
    personal_assistant_ni: Option<&str>,
    first_week_commencing: &str,
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
        first_week_commencing,
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
    first_week_commencing: &str,
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
        first_week_commencing,
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
    first_week_commencing: &str,
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
        first_week_commencing,
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

#[allow(clippy::too_many_arguments)]
fn compose_payroll_email(
    sender_email: &str,
    to_email: &str,
    cc_email: Option<&str>,
    bcc_email: Option<&str>,
    personal_assistant_name: &str,
    personal_assistant_dob: Option<&str>,
    personal_assistant_ni: Option<&str>,
    first_week_commencing: &str,
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

    let payroll_period = crate::pdf_generator::payroll_week_filename(first_week_commencing);

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
    })
}

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
    first_week_commencing: &str,
    attachment_path: &Path,
    email_body: &str,
    additional_note: Option<&str>,
    email_signature: Option<&str>,
    subject_template: &str,
) -> Result<(), Box<dyn Error>> {
    let preview = preview_payroll_email(
        payroll_department_email,
        employer_email,
        personal_assistant_email,
        personal_assistant_name,
        personal_assistant_dob,
        personal_assistant_ni,
        first_week_commencing,
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
    first_week_commencing: &str,
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
        first_week_commencing,
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
    first_week_commencing: &str,
    attachment_path: &Path,
    email_body: &str,
    additional_note: Option<&str>,
    email_signature: Option<&str>,
    subject_template: &str,
) -> Result<(), Box<dyn Error>> {
    let preview = preview_test_payslip_email(
        sender_email,
        pa_test_email,
        personal_assistant_name,
        personal_assistant_dob,
        personal_assistant_ni,
        first_week_commencing,
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

fn send_preview(
    smtp_host: &str,
    smtp_port: u16,
    smtp_username: &str,
    smtp_password: &str,
    preview: PayrollEmailPreview,
    attachment_path: &Path,
) -> Result<(), Box<dyn Error>> {
    if !attachment_path.exists() {
        return Err(format!(
            "Email attachment does not exist: {}",
            attachment_path.display()
        )
        .into());
    }

    let attachment_data = fs::read(attachment_path)?;

    let filename = attachment_path
        .file_name()
        .ok_or("Invalid timesheet filename.")?
        .to_string_lossy()
        .to_string();

    let attachment =
        Attachment::new(filename).body(attachment_data, ContentType::parse("application/pdf")?);

    let mut email = Message::builder()
        .from(preview.from.parse::<Mailbox>()?)
        .to(preview.to.parse::<Mailbox>()?)
        .subject(preview.subject);

    if let Some(cc) = preview.cc {
        email = email.cc(cc.parse::<Mailbox>()?);
    }

    if let Some(personal_assistant_email) = preview.bcc {
        email = email.bcc(personal_assistant_email.parse::<Mailbox>()?);
    }

    let email = email.multipart(
        MultiPart::mixed()
            .singlepart(SinglePart::plain(preview.body))
            .singlepart(attachment),
    )?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_composes_recipients_subject_and_signature_without_an_attachment() {
        let preview = preview_payroll_email(
            "payroll@example.test",
            "employer@example.test",
            Some("pa@example.test"),
            "Alex Smith",
            None,
            None,
            "01/04/2026",
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
        assert!(preview.subject.starts_with("Alex Smith "));
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
