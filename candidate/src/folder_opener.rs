use std::path::Path;
#[cfg(unix)]
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

/// Open the original path with the desktop's default application.
#[cfg(unix)]
pub fn open(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(not(target_os = "macos"))]
    let mut command = Command::new("xdg-open");
    command.arg(path).spawn().map(|_| ())
}

#[cfg(target_os = "windows")]
pub fn open(path: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            window: *mut std::ffi::c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> isize;
    }
    let file: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let operation: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    // Both strings remain alive and NUL-terminated throughout this synchronous
    // shell call; no command interpreter or interpolated command line is used.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        )
    };
    if result > 32 {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "Windows shell could not open the path (code {result})"
        )))
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
pub fn open(_path: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Opening files is not supported on this platform",
    ))
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
