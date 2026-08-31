use chrono::Local;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

use crate::models::PersonalAssistant;
use crate::payroll_schedule_repository::PayrollSchedule;

pub fn archive_csv(source: &Path, archive_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let now = Local::now();

    let year = now.format("%Y").to_string();
    let month = now.format("%m").to_string();

    let archive_path = archive_dir.join(year).join(month);

    fs::create_dir_all(&archive_path)?;

    let filename = source
        .file_name()
        .ok_or("Invalid source filename")?
        .to_string_lossy();

    let timestamp = now.format("%Y-%m-%d_%H%M%S");

    let archive_filename = format!("{}_{}", timestamp, filename);

    let destination = archive_path.join(archive_filename);

    fs::copy(source, &destination)?;

    Ok(destination)
}

pub struct PayrollReturnImportResult {
    pub payslips_imported: usize,
    pub information_files_imported: usize,
    pub files_skipped: usize,
}

pub fn import_payroll_return(
    zip_path: &Path,
    payslip_root: &Path,
    information_root: &Path,
    assistants: &[PersonalAssistant],
    schedule: &PayrollSchedule,
) -> Result<PayrollReturnImportResult, Box<dyn Error>> {
    let payslip_folder =
        crate::payroll_file_naming::payroll_year_directory(payslip_root, schedule)?;
    let information_folder =
        crate::payroll_file_naming::payroll_year_directory(information_root, schedule)?;
    fs::create_dir_all(&payslip_folder)?;
    fs::create_dir_all(&information_folder)?;

    let file = fs::File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    let mut result = PayrollReturnImportResult {
        payslips_imported: 0,
        information_files_imported: 0,
        files_skipped: 0,
    };

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;

        if entry.is_dir() {
            continue;
        }

        let entry_name = entry.name().to_string();

        let source_filename = Path::new(&entry_name)
            .file_name()
            .ok_or_else(|| format!("Invalid ZIP entry filename: {}", entry_name))?
            .to_string_lossy()
            .to_string();

        let destination;

        if let Some(assistant) = find_personal_assistant(&source_filename, assistants) {
            if !source_filename.to_ascii_lowercase().ends_with(".pdf") {
                result.files_skipped += 1;
                continue;
            }

            let full_name = format!("{} {}", assistant.first_name, assistant.surname);

            destination =
                crate::payroll_file_naming::payslip_path(payslip_root, &full_name, schedule)?;

            extract_entry(&mut entry, &destination)?;

            result.payslips_imported += 1;
        } else {
            destination = collision_safe_path(&information_folder, &source_filename);

            extract_entry(&mut entry, &destination)?;

            result.information_files_imported += 1;
        }
    }

    Ok(result)
}

fn collision_safe_path(directory: &Path, filename: &str) -> PathBuf {
    let initial = directory.join(filename);
    if !initial.exists() {
        return initial;
    }

    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(filename);
    let extension = path.extension().and_then(|value| value.to_str());
    for suffix in 2.. {
        let candidate_name = match extension {
            Some(extension) => format!("{stem} ({suffix}).{extension}"),
            None => format!("{stem} ({suffix})"),
        };
        let candidate = directory.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!()
}

fn find_personal_assistant<'a>(
    filename: &str,
    assistants: &'a [PersonalAssistant],
) -> Option<&'a PersonalAssistant> {
    let filename_lower = filename.to_lowercase();

    assistants.iter().find(|assistant| {
        let full_name = format!(
            "{} {}",
            assistant.first_name.trim(),
            assistant.surname.trim()
        );

        filename_lower.contains(&full_name.to_lowercase())
    })
}

