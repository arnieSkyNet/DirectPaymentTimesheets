use chrono::Local;
use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

use crate::models::PersonalAssistant;
use crate::payroll_schedule_repository::PayrollSchedule;

#[cfg(test)]
pub fn archive_csv(source: &Path, archive_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let bytes = fs::read(source)?;
    archive_csv_bytes(source, &bytes, archive_dir)
}

pub fn archive_csv_bytes(
    source: &Path,
    bytes: &[u8],
    archive_dir: &Path,
) -> Result<PathBuf, Box<dyn Error>> {
    let now = Local::now();

    let year = now.format("%Y").to_string();
    let month = now.format("%m").to_string();

    let archive_path = archive_dir.join(year).join(month);

    fs::create_dir_all(&archive_path)?;

    let filename = source
        .file_name()
        .ok_or("Invalid source filename")?
        .to_string_lossy();

    let timestamp = now.format("%Y-%m-%d_%H%M%S_%f");
    for sequence in 0..1000 {
        let archive_filename = format!("{timestamp}_{sequence:03}_{filename}");
        let destination = archive_path.join(archive_filename);
        let mut file = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        };

        let result = (|| -> io::Result<()> {
            file.write_all(bytes)?;
            file.sync_all()?;
            if file.metadata()?.len() != bytes.len() as u64 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "archived CSV length does not match source bytes",
                ));
            }
            drop(file);
            if fs::read(&destination)? != bytes {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "archived CSV content does not match source bytes",
                ));
            }
            Ok(())
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&destination);
            return Err(error.into());
        }
        return Ok(destination);
    }

    Err("Could not allocate a unique CSV archive filename".into())
}

pub const MAX_PAYROLL_RETURN_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_PAYROLL_RETURN_ENTRIES: usize = 512;
pub const MAX_PAYROLL_RETURN_FILENAME_BYTES: usize = 1024;
pub const MAX_PAYROLL_RETURN_PATH_COMPONENT_BYTES: usize = 255;
pub const MAX_PAYROLL_RETURN_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_PAYROLL_RETURN_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_PAYROLL_RETURN_COMPRESSION_RATIO: u64 = 200;

