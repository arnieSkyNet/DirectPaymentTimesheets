use chrono::Local;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

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