fn extract_entry(
    entry: &mut zip::read::ZipFile<'_>,
    destination: &Path,
) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut output = fs::File::create(destination)?;

    io::copy(entry, &mut output)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn schedule(year: &str, first_week: &str, pay_date: &str) -> PayrollSchedule {
        PayrollSchedule {
            id: 1,
            payroll_year: year.to_string(),
            cycle_number: 6,
            first_week_commencing: first_week.to_string(),
            latest_posting_date: String::new(),
            pay_date: pay_date.to_string(),
            created_at: String::new(),
            payslips_sent: false,
        }
    }

    fn assistant() -> PersonalAssistant {
        PersonalAssistant {
            id: 1,
            first_name: "Cedar".to_string(),
            surname: "Fixture".to_string(),
            date_of_birth: None,
            national_insurance_number: None,
            address: None,
            postcode: None,
            telephone: None,
            email: None,
            employment_status: None,
            sick_pay_enabled: false,
            mileage_enabled: false,
            start_date: None,
            signature: None,
        }
    }

    fn create_return_zip(path: &Path, payslip_contents: &str, information_names: &[&str]) {
        let file = File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("Cedar Fixture.pdf", options).unwrap();
        writer.write_all(payslip_contents.as_bytes()).unwrap();
        for name in information_names {
            writer.start_file(name, options).unwrap();
            writer.write_all(name.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
    }

    fn test_root(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "direct-payment-return-{label}-{}-{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn archive_creates_timestamped_filename() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let test_root =
            std::env::temp_dir().join(format!("direct_payment_archive_test_{}", unique));

        let source_dir = test_root.join("source");
        let archive_dir = test_root.join("archive");

        fs::create_dir_all(&source_dir).unwrap();

        let source_file = source_dir.join("sample_timesheet.csv");

        File::create(&source_file).unwrap();

        let result = archive_csv(&source_file, &archive_dir).unwrap();

        let filename = result.file_name().unwrap().to_string_lossy();

        assert!(filename.contains("sample_timesheet.csv"));
        assert!(filename.len() > "sample_timesheet.csv".len());

        assert!(result.exists());

        fs::remove_dir_all(test_root).unwrap();
    }

    #[test]
    fn cycle_six_imports_as_week_22_and_matches_shared_email_lookup_path() {
        let root = test_root("week-22");
        fs::create_dir_all(&root).unwrap();
        let zip_path = root.join("return.zip");
        create_return_zip(&zip_path, "payslip", &[]);
        let schedule = schedule("2026/27", "10/08/2026", "04/09/2026");
        let payslip_root = root.join("payslips");
        let information_root = root.join("payroll-information");

        import_payroll_return(
            &zip_path,
            &payslip_root,
            &information_root,
            &[assistant()],
            &schedule,
        )
        .unwrap();

        let expected =
            crate::payroll_file_naming::payslip_path(&payslip_root, "Cedar Fixture", &schedule)
                .unwrap();
        assert!(expected.exists());
        assert!(expected.ends_with("2026 to 2027/Payslip for Week 22 for Cedar Fixture.pdf"));
        assert!(!payslip_root
            .join("2026 to 2027/Payslip for Week 6 for Cedar Fixture.pdf")
            .exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn payroll_year_directories_prevent_cross_year_payslip_overwrites() {
        let root = test_root("two-years");
        fs::create_dir_all(&root).unwrap();
        let payslip_root = root.join("payslips");
        let information_root = root.join("payroll-information");
        let first_zip = root.join("first.zip");
        let second_zip = root.join("second.zip");
        create_return_zip(&first_zip, "first year", &[]);
        create_return_zip(&second_zip, "second year", &[]);
        let first = schedule("2026/27", "10/08/2026", "04/09/2026");
        let second = schedule("2027/28", "09/08/2027", "03/09/2027");

        import_payroll_return(
            &first_zip,
            &payslip_root,
            &information_root,
            &[assistant()],
            &first,
        )
        .unwrap();
        import_payroll_return(
            &second_zip,
            &payslip_root,
            &information_root,
            &[assistant()],
            &second,
        )
        .unwrap();

        let first_path =
            crate::payroll_file_naming::payslip_path(&payslip_root, "Cedar Fixture", &first)
                .unwrap();
        let second_path =
            crate::payroll_file_naming::payslip_path(&payslip_root, "Cedar Fixture", &second)
                .unwrap();
        assert_eq!(fs::read_to_string(first_path).unwrap(), "first year");
        assert_eq!(fs::read_to_string(second_path).unwrap(), "second year");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn information_files_use_information_root_and_preserve_name_collisions() {
        let root = test_root("information");
        fs::create_dir_all(&root).unwrap();
        let first_zip = root.join("first-return.zip");
        let second_zip = root.join("second-return.zip");
        create_return_zip(&first_zip, "payslip", &["Bulletin.pdf"]);
        create_return_zip(&second_zip, "payslip", &["Bulletin.pdf"]);
        let schedule = schedule("2026/27", "10/08/2026", "04/09/2026");
        let payslip_root = root.join("payslips");
        let information_root = root.join("payroll-information");
        let email_archive = root.join("email-archive");

        import_payroll_return(
            &first_zip,
            &payslip_root,
            &information_root,
            &[assistant()],
            &schedule,
        )
        .unwrap();
        import_payroll_return(
            &second_zip,
            &payslip_root,
            &information_root,
            &[assistant()],
            &schedule,
        )
        .unwrap();

        let year = information_root.join("2026 to 2027");
        assert!(year.join("Bulletin.pdf").exists());
        assert!(year.join("Bulletin (2).pdf").exists());
        assert!(!email_archive.exists());

        fs::remove_dir_all(root).unwrap();
    }
}
