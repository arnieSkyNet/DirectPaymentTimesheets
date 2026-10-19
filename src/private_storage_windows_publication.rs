//! Windows publication under the approved cooperating-writer threat model.
//! The final check/rename is not a compare-and-swap against malicious same-user IO.
use super::*;
use std::marker::PhantomData;
use std::rc::Rc;

#[cfg(test)]
thread_local! {
    static INTERPOSITION: std::cell::RefCell<Option<(bool, Box<dyn FnOnce()>)>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
pub(crate) fn with_publication_interposition<T>(
    after: bool,
    action: impl FnOnce() + 'static,
    run: impl FnOnce() -> T,
) -> T {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            INTERPOSITION.with(|hook| {
                hook.borrow_mut().take();
            });
        }
    }
    INTERPOSITION.with(|hook| {
        assert!(hook.borrow().is_none());
        *hook.borrow_mut() = Some((after, Box::new(action)));
    });
    let _reset = Reset;
    run()
}
#[cfg(test)]
fn interpose(after: bool) {
    let action = INTERPOSITION.with(|hook| {
        let mut hook = hook.borrow_mut();
        if hook.as_ref().is_some_and(|(phase, _)| *phase == after) {
            hook.take().map(|(_, action)| action)
        } else {
            None
        }
    });
    if let Some(action) = action {
        action();
    }
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(attributes: *const SecurityAttributes, owner: i32, name: *const u16) -> Handle;
    fn WaitForSingleObject(handle: Handle, timeout: u32) -> u32;
    fn ReleaseMutex(handle: Handle) -> i32;
    fn SetFileInformationByHandle(
        handle: Handle,
        class: u32,
        buffer: *const c_void,
        size: u32,
    ) -> i32;
}

struct PublicationMutex {
    handle: Token,
    // Windows mutex ownership is thread-affine, including recursive acquisition.
    _thread: PhantomData<Rc<()>>,
}
impl Drop for PublicationMutex {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.handle.0);
        }
    }
}

impl PublicationMutex {
    fn acquire(name: &str) -> io::Result<Self> {
        let sd = descriptor(false)?;
        let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        let raw = unsafe { CreateMutexW(&attributes(&sd), 0, name.as_ptr()) };
        if raw.is_null() {
            return Err(diagnostic(
                "windows.publication.mutex_create",
                io::Error::last_os_error(),
            ));
        }
        let handle = Token(raw);
        // CreateMutex ignores our descriptor when opening an existing object.
        let mut owner = std::ptr::null_mut();
        let mut acl = std::ptr::null_mut();
        let mut security = std::ptr::null_mut();
        let status = unsafe {
            GetSecurityInfo(
                raw,
                6,
                5,
                &mut owner,
                std::ptr::null_mut(),
                &mut acl,
                std::ptr::null_mut(),
                &mut security,
            )
        };
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let _security = LocalMemory(security);
        let current = binary_sid(&user_sid()?)?;
        let system = binary_sid("S-1-5-18")?;
        let administrators = binary_sid("S-1-5-32-544")?;
        if owner.is_null()
            || unsafe { IsValidSid(owner) } == 0
            || unsafe { EqualSid(owner, current.0) } == 0
            || assess_dacl(acl, &current, &system, &administrators, false, false)?.is_some()
        {
            return Err(io::Error::other(
                "windows.publication: untrusted_mutex_policy",
            ));
        }
        match unsafe { WaitForSingleObject(raw, 5000) } {
            0 => Ok(Self {
                handle,
                _thread: PhantomData,
            }),
            0x80 => {
                // We own an abandoned mutex, but cannot infer the previous result.
                unsafe {
                    ReleaseMutex(raw);
                }
                Err(io::Error::other(
                    "windows.publication: abandoned_writer; reassess before retry",
                ))
            }
            258 => Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "windows.publication: cooperating_writer_busy",
            )),
            _ => Err(diagnostic(
                "windows.publication.mutex_wait",
                io::Error::last_os_error(),
            )),
        }
    }
}

pub(crate) struct PublicationGuard {
    _locks: Vec<PublicationMutex>,
    _ancestors: Vec<Vec<File>>,
}

