//! Preserve named POSIX access ACLs on deliberate replacement, without copying
//! permissions from untrusted sources into newly created private copies.
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;

/// Anchor cleanup to the original parent and directory. Deletion is permitted
/// only while ordinary other users cannot substitute the parent's entry.
/// Privileged/malicious same-user processes are outside this boundary.
#[derive(Debug)]
pub struct DirectoryCleanup {
    parent: File,
    directory: File,
    name: std::ffi::OsString,
}

impl DirectoryCleanup {
    pub fn capture(path: &std::path::Path) -> io::Result<Self> {
        use std::os::unix::fs::OpenOptionsExt;
        let parent = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(
                path.parent()
                    .ok_or_else(|| io::Error::other("Missing staging parent"))?,
            )?;
        let name = path
            .file_name()
            .ok_or_else(|| io::Error::other("Missing staging name"))?
            .to_os_string();
        let directory = Self::open_child(&parent, &name)?;
        let result = Self {
            parent,
            directory,
            name,
        };
        result.check_access()?;
        Ok(result)
    }

    fn open_child(parent: &File, name: &std::ffi::OsStr) -> io::Result<File> {
        use std::os::{fd::FromRawFd, unix::ffi::OsStrExt};
        let name = std::ffi::CString::new(name.as_bytes())
            .map_err(|_| io::Error::other("Invalid staging name"))?;
        let descriptor = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if descriptor < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_fd(descriptor) })
    }

    fn check_access(&self) -> io::Result<()> {
        use std::os::unix::fs::MetadataExt;
        let user = unsafe { libc::geteuid() };
        let parent = self.parent.metadata()?;
        let directory = self.directory.metadata()?;
        // Linux POSIX ACL write permissions are bounded by the group-class
        // mode mask. Sticky trusted parents also protect our owned entry.
        if (parent.uid() != user && parent.uid() != 0)
            || (parent.mode() & 0o022 != 0 && parent.mode() & 0o1000 == 0)
            || directory.uid() != user
            || directory.mode() & 0o077 != 0
        {
            return Err(io::Error::other(
                "Staging cleanup namespace is not protected",
            ));
        }
        Ok(())
    }

    pub fn remove(&self) -> io::Result<()> {
        use std::os::unix::fs::MetadataExt;
        self.check_access()?;
        let current = Self::open_child(&self.parent, &self.name)?;
        let original = self.directory.metadata()?;
        let candidate = current.metadata()?;
        if original.dev() != candidate.dev() || original.ino() != candidate.ino() {
            return Err(io::Error::other(
                "Staging directory substituted; cleanup refused",
            ));
        }
        // Anchor traversal to the ORIGINAL directory, not a reopened root name.
        // No pathname fallback if procfs is absent. Rust's Linux remove_dir_all
        // traverses child directories with openat/unlinkat and O_NOFOLLOW.
        let anchor =
            std::path::PathBuf::from(format!("/proc/self/fd/{}", self.directory.as_raw_fd()));
        let anchor_metadata = std::fs::metadata(&anchor)?;
        if anchor_metadata.dev() != original.dev() || anchor_metadata.ino() != original.ino() {
            return Err(io::Error::other("Staging directory descriptor unavailable"));
        }
        for entry in std::fs::read_dir(&anchor)? {
            let path = entry?.path();
            let result = (|| {
                if std::fs::symlink_metadata(&path)?.is_dir() {
                    std::fs::remove_dir_all(&path)
                } else {
                    std::fs::remove_file(&path)
                }
            })();
            if let Err(error) = result {
                if error.kind() != io::ErrorKind::NotFound {
                    return Err(error);
                }
            }
        }
        self.check_access()?;
        let current = Self::open_child(&self.parent, &self.name)?.metadata()?;
        if current.dev() != original.dev() || current.ino() != original.ino() {
            return Err(io::Error::other("Staging root changed; unlink refused"));
        }
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::CString::new(self.name.as_bytes())
            .map_err(|_| io::Error::other("Invalid staging name"))?;
        // Only remove the empty original root name in the protected parent.
        if unsafe { libc::unlinkat(self.parent.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) }
            != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
const NAME: &[u8] = b"system.posix_acl_access\0";
fn absent(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(libc::ENODATA | libc::EOPNOTSUPP))
}
pub fn access_acl(file: &File) -> io::Result<Option<Vec<u8>>> {
    let size = unsafe {
        libc::fgetxattr(
            file.as_raw_fd(),
            NAME.as_ptr().cast(),
            std::ptr::null_mut(),
            0,
        )
    };
    if size < 0 {
        let e = io::Error::last_os_error();
        return if absent(&e) { Ok(None) } else { Err(e) };
    }
    let mut bytes = vec![0u8; size as usize];
    let read = unsafe {
        libc::fgetxattr(
            file.as_raw_fd(),
            NAME.as_ptr().cast(),
            bytes.as_mut_ptr().cast(),
            bytes.len(),
        )
    };
    if read < 0 {
        return Err(io::Error::last_os_error());
    }
    bytes.truncate(read as usize);
    Ok(Some(bytes))
}
pub fn preserve(new: &File, existing: &File) -> io::Result<()> {
    match access_acl(existing)? {
        Some(bytes) => {
            if unsafe {
                libc::fsetxattr(
                    new.as_raw_fd(),
                    NAME.as_ptr().cast(),
                    bytes.as_ptr().cast(),
                    bytes.len(),
                    0,
                )
            } < 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        None => {
            if unsafe { libc::fremovexattr(new.as_raw_fd(), NAME.as_ptr().cast()) } < 0 {
                let error = io::Error::last_os_error();
                if !absent(&error) {
                    return Err(error);
                }
            }
        }
    }
    Ok(())
}
