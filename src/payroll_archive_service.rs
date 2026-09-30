//! Explicit PA filing operations. Publish verified copies before committing paths;
//! remove originals only after commit, using a durable, restartable cleanup journal.
use crate::{
    app::Application, models::PersonalAssistant, payroll_document_repository::file_digest,
    payroll_file_naming as naming, personal_assistant_repository::PersonalAssistantRepository,
};
use rusqlite::{params, Connection};
use std::{
    collections::BTreeMap,
    error::Error,
    ffi::OsStr,
    fs,
    path::{Component, Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
#[derive(Debug)]
struct Move {
    source: PathBuf,
    destination: PathBuf,
    digest: String,
    document_id: Option<i64>,
}

pub(crate) fn checked_path(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path.components().any(|c| {
            matches!(c, Component::ParentDir | Component::CurDir)
                || c.as_os_str() == OsStr::new(".")
                || c.as_os_str() == OsStr::new("..")
        })
    {
        return Err(format!("Unsafe payroll path: {}", path.display()).into());
    }
    let mut prefix = PathBuf::new();
    for part in path.components() {
        prefix.push(part);
        // Windows drive/verbatim prefixes are not complete filesystem paths
        // until their RootDir component has been appended.
        if matches!(part, Component::Prefix(_)) {
            continue;
        }
        match fs::symlink_metadata(&prefix) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!("Symlink payroll path refused: {}", prefix.display()).into())
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
/// Compare stored paths and configured roots consistently, including Windows verbatim prefixes.
pub(crate) fn normalised_path(path: &Path) -> Result<PathBuf> {
    checked_path(path)?;
    for ancestor in path.ancestors() {
        if ancestor.try_exists()? {
            return Ok(ancestor.canonicalize()?.join(path.strip_prefix(ancestor)?));
        }
    }
    Err("No existing ancestor for managed path".into())
}

pub(crate) fn filing_base(root: &Path) -> Result<PathBuf> {
    let base = naming::root_without_payroll_year_suffix(root);
    checked_path(base)?;
    // Registry paths come from canonicalize(), including Windows verbatim
    // prefixes. Construct destinations in the same path representation.
    Ok(if base.exists() {
        base.canonicalize()?
    } else {
        base.to_path_buf()
    })
}

pub(crate) fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    fs::File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub(crate) fn verified(path: &Path, digest: &str) -> Result<()> {
    checked_path(path)?;
    crate::archive::validate_payslip_pdf(path)?;
    if file_digest(path)? != digest {
        return Err(format!("Payroll file changed: {}", path.display()).into());
    }
    Ok(())
}
fn key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}
fn errors(errors: Vec<String>) -> Result<()> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n").into())
    }
}

fn year_directories(base: &Path) -> Result<Vec<PathBuf>> {
    let mut result = vec![base.to_path_buf()];
    if base.try_exists()? {
        for entry in fs::read_dir(base)
            .map_err(|e| format!("Cannot scan payroll base {}: {e}", base.display()))?
        {
            let entry = entry?;
            if naming::is_payroll_year_directory_name(&entry.file_name().to_string_lossy()) {
                checked_path(&entry.path())?;
                if entry.file_type()?.is_dir() {
                    result.push(entry.path());
                }
            }
        }
    }
    result.sort();
    Ok(result)
}