impl PublicationGuard {
    /// A trusted renderer may intentionally atomically replace its reservation.
    /// Establish the completed object's identity BEFORE hashing/ledger approval;
    /// never refresh an unexpected identity inside the publication operation.
    pub(crate) fn rendered_stage(
        &self,
        pending: tempfile::NamedTempFile,
    ) -> Result<tempfile::NamedTempFile, tempfile::PersistError> {
        let completed = (|| {
            let parent = inspection_handle_at(
                pending
                    .path()
                    .parent()
                    .ok_or_else(|| io::Error::other("Missing render parent"))?,
                Some((1, "existing_ancestor")),
            )?;
            let id = root_identity(&parent)?;
            if !self._ancestors.iter().any(|chain| {
                chain
                    .last()
                    .is_some_and(|file| root_identity(file).is_ok_and(|held| held == id))
            }) {
                return Err(io::Error::other("Render parent is not guarded"));
            }
            let file = crate::private_storage::open_read(pending.path())?;
            if assess_access_with_owner_policy(&file, false, false, None, true)?.is_some() {
                return Err(io::Error::other("Completed render is not private"));
            }
            Ok(file)
        })();
        match completed {
            Ok(file) => {
                let (reservation, path) = pending.into_parts();
                drop(reservation);
                Ok(tempfile::NamedTempFile::from_parts(file, path))
            }
            Err(error) => Err(crate::private_storage::retain_failed_publication(
                pending, error,
            )),
        }
    }
}