#[derive(Debug, Default)]
pub struct PayrollReturnImportResult {
    pub payslips_imported: usize,
    pub payslips_already_present: usize,
    pub archival_payslips_imported: usize,
    pub archival_payslips_already_present: usize,
    pub supplements_imported: usize,
    pub supplements_already_present: usize,
    pub information_files_imported: usize,
    pub information_files_already_present: usize,
    pub files_skipped: usize,
    pub details: Vec<String>,
    pub publication_failure: Option<String>,
    pub published_paths: Vec<PathBuf>,
    pub prep_sheet_paths: Vec<PathBuf>,
    pub schedule_entries_imported: usize,
    pub prep_sheet_failures: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlannedKind {
    Payslip,
    ArchivalPayslip,
    P60,
    P45,
    PrepSheet,
    Information,
}

impl PlannedKind {
    fn is_pa_document(self) -> bool {
        matches!(
            self,
            Self::Payslip | Self::ArchivalPayslip | Self::P60 | Self::P45
        )
    }
}

fn supplement_filename(filename: &str, token: &str, assistant: &PersonalAssistant) -> String {
    let start = filename
        .char_indices()
        .find_map(|(index, character)| {
            if character.eq_ignore_ascii_case(&'p')
                && (index == 0
                    || !filename[..index]
                        .chars()
                        .next_back()
                        .unwrap()
                        .is_alphanumeric())
                && filename[index..]
                    .get(..3)
                    .is_some_and(|value| value.eq_ignore_ascii_case(token))
                && filename[index + 3..]
                    .chars()
                    .next()
                    .is_none_or(|c| !c.is_alphanumeric())
            {
                Some(index)
            } else {
                None
            }
        })
        .expect("classified document has a type token");
    let remainder = &filename[start..];
    let full_name = format!(
        "{} {}",
        assistant.first_name.trim(),
        assistant.surname.trim()
    );
    if contains_token_sequence(
        &normalized_tokens(remainder),
        &normalized_tokens(&full_name),
    ) {
        remainder.to_string()
    } else {
        let stem = Path::new(remainder).file_stem().unwrap().to_string_lossy();
        let safe_name: String = full_name
            .chars()
            .map(|c| {
                if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        format!("{stem} for {safe_name}.pdf")
    }
}

#[derive(Debug)]
struct PlannedEntry {
    archive_index: usize,
    source_name: String,
    destination: PathBuf,
    kind: PlannedKind,
    declared_size: u64,
    supplement_pa_id: Option<i64>,
}

#[derive(Debug)]
struct StagedEntry {
    plan: PlannedEntry,
    temporary_path: PathBuf,
}

trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}
type SourceArchive = ZipArchive<Box<dyn ReadSeek>>;

fn source_archive(path: &Path) -> Result<SourceArchive, Box<dyn Error>> {
    let size = fs::metadata(path)?.len();
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
    {
        if size > MAX_PAYROLL_RETURN_ARCHIVE_BYTES {
            return Err("Payroll Return ZIP is too large".into());
        }
        let declared = standard_zip_entry_count(path)?;
        let archive = ZipArchive::new(Box::new(fs::File::open(path)?) as Box<dyn ReadSeek>)?;
        if declared != archive.len() {
            return Err(
                "Payroll Return ZIP contains duplicate or ambiguously decoded entry names.".into(),
            );
        }
        if archive.len() > MAX_PAYROLL_RETURN_ENTRIES {
            return Err("Payroll Return ZIP contains too many entries.".into());
        }
        Ok(archive)
    } else {
        // Use the same validation, staging and no-clobber publication for a single file.
        if size > MAX_PAYROLL_RETURN_ENTRY_BYTES {
            return Err("Payroll document exceeds the per-entry size limit.".into());
        }
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        writer.start_file(
            path.file_name()
                .and_then(|v| v.to_str())
                .ok_or("Invalid source filename")?,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )?;
        let copied = io::copy(
            &mut fs::File::open(path)?.take(MAX_PAYROLL_RETURN_ENTRY_BYTES + 1),
            &mut writer,
        )?;
        if copied > MAX_PAYROLL_RETURN_ENTRY_BYTES {
            return Err("Payroll document exceeds the per-entry size limit.".into());
        }
        Ok(ZipArchive::new(
            Box::new(writer.finish()?) as Box<dyn ReadSeek>
        )?)
    }
}

fn classify_filename(
    name: &str,
    assistants: &[PersonalAssistant],
) -> Result<(PlannedKind, Option<i64>), Box<dyn Error>> {
    let normalized = normalized_assistants(assistants)?;
    let matches = matching_assistants(name, &normalized);
    let tokens = normalized_tokens(
        Path::new(name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(name),
    );
    let has = |token: &str| tokens.iter().any(|s| s == token);
    let pdf = name.to_ascii_lowercase().ends_with(".pdf");
    if has("p30")
        || contains_token_sequence(&tokens, &normalized_tokens("bank transfer"))
        || contains_token_sequence(&tokens, &normalized_tokens("quarter end"))
    {
        return Ok((PlannedKind::Information, None));
    }
    if crate::payroll_prep_sheet_import_service::is_prep_sheet_filename(Path::new(name)) {
        return Ok((PlannedKind::PrepSheet, None));
    }
    if has("p60") || has("p45") {
        if !pdf || matches.len() != 1 || (has("p60") && has("p45")) {
            return Err(format!("P60/P45 document '{name}' must be a PDF matching exactly one maintained Personal Assistant and one document type.").into());
        }
        return Ok((
            if has("p60") {
                PlannedKind::P60
            } else {
                PlannedKind::P45
            },
            Some(matches[0].0.id),
        ));
    }
    if pdf && (has("payslip") || matches.iter().any(|(_, n)| tokens == **n)) {
        if matches.len() > 1 {
            return Err(format!("Payslip PDF '{name}' matches more than one Personal Assistant; no files were imported.").into());
        }
        if matches.len() != 1 {
            return Err(format!("Payslip PDF '{name}' does not match exactly one maintained Personal Assistant; no files were imported.").into());
        }
        return Ok((PlannedKind::Payslip, Some(matches[0].0.id)));
    }
    Ok((PlannedKind::Information, None))
}

/// Read/classify every entry before the GUI decides whether a period is needed.
/// Week numbers recur each year; they alone do not select a reliable schedule.
pub fn source_payslip_filenames(
    path: &Path,
    assistants: &[PersonalAssistant],
) -> Result<Vec<String>, Box<dyn Error>> {
    let mut archive = source_archive(path)?;
    let mut total = 0;
    let mut payslips = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        if !entry.is_file() || entry.encrypted() {
            return Err("ZIP entry is encrypted or not a regular file.".into());
        }
        validate_archive_name(entry.name(), entry.name_raw())?;
        validate_entry_sizes(entry.name(), entry.compressed_size(), entry.size())?;
        total = checked_total_size(total, entry.size())?;
        let filename = portable_basename(entry.name())?;
        if classify_filename(&filename, assistants)?.0 == PlannedKind::Payslip {
            payslips.push(filename);
        }
    }
    Ok(payslips)
}

#[cfg(test)]
pub fn import_payroll_return(
    path: &Path,
    payslip_root: &Path,
    information_root: &Path,
    assistants: &[PersonalAssistant],
    schedule: &PayrollSchedule,
) -> Result<PayrollReturnImportResult, Box<dyn Error>> {
    import_payroll_documents(
        path,
        payslip_root,
        information_root,
        assistants,
        |_| Ok(Some(schedule.clone())),
        |_, _, _, _| Ok(()),
    )
}

pub fn import_payroll_documents(
    path: &Path,
    payslip_root: &Path,
    information_root: &Path,
    assistants: &[PersonalAssistant],
    resolve_payslip: impl Fn(&str) -> Result<Option<PayrollSchedule>, Box<dyn Error>>,
    register_document: impl Fn(i64, &str, &Path, Option<&str>) -> Result<(), Box<dyn Error>>,
) -> Result<PayrollReturnImportResult, Box<dyn Error>> {
    let mut archive = source_archive(path)?;
    let normalized_assistants = normalized_assistants(assistants)?;
    let mut plans = Vec::new();
    let mut details = Vec::new();
    let mut files_skipped = 0;
    let mut total_size = 0u64;
    let mut payslip_destinations = HashSet::new();
    let mut supplement_destinations = HashSet::new();

    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if entry.is_dir() {
            files_skipped += 1;
            details.push(format!("Ignored directory entry '{}'.", entry.name()));
            continue;
        }
        if !entry.is_file() {
            return Err(format!(
                "ZIP entry '{}' is a symlink or non-regular file.",
                entry.name()
            )
            .into());
        }
        validate_archive_name(entry.name(), entry.name_raw())?;
        if entry.encrypted() {
            return Err(format!(
                "ZIP entry '{}' is encrypted and cannot be imported.",
                entry.name()
            )
            .into());
        }
        validate_entry_sizes(entry.name(), entry.compressed_size(), entry.size())?;
        total_size = checked_total_size(total_size, entry.size())?;

        let source_filename = portable_basename(entry.name())?;
        let (mut kind, pa_id) = classify_filename(&source_filename, assistants)?;
        let year = crate::payroll_file_naming::document_year(&source_filename);
        let information_folder =
            crate::payroll_file_naming::document_year_directory(information_root, year.as_deref())?;
        let destination = match kind {
            PlannedKind::P60 | PlannedKind::P45 => {
                let assistant = assistants.iter().find(|pa| Some(pa.id) == pa_id).unwrap();
                let token = if kind == PlannedKind::P60 {
                    "p60"
                } else {
                    "p45"
                };
                let destination = crate::payroll_file_naming::independent_pa_document_directory(
                    payslip_root,
                    assistant.id,
                    year.as_deref(),
                )?
                .join(supplement_filename(&source_filename, token, assistant));
                if !supplement_destinations.insert(destination.clone()) {
                    return Err(format!(
                        "Payroll Return contains duplicate {token} destination '{}'.",
                        destination.display()
                    )
                    .into());
                }
                details.push(format!(
                    "{token} for {} {}: '{}'.",
                    assistant.first_name,
                    assistant.surname,
                    destination.display()
                ));
                destination
            }
            PlannedKind::Payslip => {
                let assistant = assistants.iter().find(|pa| Some(pa.id) == pa_id).unwrap();
                let destination = if let Some(schedule) = resolve_payslip(&source_filename)? {
                    crate::payroll_file_naming::payslip_path(
                        payslip_root,
                        &format!(
                            "{} {}",
                            assistant.first_name.trim(),
                            assistant.surname.trim()
                        ),
                        &schedule,
                    )?
                } else {
                    kind = PlannedKind::ArchivalPayslip;
                    let path = crate::payroll_file_naming::independent_pa_document_directory(
                        payslip_root,
                        assistant.id,
                        year.as_deref(),
                    )?
                    .join(
                        crate::payroll_file_naming::archival_payslip_filename(&source_filename),
                    );
                    details.push(format!("Archived ordinary payslip without cycle association: '{}'. Not eligible for automatic Email Payslips or payroll settlement; no year was inferred from other documents in the package.", path.display()));
                    path
                };
                if !payslip_destinations.insert(destination.clone()) {
                    return Err(
                        "Payroll Return contains more than one payslip for the same destination."
                            .into(),
                    );
                }
                destination
            }
            PlannedKind::ArchivalPayslip => {
                unreachable!("only assigned after payslip classification")
            }
            PlannedKind::Information | PlannedKind::PrepSheet => {
                if kind == PlannedKind::Information
                    && !source_filename.to_ascii_lowercase().ends_with(".pdf")
                    && !matching_assistants(&source_filename, &normalized_assistants).is_empty()
                {
                    files_skipped += 1;
                    details.push(format!("Skipped non-PDF file '{source_filename}' that contains a Personal Assistant name."));
                    continue;
                }
                // Resolve collisions after staging, when incoming bytes are available.
                information_folder.join(&source_filename)
            }
        };

        plans.push(PlannedEntry {
            archive_index: index,
            source_name: source_filename,
            destination,
            kind,
            declared_size: entry.size(),
            supplement_pa_id: matches!(kind, PlannedKind::P60 | PlannedKind::P45)
                .then(|| pa_id.unwrap()),
        });
    }

    let mut staged = Vec::new();
    for plan in plans {
        match stage_entry(&mut archive, plan) {
            Ok(entry) => staged.push(entry),
            Err(error) => {
                cleanup_staged(&staged);
                return Err(error);
            }
        }
    }

    if let Err(error) = validate_existing_payslips(&staged) {
        cleanup_staged(&staged);
        return Err(error);
    }

    let mut result = PayrollReturnImportResult {
        files_skipped,
        details,
        ..PayrollReturnImportResult::default()
    };
    let mut remaining = staged.into_iter();
    while let Some(mut entry) = remaining.next() {
        if matches!(
            entry.plan.kind,
            PlannedKind::Information | PlannedKind::PrepSheet
        ) {
            match allocate_information_path(
                entry
                    .plan
                    .destination
                    .parent()
                    .expect("planned destination has a parent"),
                &entry.plan.source_name,
                &entry.temporary_path,
            ) {
                Ok((destination, identical)) => {
                    entry.plan.destination = destination;
                    if identical {
                        result.information_files_already_present += 1;
                        result.details.push(format!(
                            "Payroll information already present and unchanged: '{}'.",
                            entry.plan.destination.display()
                        ));
                        if entry.plan.kind == PlannedKind::PrepSheet {
                            // Re-run the existing parser against the existing physical copy.
                            result.prep_sheet_paths.push(entry.plan.destination.clone());
                        }
                        cleanup_staged_entry(&entry);
                        continue;
                    }
                }
                Err(error) => {
                    result.publication_failure = Some(format!(
                        "Could not resolve information document '{}': {error}",
                        entry.plan.source_name
                    ));
                    cleanup_staged_entry(&entry);
                    for pending in remaining {
                        cleanup_staged_entry(&pending);
                    }
                    return Ok(result);
                }
            }
        }
        if entry.plan.kind.is_pa_document() && entry.plan.destination.exists() {
            match files_equal(&entry.temporary_path, &entry.plan.destination) {
                Ok(true) => {
                    if entry.plan.kind == PlannedKind::Payslip {
                        result.payslips_already_present += 1;
                    } else if entry.plan.kind == PlannedKind::ArchivalPayslip {
                        result.archival_payslips_already_present += 1;
                    } else {
                        result.supplements_already_present += 1;
                    }
                    result.details.push(format!(
                        "Payslip already present and unchanged: '{}'.",
                        entry.plan.destination.display()
                    ));
                    if let Err(error) = register_staged_document(&entry, &register_document) {
                        result.publication_failure = Some(format!("Document is stored but registration failed; re-import to retry: {error}"));
                        cleanup_staged_entry(&entry);
                        for pending in remaining {
                            cleanup_staged_entry(&pending);
                        }
                        return Ok(result);
                    }
                    cleanup_staged_entry(&entry);
                    continue;
                }
                Ok(false) => {
                    cleanup_staged_entry(&entry);
                    for pending in remaining {
                        cleanup_staged_entry(&pending);
                    }
                    result.publication_failure = Some(format!(
                        "Canonical payslip '{}' changed after preflight and was not overwritten.",
                        entry.plan.destination.display()
                    ));
                    return Ok(result);
                }
                Err(error) => {
                    cleanup_staged_entry(&entry);
                    for pending in remaining {
                        cleanup_staged_entry(&pending);
                    }
                    result.publication_failure = Some(format!(
                        "Could not revalidate canonical payslip '{}': {error}",
                        entry.plan.destination.display()
                    ));
                    return Ok(result);
                }
            }
        }
        match publish_no_clobber(&entry.temporary_path, &entry.plan.destination) {
            Ok(()) => {
                match entry.plan.kind {
                    PlannedKind::Payslip => result.payslips_imported += 1,
                    PlannedKind::ArchivalPayslip => result.archival_payslips_imported += 1,
                    PlannedKind::P60 | PlannedKind::P45 => result.supplements_imported += 1,
                    PlannedKind::Information => result.information_files_imported += 1,
                    PlannedKind::PrepSheet => {
                        result.information_files_imported += 1;
                        result.prep_sheet_paths.push(entry.plan.destination.clone());
                    }
                }
                result.published_paths.push(entry.plan.destination.clone());
                if let Err(error) = register_staged_document(&entry, &register_document) {
                    result.publication_failure = Some(format!(
                        "Document is stored but registration failed; re-import to retry: {error}"
                    ));
                    cleanup_staged_entry(&entry);
                    for pending in remaining {
                        cleanup_staged_entry(&pending);
                    }
                    return Ok(result);
                }
                cleanup_staged_entry(&entry);
            }
            Err(error) => {
                cleanup_staged_entry(&entry);
                for pending in remaining {
                    cleanup_staged_entry(&pending);
                }
                result.publication_failure = Some(format!(
                    "Could not publish '{}' after {} file(s) were published: {}",
                    entry.plan.destination.display(),
                    result.published_paths.len(),
                    error
                ));
                return Ok(result);
            }
        }
    }
    Ok(result)
}

fn register_staged_document(
    entry: &StagedEntry,
    register: &impl Fn(i64, &str, &Path, Option<&str>) -> Result<(), Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    if let Some(pa) = entry.plan.supplement_pa_id {
        register(
            pa,
            if entry.plan.kind == PlannedKind::P60 {
                "p60"
            } else {
                "p45"
            },
            &entry.plan.destination,
            crate::payroll_file_naming::document_year(&entry.plan.source_name).as_deref(),
        )?;
    }
    Ok(())
}

fn normalized_assistants(
    assistants: &[PersonalAssistant],
) -> Result<Vec<(&PersonalAssistant, Vec<String>)>, Box<dyn Error>> {
    let mut normalized = Vec::new();
    for assistant in assistants {
        let tokens = normalized_tokens(&format!(
            "{} {}",
            assistant.first_name.trim(),
            assistant.surname.trim()
        ));
        if tokens.is_empty() {
            return Err(format!(
                "Personal Assistant {} has an empty normalized name.",
                assistant.id
            )
            .into());
        }
        normalized.push((assistant, tokens));
    }
    Ok(normalized)
}

fn normalized_tokens(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

fn contains_token_sequence(haystack: &[String], needle: &[String]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn matching_assistants<'a>(
    filename: &str,
    assistants: &'a [(&'a PersonalAssistant, Vec<String>)],
) -> Vec<(&'a PersonalAssistant, &'a Vec<String>)> {
    let tokens = normalized_tokens(
        Path::new(filename)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or(filename),
    );
    assistants
        .iter()
        .filter(|(_, name)| contains_token_sequence(&tokens, name))
        .map(|(assistant, name)| (*assistant, name))
        .collect()
}

