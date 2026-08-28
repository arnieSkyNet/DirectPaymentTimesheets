use lettre::message::{
    header::ContentType, Attachment, Mailbox, Message, MultiPart, SinglePart,
};
use lettre::transport::smtp::SmtpTransport;
use lettre::Transport;
use std::error::Error;
use std::fs;
use std::path::Path;

pub fn send_payslip_email(
    employer_email: &str,
    recipient_email: &str,
    personal_assistant_name: &str,
    payroll_week: i64,
    payslip_path: &Path,
    email_signature: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    let employer_email = employer_email.trim();

    if employer_email.is_empty() {
        return Err("Employer has no email address.".into());
    }

    let recipient_email = recipient_email.trim();

    if recipient_email.is_empty() {
        return Err("Personal Assistant has no email address.".into());
    }

    if !payslip_path.exists() {
        return Err(format!(
            "Payslip file does not exist: {}",
            payslip_path.display()
        )
        .into());
    }

    let subject = format!("Payslip for Week {}", payroll_week);

    let mut body = format!(
        "Dear {},\n\nPlease find attached your payslip for Week {}.\n\nKind regards,\n",
        personal_assistant_name, payroll_week
    );

    if let Some(signature) = email_signature {
        if !signature.trim().is_empty() {
            body.push('\n');
            body.push_str(signature.trim());
            body.push('\n');
        }
    }

    let attachment_data = fs::read(payslip_path)?;

    let filename = payslip_path
        .file_name()
        .ok_or("Invalid payslip filename.")?
        .to_string_lossy()
        .to_string();

    let attachment = Attachment::new(filename).body(
        attachment_data,
        ContentType::parse("application/pdf")?,
    );

    let email = Message::builder()
        .from(employer_email.parse::<Mailbox>()?)
        .to(recipient_email.parse::<Mailbox>()?)
        .subject(subject)
        .multipart(
            MultiPart::mixed()
                .singlepart(SinglePart::plain(body))
                .singlepart(attachment),
        )?;

    let mailer = SmtpTransport::builder_dangerous("localhost")
        .port(25)
        .build();

    mailer.send(&email)?;

    Ok(())
}

