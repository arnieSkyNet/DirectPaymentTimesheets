use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AppError;

pub fn archive_csv(
    source_file: &Path,
    archive_root: &Path,
) -> Result<PathBuf, AppError> {
    let now = chrono::Local::now();

    let year = now.format("%Y").to_string();
    let month = now.format("%Y-%m").to_string();

    let archive_dir = archive_root.join(year).join(month);

    fs::create_dir_all(&archive_dir)
        .map_err(|e| AppError::Config(e.to_string()))?;

    let filename = source_file
        .file_name()
        .ok_or_else(|| AppError::Config("Invalid CSV filename".to_string()))?;

    let archive_file = archive_dir.join(filename);

    fs::copy(source_file, &archive_file)
        .map_err(|e| AppError::Config(e.to_string()))?;

    Ok(archive_file)
}
