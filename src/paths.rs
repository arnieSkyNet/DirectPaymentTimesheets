use std::path::PathBuf;

pub fn expand_path(path: &PathBuf) -> PathBuf {
    let path_string = path.to_string_lossy();

    if path_string.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(&path_string[2..]);
        }
    }

    path.clone()
}

