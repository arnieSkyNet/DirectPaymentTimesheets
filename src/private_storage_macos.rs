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
    fn acl_get_fd(fd: i32) -> *mut c_void;
    fn acl_set_fd(fd: i32, acl: *mut c_void) -> i32;
    fn acl_to_text(acl: *mut c_void, length: *mut isize) -> *mut c_char;
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
fn get(file: &File) -> io::Result<Acl> {
    let acl = unsafe { acl_get_fd(file.as_raw_fd()) };
    if acl.is_null() {
        Err(io::Error::last_os_error())
    } else {
        Ok(Acl(acl))
    }
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
        return Err(io::Error::last_os_error());
    }
    let file = unsafe { File::from_raw_fd(descriptor) };
    verify_private(&file)?;
    Ok(file)
}
pub fn create_dir(path: &Path) -> io::Result<()> {
    let encoded = path_string(path)?;
    if unsafe { dpt_private_mkdir(encoded.as_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
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
    if metadata.uid() != unsafe { libc::geteuid() }
        || metadata.permissions().mode() & 0o077 != 0
        || has_allow_entries(file)?
    {
        return Err(io::Error::other(
            "Filesystem did not enforce private creation attributes",
        ));
    }
    Ok(())
}
pub fn preserve(new: &File, existing: &File) -> io::Result<()> {
    let acl = get(existing)?;
    if unsafe { acl_set_fd(new.as_raw_fd(), acl.0) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
pub fn has_allow_entries(file: &File) -> io::Result<bool> {
    let acl = get(file)?;
    let text = unsafe { acl_to_text(acl.0, std::ptr::null_mut()) };
    if text.is_null() {
        return Err(io::Error::last_os_error());
    }
    let allocation = Acl(text.cast());
    let value = unsafe { std::ffi::CStr::from_ptr(text) }.to_string_lossy();
    let has_allow = value
        .split(|character: char| character == ':' || character.is_whitespace())
        .any(|token| token == "allow");
    drop(allocation);
    Ok(has_allow)
}
