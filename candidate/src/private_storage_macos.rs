//! Creation-time empty non-inheriting ACLs use a small Apple-SDK C wrapper.
//! Deliberate replacements retain the existing object's extended ACL.
use std::ffi::{c_char, c_void};
use std::fs::File;
use std::io;
use std::os::{
    fd::{AsRawFd, FromRawFd},
    unix::ffi::OsStrExt,
};
use std::path::Path;
#[link(name = "System")]
extern "C" {
    fn dpt_read_acl(fd: i32, acl: *mut *mut c_void) -> i32;
    fn dpt_acl_has_allow(acl: *mut c_void) -> i32;
    fn acl_init(count: i32) -> *mut c_void;
    fn acl_set_fd(fd: i32, acl: *mut c_void) -> i32;
    fn acl_free(memory: *mut c_void) -> i32;
}
struct Acl(*mut c_void);
impl Drop for Acl {
    fn drop(&mut self) {
        unsafe {
            acl_free(self.0);
        }
    }
}
// Successful handle inspection distinguishes an absent ACL from an I/O failure.
fn get(file: &File) -> io::Result<Option<Acl>> {
    let mut acl = std::ptr::null_mut();
    if unsafe { dpt_read_acl(file.as_raw_fd(), &mut acl) } != 0 {
        return Err(diagnostic("macos.read_acl", io::Error::last_os_error()));
    }
    Ok(if acl.is_null() { None } else { Some(Acl(acl)) })
}
fn diagnostic(operation: &str, error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!(
            "{operation}: code={:?}; class={:?}",
            error.raw_os_error(),
            error.kind()
        ),
    )
}
extern "C" {
    fn dpt_private_open(path: *const c_char) -> i32;
    fn dpt_private_mkdir(path: *const c_char) -> i32;
}
fn path_string(path: &Path) -> io::Result<std::ffi::CString> {
    std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::other("NUL in storage path"))
}
pub fn create_file(path: &Path) -> io::Result<File> {
    let path = path_string(path)?;
    let descriptor = unsafe { dpt_private_open(path.as_ptr()) };
    if descriptor < 0 {
        return Err(diagnostic("macos.create_file", io::Error::last_os_error()));
    }
    let file = unsafe { File::from_raw_fd(descriptor) };
    verify_private(&file)?;
    Ok(file)
}
pub fn create_dir(path: &Path) -> io::Result<()> {
    let encoded = path_string(path)?;
    if unsafe { dpt_private_mkdir(encoded.as_ptr()) } != 0 {
        return Err(diagnostic("macos.create_dir", io::Error::last_os_error()));
    }
    use std::os::unix::fs::OpenOptionsExt;
    let directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
        .open(path)?;
    verify_private(&directory)
}
fn verify_private(file: &File) -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let metadata = file.metadata()?;
    let classification = if metadata.uid() != unsafe { libc::geteuid() } {
        Some("owner_mismatch")
    } else if metadata.permissions().mode() & 0o077 != 0 {
        Some("broad_mode")
    } else if has_allow_entries(file)? {
        Some("allow_ace")
    } else {
        None
    };
    if let Some(classification) = classification {
        return Err(io::Error::other(format!(
            "macos.verify_private: code=none; class={classification}"
        )));
    }
    Ok(())
}
pub fn preserve(new: &File, existing: &File) -> io::Result<()> {
    let acl = match get(existing)? {
        Some(acl) => acl,
        None => {
            let pointer = unsafe { acl_init(0) };
            if pointer.is_null() {
                return Err(diagnostic("macos.empty_acl", io::Error::last_os_error()));
            }
            Acl(pointer)
        }
    };
    if unsafe { acl_set_fd(new.as_raw_fd(), acl.0) } != 0 {
        return Err(diagnostic("macos.preserve_acl", io::Error::last_os_error()));
    }
    Ok(())
}
pub fn has_allow_entries(file: &File) -> io::Result<bool> {
    let Some(acl) = get(file)? else {
        return Ok(false);
    };
    match unsafe { dpt_acl_has_allow(acl.0) } {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(diagnostic(
            "macos.inspect_acl_entries",
            io::Error::last_os_error(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phase1_macos_bad_handle_is_not_treated_as_absent_acl() {
        let mut acl = std::ptr::null_mut();
        assert_eq!(unsafe { dpt_read_acl(-1, &mut acl) }, -1);
        assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::EBADF));
        assert!(acl.is_null());
    }
    #[test]
    fn phase1_macos_absent_acl_is_not_a_missing_object() {
        let root = crate::private_storage::tempdir().unwrap();
        let path = root.path().join("synthetic-no-acl");
        std::fs::write(&path, b"synthetic").unwrap();
        let file = File::open(&path).unwrap();
        assert!(!has_allow_entries(&file).unwrap());
        assert!(verify_private(&File::open(root.path()).unwrap()).is_ok());
        let replacement = create_file(&root.path().join("replacement")).unwrap();
        preserve(&replacement, &file).unwrap();
        assert!(!has_allow_entries(&replacement).unwrap());
    }
}
