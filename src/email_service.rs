use lettre::message::{
    header::ContentType, Attachment, Mailbox, Message, MultiPart, SinglePart,
};
use lettre::transport::smtp::SmtpTransport;
use lettre::Transport;
use std::error::Error;
use std::fs;
use std::path::Path;

pub fn send_timesheet_email(
    payroll_department_email: &str,
    employer_email: &str,
    personal_assistant_email: &str,
    personal_assistant_name: &str,
    personal_assistant_dob: Option<&str>,
    personal_assistant_ni: Option<&str>,
    first_week_commencing: &str,
    timesheet_path: &Path,
    email_body: &str,
    email_signature: Option<&str>,
    subject_template: &str,
) -> Result<(), Box<dyn Error>> {
    let payroll_department_email = payroll_department_email.trim();

    if payroll_department_email.is_empty() {
        return Err("Payroll Department has no email address.".into());
    }

    let employer_email = employer_email.trim();

    if employer_email.is_empty() {
        return Err("Employer has no email address.".into());
    }

    let personal_assistant_email = personal_assistant_email.trim();

    if personal_assistant_email.is_empty() {
        return Err(format!(
            "Personal Assistant {} has no email address.",
            personal_assistant_name
        )
        .into());
    }

    if !timesheet_path.exists() {
        return Err(format!(
            "Timesheet PDF does not exist: {}",
            timesheet_path.display()
        )
        .into());
    }


    let payroll_period = crate::pdf_generator::payroll_week_filename(first_week_commencing);

    let subject = subject_template
        .replace("{Personal Assistant Name}", personal_assistant_name)
        .replace("{Personal Assistant DOB}", personal_assistant_dob.unwrap_or(""))
        .replace("{Personal Assistant NI}", personal_assistant_ni.unwrap_or(""))
        .replace("{YYYYMMwWW}", &payroll_period);

    let mut body = email_body.trim_end().to_string();

    if let Some(signature) = email_signature {
        let signature = signature.trim();

        if !signature.is_empty() {
            if !body.is_empty() {
                body.push_str("\n\n");
            }

            body.push_str(signature);
        }
    }

    let attachment_data = fs::read(timesheet_path)?;

    let filename = timesheet_path
        .file_name()
        .ok_or("Invalid timesheet filename.")?
        .to_string_lossy()
        .to_string();

    let attachment = Attachment::new(filename).body(
        attachment_data,
        ContentType::parse("application/pdf")?,
    );

    let email = Message::builder()
        .from(employer_email.parse::<Mailbox>()?)
        .to(payroll_department_email.parse::<Mailbox>()?)
        .cc(employer_email.parse::<Mailbox>()?)
        .bcc(personal_assistant_email.parse::<Mailbox>()?)
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