fn validate_archive_name(name: &str, raw: &[u8]) -> Result<(), Box<dyn Error>> {
    if raw.len() > MAX_PAYROLL_RETURN_FILENAME_BYTES {
        return Err(
            format!("ZIP entry name exceeds {MAX_PAYROLL_RETURN_FILENAME_BYTES} bytes.").into(),
        );
    }
    if name.contains('\0') || name.contains('\u{fffd}') {
        return Err("ZIP entry has an invalid or ambiguously decoded name.".into());
    }
    let portable = name.replace('\\', "/");
    if portable.starts_with('/') || portable.starts_with("//") {
        return Err(format!("ZIP entry '{name}' uses an absolute or UNC path.").into());
    }
    let bytes = portable.as_bytes();
    if bytes.get(1) == Some(&b':') && bytes[0].is_ascii_alphabetic() {
        return Err(format!("ZIP entry '{name}' uses a Windows drive path.").into());
    }
    for component in portable.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return Err(format!("ZIP entry '{name}' contains an unsafe path component.").into());
        }
        if component.len() > MAX_PAYROLL_RETURN_PATH_COMPONENT_BYTES {
            return Err(
                format!("ZIP entry '{name}' has an excessively long path component.").into(),
            );
        }
        if component.chars().any(|character| character.is_control()) {
            return Err(format!("ZIP entry '{name}' contains control characters.").into());
        }
        if component
            .chars()
            .any(|character| matches!(character, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
            || component.ends_with(['.', ' '])
        {
            return Err(format!("ZIP entry '{name}' is not a portable filename.").into());
        }
    }
    Ok(())
}

