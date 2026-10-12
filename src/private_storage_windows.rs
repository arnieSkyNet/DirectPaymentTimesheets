//! Win32 creation-time protected DACLs: current token user and SYSTEM only.
//! No inherited broad ACEs and no post-creation window with an inherited DACL.
use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::os::windows::{
    ffi::OsStrExt,
    io::{AsRawHandle, FromRawHandle},
};
use std::path::Path;

type Handle = *mut c_void;
#[repr(C)]
struct SecurityAttributes {
    length: u32,
    descriptor: *mut c_void,
    inherit: i32,
}
#[link(name = "advapi32")]
extern "system" {
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        text: *const u16,
        revision: u32,
        descriptor: *mut *mut c_void,
        size: *mut u32,
    ) -> i32;
    fn ConvertSecurityDescriptorToStringSecurityDescriptorW(
        descriptor: *const c_void,
        revision: u32,
        information: u32,
        text: *mut *mut u16,
        length: *mut u32,
    ) -> i32;
    fn GetFileSecurityW(
        path: *const u16,
        information: u32,
        descriptor: *mut c_void,
        size: u32,
        needed: *mut u32,
    ) -> i32;
    fn GetSecurityDescriptorControl(
        descriptor: *const c_void,
        control: *mut u16,
        revision: *mut u32,
    ) -> i32;
    fn SetFileSecurityW(path: *const u16, information: u32, descriptor: *const c_void) -> i32;
    fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
    fn GetTokenInformation(
        token: Handle,
        class: u32,
        information: *mut c_void,
        size: u32,
        needed: *mut u32,
    ) -> i32;
    fn ConvertStringSidToSidW(text: *const u16, sid: *mut *mut c_void) -> i32;
    fn EqualSid(first: *const c_void, second: *const c_void) -> i32;
    fn IsValidSid(sid: *const c_void) -> i32;
    fn IsValidAcl(acl: *const c_void) -> i32;
    fn GetAclInformation(acl: *const c_void, info: *mut c_void, size: u32, class: u32) -> i32;
    fn GetAce(acl: *const c_void, index: u32, ace: *mut *mut c_void) -> i32;
    fn GetSecurityInfo(
        handle: Handle,
        object: u32,
        information: u32,
        owner: *mut *mut c_void,
        group: *mut *mut c_void,
        dacl: *mut *mut c_void,
        sacl: *mut *mut c_void,
        descriptor: *mut *mut c_void,
    ) -> u32;
    fn ConvertSidToStringSidW(sid: *const c_void, text: *mut *mut u16) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn CreateDirectoryW(path: *const u16, attributes: *const SecurityAttributes) -> i32;
    fn CreateFileW(
        path: *const u16,
        access: u32,
        share: u32,
        attributes: *const SecurityAttributes,
        creation: u32,
        flags: u32,
        template: Handle,
    ) -> Handle;
    fn GetCurrentProcess() -> Handle;
    fn CloseHandle(handle: Handle) -> i32;
    fn LocalFree(memory: Handle) -> Handle;
}
struct LocalMemory(*mut c_void);
impl Drop for LocalMemory {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
struct Token(Handle);
impl Drop for Token {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn wide(path: &Path) -> io::Result<Vec<u16>> {
    let path = std::path::absolute(path)?;
    let text = path.as_os_str();
    let raw: Vec<u16> = text.encode_wide().collect();
    if raw.contains(&0) {
        return Err(io::Error::other("NUL in storage path"));
    }
    // Extended paths preserve Unicode and support paths longer than MAX_PATH.
    let prefix: Vec<u16> = "\\\\?\\".encode_utf16().collect();
    let mut value = if raw.starts_with(&prefix) {
        raw
    } else if raw.starts_with(&[92, 92]) {
        "\\\\?\\UNC\\"
            .encode_utf16()
            .chain(raw.into_iter().skip(2))
            .collect()
    } else {
        prefix.into_iter().chain(raw).collect()
    };
    value.push(0);
    Ok(value)
}
unsafe fn local_string(pointer: *mut u16) -> String {
    let mut length = 0;
    while *pointer.add(length) != 0 {
        length += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length))
}
fn user_sid() -> io::Result<String> {
    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), 8, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = Token(token);
        let mut needed = 0;
        GetTokenInformation(token.0, 1, std::ptr::null_mut(), 0, &mut needed);
        if needed == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut buffer = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
        if GetTokenInformation(token.0, 1, buffer.as_mut_ptr().cast(), needed, &mut needed) == 0 {
            return Err(io::Error::last_os_error());
        }
        // TOKEN_USER starts with SID_AND_ATTRIBUTES; the first field is PSID.
        let sid = *(buffer.as_ptr().cast::<*mut c_void>());
        let mut text = std::ptr::null_mut();
        if ConvertSidToStringSidW(sid, &mut text) == 0 {
            return Err(io::Error::last_os_error());
        }
        let allocation = LocalMemory(text.cast());
        let value = local_string(text);
        drop(allocation);
        Ok(value)
    }
}
fn descriptor(directory: bool) -> io::Result<LocalMemory> {
    let sid = user_sid()?;
    let flags = if directory { "OICI" } else { "" };
    let text: Vec<u16> = format!("O:{sid}D:P(A;{flags};FA;;;{sid})(A;{flags};FA;;;SY)")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut pointer = std::ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            1,
            &mut pointer,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(LocalMemory(pointer))
}
fn attributes(descriptor: &LocalMemory) -> SecurityAttributes {
    SecurityAttributes {
        length: std::mem::size_of::<SecurityAttributes>() as u32,
        descriptor: descriptor.0,
        inherit: 0,
    }
}
pub fn create_dir(path: &Path) -> io::Result<()> {
    let path_wide = wide(path)?;
    let descriptor = descriptor(true)?;
    if unsafe { CreateDirectoryW(path_wide.as_ptr(), &attributes(&descriptor)) } == 0 {
        return Err(diagnostic("windows.create_dir", io::Error::last_os_error()));
    }
    if !is_private(path)? {
        // Retain failed creation: deleting by a substituted path is unsafe.
        return Err(io::Error::other(
            "Filesystem did not enforce a private directory ACL",
        ));
    }
    Ok(())
}
pub fn create_file(path: &Path) -> io::Result<File> {
    let path_wide = wide(path)?;
    let descriptor = descriptor(false)?;
    let handle = unsafe {
        CreateFileW(
            path_wide.as_ptr(),
            0xc0000000,
            7,
            &attributes(&descriptor),
            1,
            0x80,
            std::ptr::null_mut(),
        )
    };
    if handle as isize == -1 {
        return Err(diagnostic(
            "windows.create_file",
            io::Error::last_os_error(),
        ));
    }
    let file = unsafe { File::from_raw_handle(handle) };
    if !is_private_file(&file)? {
        drop(file);
        // Retain failed creation: deleting by a substituted path is unsafe.
        return Err(io::Error::other(
            "Filesystem did not enforce a private file ACL",
        ));
    }
    Ok(file)
}
fn security(path: &Path) -> io::Result<Vec<usize>> {
    let path = wide(path)?;
    let mut needed = 0;
    unsafe {
        GetFileSecurityW(path.as_ptr(), 7, std::ptr::null_mut(), 0, &mut needed);
    }
    if needed == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut buffer = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
    if unsafe {
        GetFileSecurityW(
            path.as_ptr(),
            7,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(buffer)
}
fn sddl(descriptor: &[usize], information: u32) -> io::Result<String> {
    let mut text = std::ptr::null_mut();
    if unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor.as_ptr().cast(),
            1,
            information,
            &mut text,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let allocation = LocalMemory(text.cast());
    let value = unsafe { local_string(text) };
    drop(allocation);
    Ok(value)
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
fn binary_sid(text: &str) -> io::Result<LocalMemory> {
    let text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let mut sid = std::ptr::null_mut();
    if unsafe { ConvertStringSidToSidW(text.as_ptr(), &mut sid) } == 0 {
        return Err(diagnostic(
            "windows.convert_sid",
            io::Error::last_os_error(),
        ));
    }
    Ok(LocalMemory(sid))
}
fn inspection_handle(path: &Path) -> io::Result<File> {
    let path = wide(path)?;
    // READ_CONTROL | FILE_READ_ATTRIBUTES, OPEN_EXISTING, do not follow the
    // final reparse point, support directories, and deny delete sharing while held.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            0x20080,
            3,
            std::ptr::null(),
            3,
            0x02200000,
            std::ptr::null_mut(),
        )
    };
    if handle as isize == -1 {
        return Err(diagnostic(
            "windows.open_security_handle",
            io::Error::last_os_error(),
        ));
    }
    Ok(unsafe { File::from_raw_handle(handle) })
}
pub fn is_private(path: &Path) -> io::Result<bool> {
    crate::private_storage::check_path(path)?;
    is_private_file(&inspection_handle(path)?)
}
fn refused(classification: &str) -> bool {
    // Never print paths, identities or ACL contents.
    eprintln!("windows.inspect_private: code=none; class={classification}");
    false
}
fn is_private_file(file: &File) -> io::Result<bool> {
    use std::os::windows::fs::MetadataExt;
    if file.metadata()?.file_attributes() & 0x400 != 0 {
        return Ok(refused("reparse_point"));
    }
    let mut owner = std::ptr::null_mut();
    let mut acl = std::ptr::null_mut();
    let mut descriptor = std::ptr::null_mut();
    let status = unsafe {
        GetSecurityInfo(
            file.as_raw_handle().cast(),
            1,
            5,
            &mut owner,
            std::ptr::null_mut(),
            &mut acl,
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 {
        return Err(diagnostic(
            "windows.get_security_info",
            io::Error::from_raw_os_error(status as i32),
        ));
    }
    let _descriptor = LocalMemory(descriptor);
    let current = binary_sid(&user_sid()?)?;
    if owner.is_null()
        || unsafe { IsValidSid(owner) } == 0
        || unsafe { EqualSid(owner, current.0) } == 0
    {
        return Ok(refused("owner_mismatch"));
    }
    if acl.is_null() || unsafe { IsValidAcl(acl) } == 0 {
        return Ok(refused("null_or_invalid_dacl"));
    }
    #[repr(C)]
    struct AclSizeInformation {
        count: u32,
        used: u32,
        free: u32,
    }
    let mut info = AclSizeInformation {
        count: 0,
        used: 0,
        free: 0,
    };
    if unsafe {
        GetAclInformation(
            acl,
            (&mut info as *mut AclSizeInformation).cast(),
            std::mem::size_of::<AclSizeInformation>() as u32,
            2,
        )
    } == 0
    {
        return Err(diagnostic(
            "windows.get_acl_information",
            io::Error::last_os_error(),
        ));
    }
    let system = binary_sid("S-1-5-18")?;
    let administrators = binary_sid("S-1-5-32-544")?;
    let owner_rights = binary_sid("S-1-3-4")?;
    let creator_owner = binary_sid("S-1-3-0")?;
    for index in 0..info.count {
        let mut ace = std::ptr::null_mut();
        if unsafe { GetAce(acl, index, &mut ace) } == 0 {
            return Err(diagnostic("windows.get_ace", io::Error::last_os_error()));
        }
        // Only basic ALLOW/DENY ACEs have this layout. Unknown forms fail closed.
        let bytes = ace.cast::<u8>();
        let kind = unsafe { *bytes };
        if kind == 1 {
            continue;
        }
        if kind != 0 {
            return Ok(refused("unsupported_ace"));
        }
        let size = unsafe { std::ptr::read_unaligned(bytes.add(2).cast::<u16>()) } as usize;
        if size < 16 {
            return Ok(refused("invalid_ace_size"));
        }
        let trustee = unsafe { bytes.add(8) };
        let sid_size = 8 + 4 * unsafe { *trustee.add(1) } as usize;
        if sid_size > size - 8 || unsafe { IsValidSid(trustee.cast()) } == 0 {
            return Ok(refused("invalid_ace_sid"));
        }
        let matches = |sid: &LocalMemory| unsafe { EqualSid(trustee.cast(), sid.0) != 0 };
        let inherit_only = unsafe { *bytes.add(1) } & 0x08 != 0;
        if !matches(&current)
            && !matches(&system)
            && !matches(&administrators)
            && !matches(&owner_rights)
            && !(inherit_only && matches(&creator_owner))
        {
            return Ok(refused("untrusted_allow_ace"));
        }
    }
    Ok(true)
}
pub fn preserve_access(temporary: &Path, destination: &Path) -> io::Result<()> {
    let existing = security(destination)?;
    let new = security(temporary)?;
    if sddl(&existing, 3)? != sddl(&new, 3)? {
        return Err(io::Error::other(
            "Replacement would change owner/group; review storage separately",
        ));
    }
    let mut control = 0u16;
    let mut revision = 0u32;
    if unsafe {
        GetSecurityDescriptorControl(existing.as_ptr().cast(), &mut control, &mut revision)
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let inheritance = if control & 0x1000 != 0 {
        0x80000000
    } else {
        0x20000000
    };
    // Apply the destination's DACL/inheritance policy to the NEW inode only.
    if unsafe {
        SetFileSecurityW(
            wide(temporary)?.as_ptr(),
            inheritance | 4,
            existing.as_ptr().cast(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    std::fs::set_permissions(temporary, std::fs::metadata(destination)?.permissions())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phase1_windows_sqlite_sidecars_are_private_and_broad_parent_is_refused() {
        let root = crate::private_storage::tempdir().unwrap();
        let path = root.path().join("live.sqlite");
        let db = crate::database::open(&path).unwrap();
        db.execute_batch("CREATE TABLE fixture(value TEXT); BEGIN IMMEDIATE; INSERT INTO fixture VALUES('synthetic');").unwrap();
        assert!(is_private(&root.path().join("live.sqlite-journal")).unwrap());
        db.execute_batch(
            "COMMIT; PRAGMA journal_mode=WAL; INSERT INTO fixture VALUES('synthetic WAL');",
        )
        .unwrap();
        assert!(is_private(&root.path().join("live.sqlite-wal")).unwrap());
        assert!(is_private(&root.path().join("live.sqlite-shm")).unwrap());
        let shared = root.path().join("synthetic-shared-parent");
        create_dir(&shared).unwrap();
        // Alter only this isolated new fixture, under a private ancestor.
        let text: Vec<u16> = "D:P(A;OICI;FA;;;WD)"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut pointer = std::ptr::null_mut();
        assert_ne!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    text.as_ptr(),
                    1,
                    &mut pointer,
                    std::ptr::null_mut(),
                )
            },
            0
        );
        let descriptor = LocalMemory(pointer);
        assert_ne!(
            unsafe { SetFileSecurityW(wide(&shared).unwrap().as_ptr(), 0x80000004, descriptor.0) },
            0
        );
        assert!(!is_private(&shared).unwrap());
        let refused = shared.join("refused.sqlite");
        assert!(crate::private_storage::prepare_database(&refused).is_err());
        assert!(!refused.exists());
    }

    #[test]
    fn phase1_windows_creation_enforces_private_dacl_and_replacement_retains_access() {
        let root = crate::private_storage::tempdir().unwrap();
        assert!(is_private(root.path()).unwrap());
        let destination = root.path().join("existing");
        create_file(&destination).unwrap();
        let before = sddl(&security(&destination).unwrap(), 7).unwrap();
        crate::private_storage::write_atomic(&destination, b"replacement").unwrap();
        assert!(
            before == sddl(&security(&destination).unwrap(), 7).unwrap(),
            "windows.preserve_access: code=none; class=security_descriptor_changed"
        );
        assert!(is_private(&destination).unwrap());
    }
}