pub(crate) fn publication_guard(source: &Path, destination: &Path) -> io::Result<PublicationGuard> {
    let mut ancestors = Vec::new();
    let mut keys = Vec::new();
    let sid = user_sid()?;
    for path in [source, destination] {
        let chain = ancestor_guard(path, false)?;
        let root = chain
            .first()
            .ok_or_else(|| io::Error::other("Missing publication root"))?;
        if root_class(
            query_root(root).map_err(|_| io::Error::other("Unverifiable publication volume"))?,
        ) != "verified_local_ntfs_root"
        {
            return Err(io::Error::other(
                "windows.publication: requires_verified_local_ntfs",
            ));
        }
        let parent = root_identity(chain.last().unwrap())?;
        let id: String = parent.id.iter().map(|b| format!("{b:02x}")).collect();
        keys.push(format!(
            "Global\\DirectPaymentTimesheets-publication-v1-{sid}-{:016x}-{id}",
            parent.volume
        ));
        ancestors.push(chain);
    }
    // Parent identity, not spelling/case/short-name, determines writer exclusion.
    // Stable ordering avoids deadlock for a publication spanning two parents.
    keys.sort();
    keys.dedup();
    let mut locks = Vec::new();
    for key in keys {
        locks.push(PublicationMutex::acquire(&key)?);
    }
    // Validate before callers allocate/write a stage or copy the target DACL.
    // Otherwise unsafe grants could expose new bytes even when rename refuses.
    match crate::private_storage::open_read(destination) {
        Ok(file) => {
            if assess_access_with_owner_policy(&file, true, false, None, true)?.is_some() {
                return Err(io::Error::other("windows.publication: unsafe_destination"));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(error),
    }
    Ok(PublicationGuard {
        _locks: locks,
        _ancestors: ancestors,
    })
}

pub(crate) fn publish(
    stage: &File,
    source: &Path,
    destination: &Path,
    overwrite: bool,
) -> io::Result<()> {
    let _guard = publication_guard(source, destination)?;
    crate::private_storage::check_path(source)?;
    crate::private_storage::check_path(destination)?;
    // Open a DELETE-capable handle to the stage without releasing its original
    // identity anchor. Rename uses this handle, not a re-resolved source name.
    let handle = unsafe {
        CreateFileW(
            wide(source)?.as_ptr(),
            0x30080,
            7,
            std::ptr::null(),
            3,
            0x00200000,
            std::ptr::null_mut(),
        )
    };
    if handle as isize == -1 {
        return Err(io::Error::last_os_error());
    }
    let source_handle = unsafe { File::from_raw_handle(handle) };
    if root_identity(stage)? != root_identity(&source_handle)?
        || !source_handle.metadata()?.is_file()
    {
        return Err(io::Error::other(
            "windows.publication: stage_identity_changed",
        ));
    }
    // Reject untrusted destructive rights on either file; intentionally shared
    // read access is retained, as for existing business documents.
    if assess_access_with_owner_policy(&source_handle, true, false, None, true)?.is_some() {
        return Err(io::Error::other("windows.publication: unsafe_stage"));
    }
    let original = match crate::private_storage::open_read(destination) {
        Ok(file) => {
            if assess_access_with_owner_policy(&file, true, false, None, true)?.is_some() {
                return Err(io::Error::other("windows.publication: unsafe_destination"));
            }
            if !overwrite {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "Publication destination exists",
                ));
            }
            Some(file)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    // Preserve no-clobber semantics if absent even when the caller permits overwrite.
    let replace = original.is_some();
    #[cfg(test)]
    interpose(false);
    if let Some(original) = &original {
        let current = crate::private_storage::open_read(destination)?;
        if root_identity(original)? != root_identity(&current)? {
            return Err(io::Error::other(
                "windows.publication: destination_identity_changed",
            ));
        }
    }
    #[cfg(test)]
    interpose(true);
    let name = wide(destination)?;
    let name = &name[..name.len() - 1];
    if name.len() > 32766 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Publication name exceeds Windows limit",
        ));
    }
    // FILE_RENAME_INFO uses pointer alignment (FileName offset 20 on x64, 12 on x86).
    let pointer = std::mem::size_of::<usize>();
    let root_offset = pointer;
    let length_offset = root_offset + pointer;
    let name_offset = length_offset + 4;
    let bytes = name
        .len()
        .checked_mul(2)
        .ok_or_else(|| io::Error::other("Rename name overflow"))?;
    let size = (name_offset + bytes + 2).max(root_offset + pointer + 8);
    let mut buffer = vec![0usize; size.div_ceil(pointer)];
    let raw = buffer.as_mut_ptr().cast::<u8>();
    unsafe {
        // REPLACE_IF_EXISTS | POSIX_SEMANTICS; no readonly/ACL bypass flags.
        std::ptr::write_unaligned(raw.cast::<u32>(), if replace { 3 } else { 0 });
        std::ptr::write_unaligned(
            raw.add(length_offset).cast::<u32>(),
            bytes
                .try_into()
                .map_err(|_| io::Error::other("Rename name too long"))?,
        );
        std::ptr::copy_nonoverlapping(name.as_ptr().cast::<u8>(), raw.add(name_offset), bytes);
        if SetFileInformationByHandle(
            source_handle.as_raw_handle().cast(),
            22,
            raw.cast(),
            size as u32,
        ) == 0
        {
            let error = io::Error::last_os_error();
            eprintln!(
                "windows.publication.rename: code={:?}; class={:?}",
                error.raw_os_error(),
                error.kind()
            );
            return Err(error);
        }
    }
    // original intentionally remains held through rename. No retry/copy fallback.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_publication_file_read_sharing_never_admits_append_or_destructive_access() {
        let current = binary_sid(&user_sid().unwrap()).unwrap();
        let system = binary_sid("S-1-5-18").unwrap();
        let administrators = binary_sid("S-1-5-32-544").unwrap();
        for mask in [0x1200a9u32, 4, 2, 0x10000, 0x40000, 0x80000, 0x100, 0x10] {
            let text: Vec<u16> =
                format!("D:P(A;;FA;;;{})(A;;0x{mask:x};;;WD)", user_sid().unwrap())
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
            let mut raw = std::ptr::null_mut();
            assert_ne!(
                unsafe {
                    ConvertStringSecurityDescriptorToSecurityDescriptorW(
                        text.as_ptr(),
                        1,
                        &mut raw,
                        std::ptr::null_mut(),
                    )
                },
                0
            );
            let descriptor = LocalMemory(raw);
            let mut acl = std::ptr::null_mut();
            let mut present = 0;
            let mut defaulted = 0;
            assert_ne!(
                unsafe {
                    GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut acl, &mut defaulted)
                },
                0
            );
            let outcome = assess_dacl_with_file_sharing(
                acl,
                &current,
                &system,
                &administrators,
                false,
                false,
                true,
            )
            .unwrap();
            // A renderer's completed intermediate remains strictly private,
            // even though final business documents may intentionally share read.
            assert_eq!(
                assess_dacl_with_file_sharing(
                    acl,
                    &current,
                    &system,
                    &administrators,
                    false,
                    false,
                    false,
                )
                .unwrap(),
                Some("untrusted_allow_ace")
            );
            assert_eq!(
                outcome,
                if mask == 0x1200a9 {
                    None
                } else {
                    Some("untrusted_allow_ace")
                }
            );
        }
    }
}