fn standard_zip_entry_count(path: &Path) -> Result<usize, Box<dyn Error>> {
    const END_RECORD_MINIMUM: u64 = 22;
    const MAX_COMMENT: u64 = 65_535;
    let mut file = fs::File::open(path)?;
    let length = file.metadata()?.len();
    if length < END_RECORD_MINIMUM {
        return Err("Payroll Return ZIP is truncated.".into());
    }
    let tail_length = length.min(END_RECORD_MINIMUM + MAX_COMMENT);
    file.seek(SeekFrom::End(-(tail_length as i64)))?;
    let mut tail = vec![0u8; tail_length as usize];
    file.read_exact(&mut tail)?;
    let position = (0..=tail.len() - END_RECORD_MINIMUM as usize)
        .rev()
        .find(|position| {
            tail[*position..].starts_with(b"PK\x05\x06")
                && *position
                    + END_RECORD_MINIMUM as usize
                    + u16::from_le_bytes([tail[*position + 20], tail[*position + 21]]) as usize
                    == tail.len()
        })
        .ok_or("Payroll Return ZIP has no valid end record.")?;
    if position + 12 > tail.len() {
        return Err("Payroll Return ZIP end record is truncated.".into());
    }
    let count = u16::from_le_bytes([tail[position + 10], tail[position + 11]]);
    if count == u16::MAX {
        return Err("ZIP64 Payroll Return archives are not supported.".into());
    }
    Ok(count as usize)
}

fn portable_basename(name: &str) -> Result<String, Box<dyn Error>> {
    name.replace('\\', "/")
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("Invalid ZIP entry filename: {name}").into())
}

fn validate_entry_sizes(
    name: &str,
    compressed: u64,
    uncompressed: u64,
) -> Result<(), Box<dyn Error>> {
    if uncompressed > MAX_PAYROLL_RETURN_ENTRY_BYTES {
        return Err(format!("ZIP entry '{name}' exceeds the per-entry size limit.").into());
    }
    if uncompressed > 0 && compressed == 0 {
        return Err(format!("ZIP entry '{name}' has an invalid zero compressed size.").into());
    }
    if compressed > 0
        && uncompressed
            > compressed
                .checked_mul(MAX_PAYROLL_RETURN_COMPRESSION_RATIO)
                .unwrap_or(u64::MAX)
    {
        return Err(format!("ZIP entry '{name}' exceeds the maximum compression ratio.").into());
    }
    Ok(())
}

fn checked_total_size(current: u64, entry: u64) -> Result<u64, Box<dyn Error>> {
    let total = current
        .checked_add(entry)
        .ok_or("Payroll Return uncompressed size overflowed.")?;
    if total > MAX_PAYROLL_RETURN_TOTAL_BYTES {
        return Err(format!(
            "Payroll Return ZIP expands beyond the {} byte total limit.",
            MAX_PAYROLL_RETURN_TOTAL_BYTES
        )
        .into());
    }
    Ok(total)
}

fn validate_existing_payslips(entries: &[StagedEntry]) -> Result<(), Box<dyn Error>> {
    for entry in entries {
        if entry.plan.kind.is_pa_document()
            && entry.plan.destination.exists()
            && !files_equal(&entry.temporary_path, &entry.plan.destination)?
        {
            return Err(format!(
                "A different canonical payslip already exists at '{}'; it was not overwritten.",
                entry.plan.destination.display()
            )
            .into());
        }
    }
    Ok(())
}

fn allocate_information_path(
    directory: &Path,
    filename: &str,
    incoming: &Path,
) -> Result<(PathBuf, bool), Box<dyn Error>> {
    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(filename);
    let extension = path.extension().and_then(|value| value.to_str());
    // Inspect all existing candidates before choosing a free suffix, including
    // candidates beyond a gap left by a removed/renamed earlier copy.
    let existing = fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<HashSet<_>>>()?;
    let mut first_free = None;
    for suffix in 1..=10_000usize {
        let candidate_name = if suffix == 1 {
            filename.to_string()
        } else {
            match extension {
                Some(extension) => format!("{stem} ({suffix}).{extension}"),
                None => format!("{stem} ({suffix})"),
            }
        };
        let candidate = directory.join(candidate_name);
        if existing.contains(&candidate) {
            if files_equal(incoming, &candidate)? {
                return Ok((candidate, true));
            }
        } else if first_free.is_none() {
            first_free = Some(candidate);
        }
    }
    first_free.map(|path| (path, false)).ok_or_else(|| {
        format!("Could not allocate a collision-safe destination for '{filename}'.").into()
    })
}

