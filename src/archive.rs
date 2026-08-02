use chrono::Local;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

pub fn archive_csv(
    source: &Path,
    archive_dir: &Path,
) -> Result<PathBuf, Box<dyn Error>> {

    let now = Local::now();

    let year = now.format("%Y").to_string();
    let month = now.format("%m").to_string();

    let archive_path = archive_dir
        .join(year)
        .join(month);

    fs::create_dir_all(&archive_path)?;

    let filename = source
        .file_name()
        .ok_or("Invalid source filename")?
        .to_string_lossy();

    let timestamp = now.format("%Y-%m-%d_%H%M%S");

    let archive_filename =
        format!("{}_{}", timestamp, filename);

    let destination = archive_path.join(archive_filename);

    fs::copy(source, &destination)?;

    Ok(destination)
}

