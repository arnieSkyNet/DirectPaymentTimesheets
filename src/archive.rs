use chrono::Local;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

use crate::models::PersonalAssistant;

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
    payslip_folder: &Path,
    information_folder: &Path,
    assistants: &[PersonalAssistant],
    week_number: i64,
) -> Result<PayrollReturnImportResult, Box<dyn Error>> {
    fs::create_dir_all(payslip_folder)?;
    fs::create_dir_all(information_folder)?;

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

            let filename = format!("Payslip for Week {} for {}.pdf", week_number, full_name);

            destination = payslip_folder.join(filename);

            extract_entry(&mut entry, &destination)?;

            result.payslips_imported += 1;
        } else {
            destination = information_folder.join(&source_filename);

            extract_entry(&mut entry, &destination)?;

            result.information_files_imported += 1;
        }
    }

    Ok(result)
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
    use std::time::{SystemTime, UNIX_EPOCH};

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
}