fn plan(
    app: &Application,
    pa: &PersonalAssistant,
    previous: &PersonalAssistant,
    archive_ordinary: bool,
    rename: bool,
) -> Result<Vec<Move>> {
    let root = crate::paths::expand_path(&app.context.config.folders.payslip_folder);
    let base = filing_base(&root)?;
    let base = base.as_path();
    let mut moves = Vec::new();
    let mut failures = Vec::new();
    let mut possible_published_copies = Vec::new();
    if rename {
        let mut final_assistants = app.personal_assistant_repository.get_all()?;
        for assistant in &mut final_assistants {
            if assistant.id == pa.id {
                *assistant = pa.clone();
            }
        }
        let canonical = format!(
            "Payslip for {}.pdf",
            naming::sanitise_filename(&format!("{} {}", pa.first_name.trim(), pa.surname.trim()))
        );
        if crate::archive::ordinary_payslip_owner(&canonical, &final_assistants)? != Some(pa.id) {
            return Err(
                "New maintained name does not uniquely identify this PA; filing refused".into(),
            );
        }
    }
    let documents = app
        .payroll_timesheet_email_repository
        .all_documents_for_pa(pa.id)?;
    for doc in documents {
        let attempt = (|| -> Result<()> {
            verified(&doc.path, &doc.sha256)?;
            let legacy = naming::independent_pa_document_directory(
                base,
                pa.id,
                doc.document_year.as_deref(),
            )?;
            let mut filing_pa = pa.clone();
            if !archive_ordinary {
                filing_pa.employment_status = Some("Active".into());
            }
            let destination = naming::supplement_path(
                base,
                &filing_pa,
                &doc.document_type,
                doc.document_year.as_deref(),
            )?
            .with_file_name(crate::archive::clean_owned_document_filename(
                doc.path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .ok_or("Invalid registered filename")?,
                &doc.document_type,
                pa,
                previous,
            ));
            // Registry identity + verified content + an application-owned directory.
            // Never reorganise arbitrary files elsewhere on the user's computer.
            let year_dir = legacy.parent().unwrap();
            let parent = doc
                .path
                .parent()
                .ok_or("Registered document has no parent")?;
            if parent != legacy && parent != year_dir && parent != year_dir.join("Archived") {
                return Err("Registered supplement is outside its expected payroll filing area; no automatic move was made".into());
            }
            // Reactivation must not move an already Archived document back out.
            let destination = if parent == year_dir.join("Archived") {
                year_dir
                    .join("Archived")
                    .join(destination.file_name().unwrap())
            } else {
                destination
            };
            if doc.path != destination {
                moves.push(Move {
                    source: doc.path.clone(),
                    destination,
                    digest: doc.sha256,
                    document_id: Some(doc.id),
                });
            }
            Ok(())
        })();
        if let Err(e) = attempt {
            failures.push(format!("{}: {e}", doc.path.display()));
        }
    }
    // Durable identities take precedence over filename inference, including superseded revisions.
    let db = &app.payroll_timesheet_email_repository.connection;
    let revisions = db.prepare("SELECT stored_path,sha256,payroll_year FROM payslip_revisions WHERE personal_assistant_id=?1")?.query_map([pa.id], |r|Ok((PathBuf::from(r.get::<_,String>(0)?),r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let revision_paths: Vec<_> = revisions
        .iter()
        .map(|r| normalised_path(&r.0))
        .collect::<Result<_>>()?;
    if archive_ordinary {
        for (source, digest, year) in revisions {
            verified(&source, &digest)?;
            let year_dir = base.join(naming::payroll_year_directory_name(&year)?);
            let parent = normalised_path(source.parent().ok_or("Revision has no parent")?)?;
            if parent != year_dir && parent != year_dir.join("Archived") {
                return Err("Payslip revision outside managed year directory".into());
            }
            let destination = year_dir.join("Archived").join(source.file_name().unwrap());
            if normalised_path(&source)? != normalised_path(&destination)? {
                moves.push(Move {
                    source,
                    destination,
                    digest,
                    document_id: None,
                });
            }
        }
        plan_timesheets(app, pa, &mut moves)?;
    }
    if archive_ordinary || rename {
        let assistants = app.personal_assistant_repository.get_all()?;
        let mut final_assistants = assistants.clone();
        if let Some(saved) = final_assistants.iter_mut().find(|p| p.id == pa.id) {
            *saved = pa.clone();
        }
        for year in year_directories(base)? {
            let mut directories = vec![year.clone(), year.join(format!("PA {}", pa.id))];
            if rename {
                directories.push(year.join("Archived"));
            }
            for directory in directories {
                if !directory.try_exists().map_err(|e| {
                    format!(
                        "Cannot inspect payroll directory {}: {e}",
                        directory.display()
                    )
                })? {
                    continue;
                }
                checked_path(&directory)?;
                let mut entries = fs::read_dir(&directory)
                    .map_err(|e| {
                        format!("Cannot scan payroll directory {}: {e}", directory.display())
                    })?
                    .collect::<std::io::Result<Vec<_>>>()?;
                entries.sort_by_key(|e| e.file_name());
                for entry in entries {
                    if moves.iter().any(|m| m.source == entry.path())
                        || revision_paths.contains(&entry.path())
                    {
                        continue;
                    }
                    let name = entry.file_name().to_string_lossy().to_string();
                    let pa_directory = directory == year.join(format!("PA {}", pa.id));
                    if !name.to_ascii_lowercase().ends_with(".pdf")
                        || (!pa_directory && !name.to_ascii_lowercase().contains("payslip"))
                    {
                        continue;
                    }
                    let attempt = (|| -> Result<()> {
                        let owner = if pa_directory {
                            crate::archive::classify_filename(&name, &assistants).map(
                                |(kind, id)| {
                                    if kind == crate::archive::PlannedKind::Payslip {
                                        id
                                    } else {
                                        None
                                    }
                                },
                            )
                        } else {
                            crate::archive::ordinary_payslip_owner(&name, &assistants)
                        };
                        match owner {
                            Ok(Some(id)) if id == pa.id => {}
                            Err(_)
                                if rename
                                    && crate::archive::ordinary_payslip_owner(
                                        &name,
                                        &final_assistants,
                                    )
                                    .ok()
                                        == Some(Some(pa.id)) =>
                            {
                                // An interrupted pre-commit rename may have left a
                                // new-name copy. Do not infer ownership from that
                                // name: require an independently owned old source
                                // targeting it, with identical bytes checked below.
                                possible_published_copies.push(entry.path());
                                return Ok(());
                            }
                            Err(error) => return Err(error),
                            _ => return Ok(()),
                        }
                        let source = entry.path();
                        checked_path(&source)?;
                        crate::archive::validate_payslip_pdf(&source)?;
                        let unassociated = directory == year.join(format!("PA {}", pa.id))
                            || name.starts_with("Unassociated - ");
                        let filename = if rename {
                            crate::archive::clean_owned_document_filename(
                                &name, "payslip", pa, previous,
                            )
                        } else {
                            name.strip_prefix("Unassociated - ")
                                .unwrap_or(&name)
                                .to_string()
                        };
                        // A new name must not make another maintained PA a possible
                        // owner. Source ownership was checked against saved names.
                        let final_owner = if pa_directory && !rename {
                            Some(pa.id)
                        } else {
                            crate::archive::ordinary_payslip_owner(&filename, &final_assistants)?
                        };
                        if final_owner != Some(pa.id) {
                            return Err("Renamed payslip does not uniquely identify this PA".into());
                        }
                        let target_directory = if archive_ordinary {
                            year.join("Archived")
                        } else {
                            directory.clone()
                        };
                        let filename = if unassociated && target_directory == year.join("Archived")
                        {
                            format!("Unassociated - {filename}")
                        } else {
                            filename
                        };
                        let destination = target_directory.join(filename);
                        if source == destination {
                            return Ok(());
                        }
                        moves.push(Move {
                            digest: file_digest(&source)?,
                            source,
                            destination,
                            document_id: None,
                        });
                        Ok(())
                    })();
                    if let Err(e) = attempt {
                        failures.push(format!("{}: {e}", entry.path().display()));
                    }
                }
            }
        }
    }
    for path in possible_published_copies {
        if !moves.iter().any(|m| m.destination == path) {
            failures.push(format!("{}: new-name file has no independently identified old-name source; ownership was not assumed", path.display()));
        }
    }
    moves.sort_by(|a, b| a.source.cmp(&b.source));
    let mut targets: BTreeMap<String, Vec<&Move>> = BTreeMap::new();
    for m in &moves {
        targets.entry(key(&m.destination)).or_default().push(m);
    }
    for group in targets.values().filter(|g| g.len() > 1) {
        for m in group {
            failures.push(format!(
                "{}: multiple sources target {}; none moved",
                m.source.display(),
                m.destination.display()
            ));
        }
    }
    for m in &moves {
        let attempt = (|| -> Result<()> {
            for path in [&m.source, &m.destination] {
                let foreign:bool=app.payroll_timesheet_email_repository.connection.query_row("SELECT EXISTS(SELECT 1 FROM imported_payroll_documents WHERE stored_path=?1 AND personal_assistant_id<>?2 UNION SELECT 1 FROM payslip_revisions WHERE stored_path=?1 AND personal_assistant_id<>?2 UNION SELECT 1 FROM payroll_timesheet_snapshot_states s JOIN payroll_timesheets p ON p.id=s.payroll_timesheet_id WHERE s.pdf_path=?1 AND p.personal_assistant_id<>?2 UNION SELECT 1 FROM payroll_submissions s JOIN payroll_timesheets p ON p.id=s.payroll_timesheet_id WHERE s.pdf_path=?1 AND p.personal_assistant_id<>?2)",params![path.to_str(),pa.id],|r|r.get(0))?;
                if foreign {
                    return Err("Filing path is referenced by another PA".into());
                }
            }
            checked_path(&m.destination)?;
            if let Some(parent) = m.destination.parent().filter(|p| p.exists()) {
                for entry in fs::read_dir(parent)? {
                    let existing = entry?.path();
                    if key(&existing) == key(&m.destination) {
                        if existing != m.destination {
                            return Err("Case-insensitive destination collision".into());
                        }
                        verified(&existing, &m.digest)?;
                    }
                }
            }
            let registered = app
                .payroll_timesheet_email_repository
                .connection
                .prepare("SELECT id,stored_path FROM imported_payroll_documents")?
                .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?
                .iter()
                .any(|(id, path)| {
                    Some(*id) != m.document_id && key(Path::new(path)) == key(&m.destination)
                });
            if registered {
                return Err("Destination belongs to another registered document".into());
            }
            Ok(())
        })();
        if let Err(e) = attempt {
            failures.push(format!(
                "{} -> {}: {e}",
                m.source.display(),
                m.destination.display()
            ));
        }
    }
    errors(failures)?;
    Ok(moves)
}

fn managed_bases(app: &Application) -> Result<Vec<PathBuf>> {
    [
        &app.context.config.folders.payslip_folder,
        &app.context.config.folders.pdf_output,
    ]
    .into_iter()
    .map(|p| filing_base(&crate::paths::expand_path(p)))
    .collect()
}

fn plan_timesheets(app: &Application, pa: &PersonalAssistant, moves: &mut Vec<Move>) -> Result<()> {
    let base = filing_base(&crate::paths::expand_path(
        &app.context.config.folders.pdf_output,
    ))?;
    let db = &app.payroll_timesheet_email_repository.connection;
    let mut stmt=db.prepare("SELECT s.pdf_path,s.pdf_sha256,p.payroll_year FROM payroll_timesheet_snapshot_states s JOIN payroll_timesheets p ON p.id=s.payroll_timesheet_id WHERE p.personal_assistant_id=?1 UNION SELECT s.pdf_path,s.pdf_sha256,p.payroll_year FROM payroll_submissions s JOIN payroll_timesheets p ON p.id=s.payroll_timesheet_id WHERE p.personal_assistant_id=?1")?;
    let records = stmt
        .query_map([pa.id], |r| {
            Ok((
                PathBuf::from(r.get::<_, String>(0)?),
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut seen = std::collections::HashSet::new();
    for (source, digest, year) in records {
        // Historical submissions may retain older bytes only in the database after regeneration.
        if !source.try_exists()? {
            continue;
        }
        if !normalised_path(&source)?.starts_with(&base) {
            return Err(format!(
                "Timesheet is outside managed PDF root: {}",
                source.display()
            )
            .into());
        }
        checked_path(&source)?;
        let actual = file_digest(&source)?;
        let current_mismatch:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_timesheet_snapshot_states WHERE pdf_path=?1 AND pdf_sha256<>?2)",params![source.to_str(),actual],|r|r.get(0))?;
        if current_mismatch {
            return Err(format!("Current timesheet has changed: {}", source.display()).into());
        }
        if actual != digest {
            let known:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_submissions WHERE pdf_path=?1 AND pdf_sha256=?2 UNION SELECT 1 FROM payroll_timesheet_snapshot_states WHERE pdf_path=?1 AND pdf_sha256=?2)",params![source.to_str(),actual],|r|r.get(0))?;
            if !known {
                return Err(
                    format!("Historical timesheet has changed: {}", source.display()).into(),
                );
            }
            continue;
        }
        if !seen.insert(normalised_path(&source)?) {
            continue;
        }
        let shared:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_timesheet_snapshot_states s JOIN payroll_timesheets p ON p.id=s.payroll_timesheet_id WHERE s.pdf_path=?1 AND p.personal_assistant_id<>?2 UNION SELECT 1 FROM payroll_submissions s JOIN payroll_timesheets p ON p.id=s.payroll_timesheet_id WHERE s.pdf_path=?1 AND p.personal_assistant_id<>?2)",params![source.to_str(),pa.id],|r|r.get(0))?;
        if shared {
            return Err("Timesheet path belongs to more than one PA".into());
        }
        verified(&source, &digest)?;
        let destination = base
            .join(naming::payroll_year_directory_name(&year)?)
            .join("Archived")
            .join(source.file_name().ok_or("Timesheet filename missing")?);
        if normalised_path(&source)? != normalised_path(&destination)? {
            moves.push(Move {
                source,
                destination,
                digest,
                document_id: None,
            });
        }
    }
    // Unregistered generated PDFs: accept only exact application filenames for this PA and a stored schedule.
    let assistants = app.personal_assistant_repository.get_all()?;
    for schedule in app.payroll_schedule_repository.get_all()? {
        let name =
            naming::timesheet_filename(&format!("{} {}", pa.first_name, pa.surname), &schedule)?;
        if assistants
            .iter()
            .filter(|p| {
                naming::timesheet_filename(&format!("{} {}", p.first_name, p.surname), &schedule)
                    .ok()
                    .as_deref()
                    == Some(name.as_str())
            })
            .count()
            != 1
        {
            return Err("Ambiguous timesheet filename ownership".into());
        }
        for directory in [
            base.clone(),
            base.join(naming::payroll_year_directory_name(&schedule.payroll_year)?),
        ] {
            let source = directory.join(&name);
            if !source.try_exists()? || seen.contains(&normalised_path(&source)?) {
                continue;
            }
            let referenced:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_timesheet_snapshot_states WHERE pdf_path=?1 UNION SELECT 1 FROM payroll_submissions WHERE pdf_path=?1)",[source.to_str()],|r|r.get(0))?;
            if referenced {
                return Err(format!(
                    "Timesheet no longer matches its registered digest: {}",
                    source.display()
                )
                .into());
            }
            let digest = file_digest(&source)?;
            verified(&source, &digest)?;
            let destination = base
                .join(naming::payroll_year_directory_name(&schedule.payroll_year)?)
                .join("Archived")
                .join(&name);
            seen.insert(normalised_path(&source)?);
            moves.push(Move {
                source,
                destination,
                digest,
                document_id: None,
            });
        }
    }
    // Exact generated-name pattern, unique maintained PA, but no stored schedule:
    // retain an existing year directory; otherwise use unknown-year Archived.
    for directory in year_directories(&base)? {
        if !directory.exists() {
            continue;
        }
        for entry in fs::read_dir(&directory)? {
            let source = entry?.path();
            if seen.contains(&normalised_path(&source)?) || !source.is_file() {
                continue;
            }
            let name = source.file_name().unwrap().to_string_lossy();
            let matches: Vec<_> = assistants
                .iter()
                .filter(|p| {
                    let prefix = format!(
                        "Timesheet - {} - ",
                        naming::sanitise_filename(&format!("{} {}", p.first_name, p.surname))
                    );
                    name.strip_prefix(&prefix)
                        .and_then(|tail| tail.strip_suffix(".pdf"))
                        .is_some_and(|code| {
                            code.len() == 9
                                && code.as_bytes()[6] == b'w'
                                && code.as_bytes()[..6].iter().all(|c| c.is_ascii_digit())
                                && code.as_bytes()[7..].iter().all(|c| c.is_ascii_digit())
                        })
                })
                .collect();
            if matches.len() != 1 || matches[0].id != pa.id {
                continue;
            }
            let referenced:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM payroll_timesheet_snapshot_states WHERE pdf_path=?1 UNION SELECT 1 FROM payroll_submissions WHERE pdf_path=?1)",[source.to_str()],|r|r.get(0))?;
            if referenced {
                continue;
            }
            let digest = file_digest(&source)?;
            verified(&source, &digest)?;
            let destination = directory.join("Archived").join(source.file_name().unwrap());
            moves.push(Move {
                source,
                destination,
                digest,
                document_id: None,
            });
        }
    }
    Ok(())
}

/// Retries only journalled cleanup. A changed source/destination is never deleted.
fn cleanup(db: &Connection, pa: i64, bases: &[PathBuf]) -> Result<()> {
    let pending=db.prepare("SELECT source_path,destination_path,sha256 FROM payroll_file_moves WHERE personal_assistant_id=?1 ORDER BY source_path")?.query_map([pa],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut failures = Vec::new();
    for (source, destination, digest) in pending {
        let attempt = (|| -> Result<()> {
            if source == destination {
                return Err("Invalid identical journal paths".into());
            }
            let normal_source = normalised_path(Path::new(&source))?;
            let normal_destination = normalised_path(Path::new(&destination))?;
            if normal_source == normal_destination {
                return Err("Journal paths identify the same file".into());
            }
            if !bases
                .iter()
                .any(|base| normal_source.starts_with(base) && normal_destination.starts_with(base))
            {
                return Err(
                    "Journalled file is outside the configured managed roots; cleanup refused"
                        .into(),
                );
            }
            verified(Path::new(&destination), &digest)?;
            checked_path(Path::new(&source))?;
            if Path::new(&source).exists() {
                verified(Path::new(&source), &digest)?;
                fs::remove_file(&source)?;
                sync_directory(Path::new(&source).parent().unwrap())?;
            }
            let parent = Path::new(&source).parent().unwrap();
            if parent
                .file_name()
                .is_some_and(|s| s == format!("PA {pa}").as_str())
                && parent.exists()
                && fs::read_dir(parent)?.next().is_none()
            {
                fs::remove_dir(parent)?;
                sync_directory(parent.parent().unwrap())?;
            }
            db.execute(
                "DELETE FROM payroll_file_moves WHERE source_path=?1",
                [&source],
            )?;
            Ok(())
        })();
        if let Err(e) = attempt {
            failures.push(format!(
                "Cleanup pending for {source} -> {destination}: {e}"
            ));
        }
    }
    errors(failures)
}

/// Repair directly normalises registered supplements without changing employment
/// or initiating ordinary filing. It also finishes committed cleanup for this PA,
/// which may remove verified obsolete ordinary-payslip sources.
pub fn apply(app: &Application, pa: &PersonalAssistant, save: bool) -> Result<Vec<String>> {
    let repository =
        PersonalAssistantRepository::new(Connection::open(&app.context.environment.database_path)?);
    let previous = repository
        .get_all()?
        .into_iter()
        .find(|p| p.id == pa.id)
        .ok_or("PA no longer exists")?;
    let rename = save && (pa.first_name != previous.first_name || pa.surname != previous.surname);
    let transition = save
        && previous
            .employment_status
            .as_deref()
            .is_some_and(|s| s.trim().eq_ignore_ascii_case("active"))
        && naming::is_inactive(pa);
    if save && !transition && !rename {
        // Ordinary active edits/reactivation retain the existing save behaviour;
        // they do not depend on archive availability or initiate file repair.
        repository.update(pa)?;
        let bases = managed_bases(app)?;
        let mut messages = Vec::new();
        if let Err(e) = cleanup(&repository.connection, pa.id, &bases) {
            messages.push(format!("Saved; cleanup remains pending: {e}"));
        }
        return Ok(messages);
    }
    let bases = managed_bases(app)?;
    cleanup(&repository.connection, pa.id, &bases)?;
    // Acquire the write lock before planning/publication; status/path changes share it.
    let tx = rusqlite::Transaction::new_unchecked(
        &repository.connection,
        rusqlite::TransactionBehavior::Immediate,
    )?;
    let moves = plan(app, pa, &previous, transition, rename)?;
    let mut published = Vec::new();
    let attempt = (|| -> Result<()> {
        if save {
            repository.update(pa)?;
        }
        for m in &moves {
            verified(&m.source, &m.digest)?;
            checked_path(&m.destination)?;
            fs::create_dir_all(m.destination.parent().unwrap())?;
            if !m.destination.exists() {
                // Stage privately, sync and verify, then publish with no-clobber semantics.
                let mut temporary =
                    tempfile::NamedTempFile::new_in(m.destination.parent().unwrap())?;
                std::io::copy(&mut fs::File::open(&m.source)?, &mut temporary)?;
                temporary.as_file().sync_all()?;
                verified(temporary.path(), &m.digest)?;
                fs::hard_link(temporary.path(), &m.destination)?;
                published.push(m.destination.clone());
            }
            verified(&m.destination, &m.digest)?;
            sync_directory(m.destination.parent().unwrap())?;
            if let Some(id) = m.document_id {
                if tx.execute("UPDATE imported_payroll_documents SET stored_path=?1 WHERE id=?2 AND stored_path=?3 AND sha256=?4",params![m.destination.to_str().ok_or("Non-UTF8 path")?,id,m.source.to_str().ok_or("Non-UTF8 path")?,m.digest])?!=1 { return Err("Document registration changed during filing".into()); }
            }
            // Location metadata changes; hashes and historical delivery facts do not.
            for table in [
                "payslip_revisions",
                "payroll_timesheet_snapshot_states",
                "payroll_submissions",
            ] {
                let column = if table == "payslip_revisions" {
                    "stored_path"
                } else {
                    "pdf_path"
                };
                tx.execute(
                    &format!("UPDATE {table} SET {column}=?1 WHERE {column}=?2"),
                    params![m.destination.to_str(), m.source.to_str()],
                )?;
            }
            tx.execute("INSERT INTO payroll_file_moves(source_path,destination_path,personal_assistant_id,sha256) VALUES(?1,?2,?3,?4)",params![m.source.to_str().ok_or("Non-UTF8 path")?,m.destination.to_str().ok_or("Non-UTF8 path")?,pa.id,m.digest])?;
        }
        tx.commit()?;
        Ok(())
    })();
    if let Err(error) = attempt {
        let mut failures = vec![format!(
            "Payroll filing not committed; originals retained: {error}"
        )];
        for path in published {
            let digest = &moves.iter().find(|m| m.destination == path).unwrap().digest;
            if let Err(e) = verified(&path, digest).and_then(|_| Ok(fs::remove_file(&path)?)) {
                failures.push(format!(
                    "Extra unregistered copy retained at {}: {e}",
                    path.display()
                ));
            }
        }
        return Err(failures.join("\n").into());
    }
    let mut messages = vec![format!(
        "{} payroll file(s) filed; document delivery history preserved.",
        moves.len()
    )];
    if let Err(e) = cleanup(&repository.connection, pa.id, &bases) {
        messages.push(format!("Saved successfully, but {e}. Use Repair registered payroll filing to retry cleanup; do not delete either copy manually."));
    }
    Ok(messages)
}

#[cfg(test)]
#[path = "payroll_archive_tests.rs"]
mod tests;
