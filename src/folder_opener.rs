use std::path::Path;
use std::process::Command;

use eframe::egui;

pub fn containing_folder(path: &Path) -> Option<&Path> {
    if path.is_dir() {
        Some(path)
    } else if path.exists() {
        path.parent()
    } else {
        None
    }
}

pub fn button(ui: &mut egui::Ui, path: &Path) -> Option<String> {
    let folder = containing_folder(path)?;
    if ui
        .small_button("📂")
        .on_hover_text("Open containing folder")
        .clicked()
    {
        if let Err(error) = open(folder) {
            return Some(format!("Could not open {}: {error}", folder.display()));
        }
    }
    None
}

fn open(folder: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer");
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = Command::new("xdg-open");

    command.arg(folder).spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_files_and_directories_but_not_missing_paths() {
        let directory = tempfile::TempDir::new().unwrap();
        let file = directory.path().join("result.pdf");
        std::fs::write(&file, "result").unwrap();
        assert_eq!(containing_folder(&file), Some(directory.path()));
        assert_eq!(containing_folder(directory.path()), Some(directory.path()));
        assert_eq!(containing_folder(&directory.path().join("missing")), None);
    }
}