fn stage_entry(
    archive: &mut SourceArchive,
    mut plan: PlannedEntry,
) -> Result<StagedEntry, Box<dyn Error>> {
    let parent = plan
        .destination
        .parent()
        .ok_or("Payroll Return destination has no parent directory.")?;
    fs::create_dir_all(parent)?;
    let mut entry = archive.by_index(plan.archive_index)?;
    let (temporary_path, mut output) = create_temporary_file(parent)?;
    let result = (|| -> Result<(), Box<dyn Error>> {
        let mut limited = (&mut entry).take(MAX_PAYROLL_RETURN_ENTRY_BYTES + 1);
        let copied = io::copy(&mut limited, &mut output)?;
        if copied > MAX_PAYROLL_RETURN_ENTRY_BYTES {
            return Err(format!(
                "ZIP entry '{}' exceeded the streaming output limit.",
                plan.source_name
            )
            .into());
        }
        if copied != plan.declared_size {
            return Err(format!(
                "ZIP entry '{}' extracted {copied} bytes but declared {}.",
                plan.source_name, plan.declared_size
            )
            .into());
        }
        output.flush()?;
        output.sync_all()?;
        if plan.kind.is_pa_document() {
            validate_payslip_pdf(&temporary_path)?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        drop(output);
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    drop(output);
    if plan.kind == PlannedKind::Information
        && plan.source_name.to_ascii_lowercase().ends_with(".pdf")
        && crate::payroll_prep_sheet_import_service::has_prep_sheet_heading(&temporary_path)
    {
        plan.kind = PlannedKind::PrepSheet;
    }
    Ok(StagedEntry {
        plan,
        temporary_path,
    })
}

fn create_temporary_file(directory: &Path) -> Result<(PathBuf, fs::File), Box<dyn Error>> {
    for sequence in 0..1000u32 {
        let name = format!(
            ".direct-payment-payroll-return-{}-{}-{sequence:03}.tmp",
            std::process::id(),
            Local::now().timestamp_nanos_opt().unwrap_or_default()
        );
        let path = directory.join(name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err("Could not allocate a temporary Payroll Return file.".into())
}

pub fn validate_payslip_pdf(path: &Path) -> Result<(), Box<dyn Error>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() == 0 {
        return Err(format!(
            "Payslip is not a non-empty regular file: {}",
            path.display()
        )
        .into());
    }
    let mut file = fs::File::open(path)?;
    let mut prefix = [0u8; 1024];
    let read = file.read(&mut prefix)?;
    if !prefix[..read].windows(5).any(|bytes| bytes == b"%PDF-") {
        return Err(format!(
            "Payslip does not contain a PDF signature: {}",
            path.display()
        )
        .into());
    }
    Ok(())
}

fn files_equal(left: &Path, right: &Path) -> Result<bool, Box<dyn Error>> {
    let right_metadata = fs::symlink_metadata(right)?;
    if !right_metadata.file_type().is_file() || fs::metadata(left)?.len() != right_metadata.len() {
        return Ok(false);
    }
    let mut left = fs::File::open(left)?;
    let mut right = fs::File::open(right)?;
    let mut left_buffer = [0u8; 8192];
    let mut right_buffer = [0u8; 8192];
    loop {
        let left_read = left.read(&mut left_buffer)?;
        let right_read = right.read(&mut right_buffer)?;
        if left_read != right_read || left_buffer[..left_read] != right_buffer[..right_read] {
            return Ok(false);
        }
        if left_read == 0 {
            return Ok(true);
        }
    }
}

fn publish_no_clobber(temporary: &Path, destination: &Path) -> io::Result<()> {
    fs::hard_link(temporary, destination)
}

fn cleanup_staged(entries: &[StagedEntry]) {
    for entry in entries {
        cleanup_staged_entry(entry);
    }
}

fn cleanup_staged_entry(entry: &StagedEntry) {
    let _ = fs::remove_file(&entry.temporary_path);
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
            mileage_enabled: false,
            start_date: None,
            leaving_date: None,
            signature: None,
        }
    }

    fn named_assistant(id: i64, first_name: &str, surname: &str) -> PersonalAssistant {
        let mut assistant = assistant();
        assistant.id = id;
        assistant.first_name = first_name.to_string();
        assistant.surname = surname.to_string();
        assistant
    }

    #[test]
    fn document_only_returns_do_not_require_an_ordinary_payslip() {
        for (name, supplements, information) in [
            ("Provider - P60 for Cedar Fixture.pdf", 1, 0),
            ("Provider - P45 for Cedar Fixture.pdf", 1, 0),
            ("P30 Employer's Payslip.pdf", 0, 1),
            ("General information Cedar Fixture.pdf", 0, 1),
        ] {
            let root = tempfile::TempDir::new().unwrap();
            let zip = root.path().join("return.zip");
            create_zip(&zip, &[(name, b"%PDF-1.4 document")]);
            let result = import_payroll_return(
                &zip,
                &root.path().join("payslips"),
                &root.path().join("information"),
                &[assistant()],
                &schedule("2026/27", "10/08/2026", "04/09/2026"),
            )
            .unwrap();
            assert_eq!(result.payslips_imported, 0);
            assert_eq!(result.supplements_imported, supplements);
            assert_eq!(result.information_files_imported, information);
            assert!(result.publication_failure.is_none());
        }
    }

    #[test]
    fn mixed_provider_return_routes_and_preserves_supplement_names() {
        let root = tempfile::TempDir::new().unwrap();
        let zip = root.path().join("return.zip");
        let p60 =
            "Example Employer - P60 End of Year Summary for year 2025-26 for Cedar_Fixture.pdf";
        let p45 = "Example Employer - P45 Leaving details for Cedar Fixture.pdf";
        let p30 = "Example Employer - P30 Employer's Payslip for Week 48 to 52.pdf";
        let p30_variant = "Example Employer - P30 Employer's Payslip for Week 48 to 52 [1].pdf";
        let general = [
            "Bank-transfer slip for Cedar Fixture.pdf",
            "Quarter-end memo for Cedar Fixture.pdf",
            "Payroll prep sheet for Cedar Fixture.pdf",
            "Unknown report for Cedar Fixture.pdf",
        ];
        let mut entries = vec![
            (
                "Payslip Cedar Fixture.pdf",
                b"%PDF-1.4 payslip".as_slice(),
            ),
            (p60, b"%PDF-1.4 p60".as_slice()),
            (p45, b"%PDF-1.4 p45".as_slice()),
            (p30, b"%PDF-1.4 p30".as_slice()),
            (p30_variant, b"%PDF-1.4 variant".as_slice()),
        ];
        entries.extend(
            general
                .iter()
                .map(|name| (*name, b"%PDF-1.4 info".as_slice())),
        );
        create_zip(&zip, &entries);
        let payslips = root.path().join("payslips");
        let information = root.path().join("information");
        let period = schedule("2026/27", "10/08/2026", "04/09/2026");
        // Include the employer as a maintained PA: P30 wins over name/payslip cues.
        let pas = [assistant(), named_assistant(2, "Morgan", "Worsdall")];
        // The employer prefix is not an employee identifier in this fixture's P60/P45.
        // Use the actual PA list for supplements, then test P30 independently below.
        let result =
            import_payroll_return(&zip, &payslips, &information, &pas[..1], &period).unwrap();
        assert_eq!(result.payslips_imported, 1);
        assert_eq!(result.supplements_imported, 2);
        assert_eq!(result.information_files_imported, 6);
        assert!(result.publication_failure.is_none());
        let ordinary =
            crate::payroll_file_naming::payslip_path(&payslips, "Cedar Fixture", &period)
                .unwrap();
        assert!(ordinary.ends_with("2026 to 2027/Payslip for Week 22 for Cedar Fixture.pdf"));
        assert_eq!(fs::read(ordinary).unwrap(), b"%PDF-1.4 payslip");
        assert!(payslips.join("2025 to 2026/PA 1/P60 End of Year Summary for year 2025-26 for Cedar_Fixture.pdf").is_file());
        assert!(payslips
            .join("PA 1/P45 Leaving details for Cedar Fixture.pdf")
            .is_file());
        let info_year = information.clone();
        for name in general.iter().copied().chain([p30, p30_variant]) {
            assert!(info_year.join(name).is_file());
        }
        create_zip(
            &zip,
            &[(p30, b"%PDF-1.4 p30"), (p30_variant, b"%PDF-1.4 variant")],
        );
        let result = import_payroll_return(&zip, &payslips, &information, &pas, &period).unwrap();
        assert_eq!(result.information_files_imported, 0);
        assert_eq!(result.information_files_already_present, 2);
        assert!(!info_year
            .join("Example Employer - P30 Employer's Payslip for Week 48 to 52 (2).pdf")
            .exists());
        assert!(!info_year
            .join("Example Employer - P30 Employer's Payslip for Week 48 to 52 [1] (2).pdf")
            .exists());
        assert!(!payslips
            .join("2026 to 2027/Payroll return cycle 6")
            .exists());
    }

    #[test]
    fn supplements_preserve_pa_identity_when_name_precedes_type_and_do_not_mutate_employment() {
        for token in ["P60", "P45"] {
            let root = tempfile::TempDir::new().unwrap();
            let zip = root.path().join("return.zip");
            let mut pa = assistant();
            pa.employment_status = Some("Inactive".into());
            pa.leaving_date = Some("01/01/2025".into());
            let before = format!("{pa:?}");
            let filename = format!("Cedar Fixture - {token} Provider details.pdf");
            create_zip(&zip, &[(&filename, b"%PDF-1.4 document")]);
            let period = schedule("2026/27", "10/08/2026", "04/09/2026");
            let result = import_payroll_return(
                &zip,
                root.path(),
                &root.path().join("info"),
                std::slice::from_ref(&pa),
                &period,
            )
            .unwrap();
            assert_eq!(before, format!("{pa:?}"));
            assert_eq!(result.supplements_imported, 1);
            assert!(root
                .path()
                .join(format!(
                    "PA {}/{token} Provider details for Cedar Fixture.pdf",
                    pa.id
                ))
                .is_file());
            assert!(!root.path().join("PA 999").exists());
            let mut other_year = period.clone();
            other_year.payroll_year = "2027/28".into();
            other_year.cycle_number += 1;
            assert_eq!(
                import_payroll_return(
                    &zip,
                    root.path(),
                    &root.path().join("info"),
                    &[pa],
                    &other_year
                )
                .unwrap()
                .supplements_already_present,
                1
            );
        }
    }

    #[test]
    fn ambiguous_and_unmatched_supplements_refuse_before_publication() {
        for token in ["P60", "P45"] {
            for name in ["Alex Smith", "Nobody Here"] {
                let root = tempfile::TempDir::new().unwrap();
                let zip = root.path().join("return.zip");
                let filename = format!("{token} for {name}.pdf");
                create_zip(
                    &zip,
                    &[("general.pdf", b"%PDF-1.4"), (&filename, b"%PDF-1.4")],
                );
                assert!(import_payroll_return(
                    &zip,
                    &root.path().join("payslips"),
                    &root.path().join("info"),
                    &[
                        named_assistant(1, "Alex", "Smith"),
                        named_assistant(2, "alex", "smith")
                    ],
                    &schedule("2026/27", "10/08/2026", "04/09/2026")
                )
                .is_err());
                assert!(!root.path().join("payslips").exists());
                assert!(!root.path().join("info").exists());
            }
        }
    }

    #[test]
    fn supplement_reimport_is_identical_or_refused_without_overwrite() {
        for token in ["P60", "P45"] {
            let root = tempfile::TempDir::new().unwrap();
            let zip = root.path().join("return.zip");
            let filename = format!("Provider - {token} for Cedar Fixture.pdf");
            let period = schedule("2026/27", "10/08/2026", "04/09/2026");
            create_zip(&zip, &[(&filename, b"%PDF-1.4 original")]);
            let run = || {
                import_payroll_return(
                    &zip,
                    root.path(),
                    &root.path().join("info"),
                    &[assistant()],
                    &period,
                )
            };
            assert_eq!(run().unwrap().supplements_imported, 1);
            assert_eq!(run().unwrap().supplements_already_present, 1);
            let path = root
                .path()
                .join(format!("PA 1/{token} for Cedar Fixture.pdf"));
            create_zip(
                &zip,
                &[
                    ("general.pdf", b"%PDF-1.4"),
                    (&filename, b"%PDF-1.4 changed"),
                ],
            );
            assert!(run().is_err());
            assert_eq!(fs::read(path).unwrap(), b"%PDF-1.4 original");
            assert!(!root.path().join("info/general.pdf").exists());
        }
    }

    #[test]
    fn general_pdf_with_multiple_pa_names_is_information_not_ambiguous_payslip() {
        let root = tempfile::TempDir::new().unwrap();
        let zip = root.path().join("return.zip");
        create_zip(&zip, &[("Report Alex Smith and Jo Jones.pdf", b"%PDF-1.4")]);
        let result = import_payroll_return(
            &zip,
            &root.path().join("payslips"),
            &root.path().join("info"),
            &[
                named_assistant(1, "Alex", "Smith"),
                named_assistant(2, "Jo", "Jones"),
            ],
            &schedule("2026/27", "10/08/2026", "04/09/2026"),
        )
        .unwrap();
        assert_eq!(result.information_files_imported, 1);
        assert_eq!(result.payslips_imported, 0);
    }

    fn create_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, contents) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap();
    }

    fn create_return_zip(path: &Path, payslip_contents: &str, information_names: &[&str]) {
        let file = File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("Cedar Fixture.pdf", options).unwrap();
        writer.write_all(b"%PDF-1.4\n").unwrap();
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
    fn csv_archives_use_exact_bytes_and_never_overwrite() {
        let root = test_root("csv-no-overwrite");
        let source = root.join("source.csv");
        let archive_dir = root.join("archive");
        fs::create_dir_all(&root).unwrap();
        fs::write(&source, b"source changed after read").unwrap();
        let captured = b"captured original bytes";

        let first = archive_csv_bytes(&source, captured, &archive_dir).unwrap();
        let second = archive_csv_bytes(&source, captured, &archive_dir).unwrap();

        assert_ne!(first, second);
        assert_eq!(fs::read(first).unwrap(), captured);
        assert_eq!(fs::read(second).unwrap(), captured);
        fs::remove_dir_all(root).unwrap();
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
        assert_eq!(
            fs::read_to_string(first_path).unwrap(),
            "%PDF-1.4\nfirst year"
        );
        assert_eq!(
            fs::read_to_string(second_path).unwrap(),
            "%PDF-1.4\nsecond year"
        );

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

        let year = information_root.clone();
        assert!(year.join("Bulletin.pdf").exists());
        assert!(!year.join("Bulletin (2).pdf").exists());
        assert!(!email_archive.exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn token_matching_is_boundary_safe_and_supports_separators_and_case() {
        let assistants = vec![
            named_assistant(1, "Ann", "Lee"),
            named_assistant(2, "Joann", "Lee"),
        ];
        let normalized = normalized_assistants(&assistants).unwrap();
        let matches = matching_assistants("PAYSLIP-Joann_Lee.pdf", &normalized);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].0.id, 2);
        assert!(matching_assistants("Payslip Joann Leeming.pdf", &normalized).is_empty());
    }

    #[test]
    fn overlapping_and_duplicate_pa_names_are_refused() {
        let root = test_root("ambiguous-pa");
        fs::create_dir_all(&root).unwrap();
        let zip_path = root.join("return.zip");
        create_zip(&zip_path, &[("Payslip John Alex Smith.pdf", b"%PDF-1.4\n")]);
        let assistants = vec![
            named_assistant(1, "Alex", "Smith"),
            named_assistant(2, "John Alex", "Smith"),
        ];
        let error = import_payroll_return(
            &zip_path,
            &root.join("payslips"),
            &root.join("information"),
            &assistants,
            &schedule("2026/27", "10/08/2026", "04/09/2026"),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("more than one Personal Assistant"));

        let duplicates = vec![
            named_assistant(1, "Alex", "Smith"),
            named_assistant(2, "alex", "smith"),
        ];
        let duplicate_zip = root.join("duplicate-name.zip");
        create_zip(&duplicate_zip, &[("Payslip Alex Smith.pdf", b"%PDF-1.4\n")]);
        assert!(import_payroll_return(
            &duplicate_zip,
            &root.join("payslips"),
            &root.join("information"),
            &duplicates,
            &schedule("2026/27", "10/08/2026", "04/09/2026"),
        )
        .is_err());
        assert!(!root.join("payslips").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unmatched_explicit_payslip_is_refused_before_publication() {
        let root = test_root("weak-pdf");
        fs::create_dir_all(&root).unwrap();
        let zip_path = root.join("return.zip");
        create_zip(
            &zip_path,
            &[
                ("Payslip Unknown Person.pdf", b"%PDF-1.4\n"),
                ("safe.txt", b"safe"),
            ],
        );
        assert!(import_payroll_return(
            &zip_path,
            &root.join("payslips"),
            &root.join("information"),
            &[assistant()],
            &schedule("2026/27", "10/08/2026", "04/09/2026"),
        )
        .is_err());
        assert!(!root.join("payslips").exists());
        assert!(!root.join("information").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn two_payslips_for_one_pa_are_refused_before_publication() {
        let root = test_root("duplicate-payslip");
        fs::create_dir_all(&root).unwrap();
        let zip_path = root.join("return.zip");
        create_zip(
            &zip_path,
            &[
                ("Payslip Cedar Fixture.pdf", b"%PDF-1.4\none"),
                ("Cedar Fixture.pdf", b"%PDF-1.4\ntwo"),
            ],
        );
        assert!(import_payroll_return(
            &zip_path,
            &root.join("payslips"),
            &root.join("information"),
            &[assistant()],
            &schedule("2026/27", "10/08/2026", "04/09/2026"),
        )
        .is_err());
        assert!(!root.join("payslips").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn exact_duplicate_archive_names_are_refused_before_publication() {
        let root = test_root("duplicate-entry-name");
        fs::create_dir_all(&root).unwrap();
        let zip_path = root.join("return.zip");
        let file = File::create(&zip_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("Bulletin1.txt", options).unwrap();
        writer.write_all(b"first").unwrap();
        writer.start_file("Bulletin2.txt", options).unwrap();
        writer.write_all(b"second").unwrap();
        writer.finish().unwrap();
        let mut bytes = fs::read(&zip_path).unwrap();
        for position in 0..=bytes.len() - b"Bulletin2.txt".len() {
            if &bytes[position..position + b"Bulletin2.txt".len()] == b"Bulletin2.txt" {
                bytes[position..position + b"Bulletin2.txt".len()]
                    .copy_from_slice(b"Bulletin1.txt");
            }
        }
        fs::write(&zip_path, bytes).unwrap();
        assert!(import_payroll_return(
            &zip_path,
            &root.join("payslips"),
            &root.join("information"),
            &[assistant()],
            &schedule("2026/27", "10/08/2026", "04/09/2026"),
        )
        .is_err());
        assert!(!root.join("information").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn identical_existing_payslip_is_idempotent_but_different_content_is_refused() {
        let root = test_root("existing-payslip");
        fs::create_dir_all(&root).unwrap();
        let schedule = schedule("2026/27", "10/08/2026", "04/09/2026");
        let payslip_root = root.join("payslips");
        let information_root = root.join("information");
        let first_zip = root.join("first.zip");
        create_zip(
            &first_zip,
            &[("Cedar Fixture.pdf", b"%PDF-1.4\noriginal")],
        );
        import_payroll_return(
            &first_zip,
            &payslip_root,
            &information_root,
            &[assistant()],
            &schedule,
        )
        .unwrap();
        let destination =
            crate::payroll_file_naming::payslip_path(&payslip_root, "Cedar Fixture", &schedule)
                .unwrap();
        let original = fs::read(&destination).unwrap();

        let unchanged = import_payroll_return(
            &first_zip,
            &payslip_root,
            &information_root,
            &[assistant()],
            &schedule,
        )
        .unwrap();
        assert_eq!(unchanged.payslips_already_present, 1);
        assert_eq!(unchanged.payslips_imported, 0);

        let changed_zip = root.join("changed.zip");
        create_zip(
            &changed_zip,
            &[("Cedar Fixture.pdf", b"%PDF-1.4\nchanged")],
        );
        assert!(import_payroll_return(
            &changed_zip,
            &payslip_root,
            &information_root,
            &[assistant()],
            &schedule,
        )
        .is_err());
        assert_eq!(fs::read(destination).unwrap(), original);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_pdf_is_rejected_and_temporary_file_is_cleaned() {
        let root = test_root("invalid-pdf");
        fs::create_dir_all(&root).unwrap();
        let zip_path = root.join("return.zip");
        create_zip(&zip_path, &[("Cedar Fixture.pdf", b"not a pdf")]);
        let payslip_root = root.join("payslips");
        assert!(import_payroll_return(
            &zip_path,
            &payslip_root,
            &root.join("information"),
            &[assistant()],
            &schedule("2026/27", "10/08/2026", "04/09/2026"),
        )
        .is_err());
        let year = payslip_root.join("2026 to 2027");
        assert!(year.exists());
        assert_eq!(fs::read_dir(year).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn zero_byte_payslip_and_malformed_zip_are_refused() {
        let root = test_root("invalid-archives");
        fs::create_dir_all(&root).unwrap();
        let empty_zip = root.join("empty-pdf.zip");
        create_zip(&empty_zip, &[("Cedar Fixture.pdf", b"")]);
        assert!(import_payroll_return(
            &empty_zip,
            &root.join("payslips"),
            &root.join("information"),
            &[assistant()],
            &schedule("2026/27", "10/08/2026", "04/09/2026"),
        )
        .is_err());
        let malformed = root.join("malformed.zip");
        fs::write(&malformed, b"not a zip").unwrap();
        assert!(import_payroll_return(
            &malformed,
            &root.join("payslips-2"),
            &root.join("information-2"),
            &[assistant()],
            &schedule("2026/27", "10/08/2026", "04/09/2026"),
        )
        .is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unsafe_archive_paths_are_rejected_portably() {
        for name in [
            "../evil.txt",
            "/absolute.txt",
            "C:\\evil.txt",
            "\\\\server\\share.txt",
        ] {
            assert!(
                validate_archive_name(name, name.as_bytes()).is_err(),
                "{name}"
            );
        }
        assert!(validate_archive_name(
            "provider/folder/Bulletin.txt",
            b"provider/folder/Bulletin.txt"
        )
        .is_ok());
    }

    #[test]
    fn symlink_entries_are_refused_before_publication() {
        let root = test_root("symlink-entry");
        fs::create_dir_all(&root).unwrap();
        let zip_path = root.join("return.zip");
        let file = File::create(&zip_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        writer.add_symlink("link", "target", options).unwrap();
        writer.finish().unwrap();
        let result = import_payroll_return(
            &zip_path,
            &root.join("payslips"),
            &root.join("information"),
            &[assistant()],
            &schedule("2026/27", "10/08/2026", "04/09/2026"),
        );
        assert!(result.is_err());
        assert!(!root.join("information").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn no_clobber_publication_refuses_a_racing_destination() {
        let root = test_root("no-clobber");
        fs::create_dir_all(&root).unwrap();
        let temporary = root.join("temporary");
        let destination = root.join("destination");
        fs::write(&temporary, b"new evidence").unwrap();
        fs::write(&destination, b"existing evidence").unwrap();
        assert_eq!(
            publish_no_clobber(&temporary, &destination)
                .unwrap_err()
                .kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read(&destination).unwrap(), b"existing evidence");
        assert_eq!(fs::read(&temporary).unwrap(), b"new evidence");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn truncated_zip_is_refused_without_destination_creation() {
        let root = test_root("truncated");
        fs::create_dir_all(&root).unwrap();
        let zip_path = root.join("return.zip");
        create_zip(&zip_path, &[("Cedar Fixture.pdf", b"%PDF-1.4\ncontent")]);
        let length = fs::metadata(&zip_path).unwrap().len();
        fs::OpenOptions::new()
            .write(true)
            .open(&zip_path)
            .unwrap()
            .set_len(length / 2)
            .unwrap();
        assert!(import_payroll_return(
            &zip_path,
            &root.join("payslips"),
            &root.join("information"),
            &[assistant()],
            &schedule("2026/27", "10/08/2026", "04/09/2026"),
        )
        .is_err());
        assert!(!root.join("payslips").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn entry_size_and_compression_ratio_limits_are_enforced() {
        assert!(validate_entry_sizes("large", 1, MAX_PAYROLL_RETURN_ENTRY_BYTES + 1).is_err());
        assert!(
            validate_entry_sizes("ratio", 1, MAX_PAYROLL_RETURN_COMPRESSION_RATIO + 1).is_err()
        );
        assert!(validate_entry_sizes("normal", 100, 1000).is_ok());
        assert!(checked_total_size(MAX_PAYROLL_RETURN_TOTAL_BYTES, 1).is_err());
        assert!(checked_total_size(u64::MAX, 1).is_err());
    }

    #[test]
    fn entry_count_limit_is_enforced_before_destination_creation() {
        let root = test_root("entry-count");
        fs::create_dir_all(&root).unwrap();
        let zip_path = root.join("return.zip");
        let file = File::create(&zip_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for index in 0..=MAX_PAYROLL_RETURN_ENTRIES {
            writer
                .start_file(format!("info-{index}.txt"), options)
                .unwrap();
        }
        writer.finish().unwrap();
        assert!(import_payroll_return(
            &zip_path,
            &root.join("payslips"),
            &root.join("information"),
            &[assistant()],
            &schedule("2026/27", "10/08/2026", "04/09/2026"),
        )
        .is_err());
        assert!(!root.join("information").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn non_pdf_pa_file_is_skipped_and_information_names_are_reserved_as_a_batch() {
        let root = test_root("information-batch");
        fs::create_dir_all(&root).unwrap();
        let zip_path = root.join("return.zip");
        create_zip(
            &zip_path,
            &[
                ("Cedar Fixture.txt", b"skip"),
                ("one/Bulletin.txt", b"one"),
                ("two/Bulletin.txt", b"two"),
            ],
        );
        let result = import_payroll_return(
            &zip_path,
            &root.join("payslips"),
            &root.join("information"),
            &[assistant()],
            &schedule("2026/27", "10/08/2026", "04/09/2026"),
        )
        .unwrap();
        assert_eq!(result.files_skipped, 1);
        assert_eq!(result.information_files_imported, 2);
        let year = root.join("information");
        assert_eq!(
            fs::read_to_string(year.join("Bulletin.txt")).unwrap(),
            "one"
        );
        assert_eq!(
            fs::read_to_string(year.join("Bulletin (2).txt")).unwrap(),
            "two"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn payslip_attachment_validation_requires_regular_nonempty_pdf() {
        let root = test_root("attachment-validation");
        fs::create_dir_all(&root).unwrap();
        let valid = root.join("valid.pdf");
        fs::write(&valid, b"prefix\n%PDF-1.7\n").unwrap();
        assert!(validate_payslip_pdf(&valid).is_ok());
        let invalid = root.join("invalid.pdf");
        fs::write(&invalid, b"plain text").unwrap();
        assert!(validate_payslip_pdf(&invalid).is_err());
        assert!(validate_payslip_pdf(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
