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

#[path = "private_storage_windows_publication.rs"]
mod publication;
pub(super) use publication::{publication_guard, publish, PublicationGuard};
#[cfg(test)]
pub(super) use publication::with_publication_interposition;

type Handle = *mut c_void;
// Metadata-only opens do not participate in delete-share accounting on NTFS.
// FILE_LIST_DIRECTORY makes share3 a directory rename barrier. Never fall back
// to metadata-only access when this required right is denied.
const DIRECTORY_GUARD_ACCESS: u32 = 0x20081; // READ_CONTROL | READ_ATTRIBUTES | LIST_DIRECTORY
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
    #[cfg(test)]
    fn GetSecurityDescriptorOwner(descriptor: *const c_void, owner: *mut *mut c_void, defaulted: *mut i32) -> i32;
    #[cfg(test)]
    fn GetSecurityDescriptorDacl(
        descriptor: *const c_void,
        present: *mut i32,
        acl: *mut *mut c_void,
        defaulted: *mut i32,
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
    fn LookupAccountNameW(
        system: *const u16,
        name: *const u16,
        sid: *mut c_void,
        sid_size: *mut u32,
        domain: *mut u16,
        domain_size: *mut u32,
        sid_use: *mut u32,
    ) -> i32;
    fn LookupAccountSidW(
        system: *const u16,
        sid: *const c_void,
        name: *mut u16,
        name_size: *mut u32,
        domain: *mut u16,
        domain_size: *mut u32,
        sid_use: *mut u32,
    ) -> i32;
    fn OpenSCManagerW(machine: *const u16, database: *const u16, access: u32) -> Handle;
    fn OpenServiceW(manager: Handle, name: *const u16, access: u32) -> Handle;
    fn QueryServiceConfig2W(
        service: Handle,
        level: u32,
        buffer: *mut u8,
        size: u32,
        needed: *mut u32,
    ) -> i32;
    fn CloseServiceHandle(handle: Handle) -> i32;
    fn ConvertSidToStringSidW(sid: *const c_void, text: *mut *mut u16) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn GetFinalPathNameByHandleW(handle: Handle, buffer: *mut u16, size: u32, flags: u32) -> u32;
    fn GetVolumeInformationByHandleW(
        handle: Handle,
        name: *mut u16,
        name_size: u32,
        serial: *mut u32,
        maximum: *mut u32,
        flags: *mut u32,
        filesystem: *mut u16,
        filesystem_size: u32,
    ) -> i32;
    fn GetFileInformationByHandleEx(
        handle: Handle,
        class: u32,
        buffer: *mut c_void,
        size: u32,
    ) -> i32;
    fn GetDriveTypeW(root: *const u16) -> u32;
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
        return Err(diagnostic(
            "windows.GetFileSecurityW.size",
            io::Error::last_os_error(),
        ));
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
        return Err(diagnostic(
            "windows.GetFileSecurityW.read",
            io::Error::last_os_error(),
        ));
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
    eprintln!(
        "{operation}: code={:?}; class={:?}",
        error.raw_os_error(),
        error.kind()
    );
    io::Error::new(
        error.kind(),
        format!(
            "{operation}: code={:?}; class={:?}",
            error.raw_os_error(),
            error.kind()
        ),
    )
}
#[derive(Debug)]
struct DiagnosticError {
    operation: &'static str,
    code: Option<i32>,
    kind: io::ErrorKind,
}
impl std::fmt::Display for DiagnosticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: code={:?}; class={:?}",
            self.operation, self.code, self.kind
        )
    }
}
impl std::error::Error for DiagnosticError {}
fn dacl_diagnostic(operation: &'static str, error: io::Error) -> io::Error {
    let payload = DiagnosticError {
        operation,
        code: error.raw_os_error(),
        kind: error.kind(),
    };
    eprintln!("{payload}");
    io::Error::new(error.kind(), payload)
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
// Classification only: this fixed candidate is never an admission allowlist.
const ROOT_SERVICE_CANDIDATE: &str = "TrustedInstaller";
struct ServiceHandle(Handle);
impl Drop for ServiceHandle {
    fn drop(&mut self) {
        unsafe {
            CloseServiceHandle(self.0);
        }
    }
}
#[derive(Default)]
struct RootProbe {
    resolved: bool,
    identity_verified: bool,
    configuration_verified: bool,
    owner_matches: Option<bool>,
    operation: &'static str,
    code: Option<i32>,
}
impl RootProbe {
    fn category(&self) -> &'static str {
        if !self.resolved || !self.identity_verified || !self.configuration_verified {
            "indeterminate"
        } else if self.owner_matches == Some(true) {
            "verified_candidate_service_sid"
        } else if self.owner_matches == Some(false) {
            "other"
        } else {
            "indeterminate"
        }
    }
    fn render(&self) -> String {
        format!("windows.root_owner_probe: depth=0; role=volume_root; class={}; resolved={}; identity_verified={}; configuration_verified={}; owner_matches={:?}; operation={}; code={:?}",
            self.category(), self.resolved, self.identity_verified, self.configuration_verified, self.owner_matches, self.operation, self.code)
    }
    fn failed(mut self, operation: &'static str) -> Self {
        self.operation = operation;
        self.code = io::Error::last_os_error().raw_os_error();
        self
    }
}
fn lookup_field_matches(field: &[u16], expected: &str) -> bool {
    let Some(end) = field.iter().position(|c| *c == 0) else {
        return false;
    };
    String::from_utf16(&field[..end]).is_ok_and(|value| value.eq_ignore_ascii_case(expected))
}
fn root_probe_required(valid: bool, category: &str, context: Option<(usize, &str)>) -> bool {
    valid && category == "other" && context == Some((0, "volume_root"))
}
fn owner_sid_category(
    owner: *const c_void,
    current: *const c_void,
    system: *const c_void,
    administrators: *const c_void,
) -> (bool, &'static str) {
    let valid = !owner.is_null() && unsafe { IsValidSid(owner) } != 0;
    let category = if !valid {
        "invalid"
    } else if unsafe { EqualSid(owner, current) } != 0 {
        "current"
    } else if unsafe { EqualSid(owner, system) } != 0 {
        "system"
    } else if unsafe { EqualSid(owner, administrators) } != 0 {
        "administrators"
    } else {
        "other"
    };
    (valid, category)
}

fn probe_root_service_owner(owner: *const c_void) -> RootProbe {
    // Qualified service SID; resolution may consult domains. Never print identities.
    let qualified: Vec<u16> = format!("NT SERVICE\\{ROOT_SERVICE_CANDIDATE}")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut sid = [0usize; 68usize.div_ceil(std::mem::size_of::<usize>())]; // Aligned storage, at least SECURITY_MAX_SID_SIZE (68).
    let mut sid_size = 68u32;
    let mut domain = [0u16; 257];
    let mut domain_size = domain.len() as u32;
    let mut sid_use = 0u32;
    let mut report = RootProbe::default();
    if unsafe {
        LookupAccountNameW(
            std::ptr::null(),
            qualified.as_ptr(),
            sid.as_mut_ptr().cast(),
            &mut sid_size,
            domain.as_mut_ptr(),
            &mut domain_size,
            &mut sid_use,
        )
    } == 0
    {
        return report.failed("lookup_candidate");
    }
    // Check the returned SID fits before native validation dereferences it.
    let bytes = unsafe { std::slice::from_raw_parts(sid.as_ptr().cast::<u8>(), 68) };
    if !(8..=68).contains(&sid_size)
        || 8 + 4 * bytes[1] as usize > sid_size as usize
        || unsafe { IsValidSid(sid.as_ptr().cast()) } == 0
    {
        report.operation = "validate_candidate_sid";
        return report;
    }
    report.resolved = true;
    report.owner_matches = Some(unsafe { EqualSid(owner, sid.as_ptr().cast()) } != 0);
    let mut name = [0u16; 257];
    let mut name_size = name.len() as u32;
    let mut reverse_domain = [0u16; 257];
    let mut reverse_domain_size = reverse_domain.len() as u32;
    let mut reverse_use = 0u32;
    if unsafe {
        LookupAccountSidW(
            std::ptr::null(),
            sid.as_ptr().cast(),
            name.as_mut_ptr(),
            &mut name_size,
            reverse_domain.as_mut_ptr(),
            &mut reverse_domain_size,
            &mut reverse_use,
        )
    } == 0
    {
        return report.failed("verify_candidate_identity");
    }
    report.identity_verified = lookup_field_matches(&domain, "NT SERVICE")
        && lookup_field_matches(&reverse_domain, "NT SERVICE")
        && lookup_field_matches(&name, ROOT_SERVICE_CANDIDATE)
        && sid_use == reverse_use;
    if !report.identity_verified {
        report.operation = "candidate_identity_mismatch";
        return report;
    }
    // SC_MANAGER_CONNECT and SERVICE_QUERY_CONFIG only. No start/change rights.
    let manager = unsafe { OpenSCManagerW(std::ptr::null(), std::ptr::null(), 1) };
    if manager.is_null() {
        return report.failed("open_local_scm");
    }
    let manager = ServiceHandle(manager);
    let service_name: Vec<u16> = ROOT_SERVICE_CANDIDATE
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let service = unsafe { OpenServiceW(manager.0, service_name.as_ptr(), 1) };
    if service.is_null() {
        return report.failed("open_exact_service");
    }
    let service = ServiceHandle(service);
    // SERVICE_CONFIG_SERVICE_SID_INFO is a DWORD SERVICE_SID_INFO structure.
    let mut sid_type = 0u32;
    let mut needed = 0u32;
    if unsafe {
        QueryServiceConfig2W(
            service.0,
            5,
            (&mut sid_type as *mut u32).cast(),
            4,
            &mut needed,
        )
    } == 0
    {
        return report.failed("query_service_sid_configuration");
    }
    // Only documented enabled service-SID configurations count as verification.
    report.configuration_verified = matches!(sid_type, 1 | 3);
    report.operation = if report.configuration_verified {
        "classification_complete"
    } else {
        "service_sid_disabled_or_unknown"
    };
    report
}

// Diagnostics are reachable only from a synthetic test's thread-local scope.
// The exact environment opt-in alone cannot activate production observations.
#[cfg(test)]
thread_local! {
    static SYNTHETIC_ROOT_OBSERVATION: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}
#[cfg(test)]
pub(super) fn with_synthetic_root_observation<T>(run: impl FnOnce() -> T) -> T {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            SYNTHETIC_ROOT_OBSERVATION.with(|active| active.set(None));
        }
    }
    SYNTHETIC_ROOT_OBSERVATION.with(|active| {
        assert!(active.get().is_none());
        active.set(Some(false));
    });
    let _reset = Reset;
    run()
}
fn root_diagnostics_enabled() -> bool {
    #[cfg(test)]
    {
        std::env::var_os("DPT_WINDOWS_ROOT_DIAGNOSTICS").is_some_and(|v| v == "1")
            && SYNTHETIC_ROOT_OBSERVATION.with(|active| {
                if active.get() == Some(false) {
                    active.set(Some(true));
                    true
                } else {
                    false
                }
            })
    }
    #[cfg(not(test))]
    {
        false
    }
}
#[repr(C)]
#[derive(Default, PartialEq, Eq)]
struct RootIdentity {
    volume: u64,
    id: [u8; 16],
}
fn root_identity(file: &File) -> io::Result<RootIdentity> {
    let mut identity = RootIdentity::default();
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle().cast(),
            18,
            (&mut identity as *mut RootIdentity).cast(),
            std::mem::size_of::<RootIdentity>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(identity)
}
#[derive(Debug, Default, Clone, Copy)]
struct RootFacts {
    network_namespace: bool,
    query_code: Option<i32>,
    reparse: bool,
    guid_root: bool,
    local_fixed: bool,
    ntfs: bool,
    persistent_acls: bool,
    identity_matches: bool,
}
fn root_class(facts: RootFacts) -> &'static str {
    if facts.reparse {
        "reparse_point"
    } else if facts.network_namespace {
        "network_namespace"
    } else if !facts.guid_root {
        "ambiguous_or_network_namespace"
    } else if !facts.local_fixed {
        "unsupported_or_network_volume"
    } else if !facts.ntfs || !facts.persistent_acls {
        "unsupported_filesystem"
    } else if !facts.identity_matches {
        "root_identity_mismatch"
    } else {
        "verified_local_ntfs_root"
    }
}
// Only accept the complete GUID root form; no descendant, UNC or device namespace.
fn guid_root(value: &[u16]) -> bool {
    let Ok(text) = String::from_utf16(value) else {
        return false;
    };
    let bytes = text.as_bytes();
    bytes.len() == 49
        && text.starts_with("\\\\?\\Volume{")
        && text.ends_with("}\\")
        && bytes[11..47].iter().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                *b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
#[derive(Debug)]
struct QueryError {
    operation: &'static str,
    code: Option<i32>,
    primary: Option<i32>,
    fallback: &'static str,
}
impl QueryError {
    fn new(operation: &'static str, error: io::Error) -> Self {
        Self {
            operation,
            code: error.raw_os_error(),
            primary: None,
            fallback: "not_attempted",
        }
    }
}
trait RootQueries {
    type Held;
    fn reparse(&self, held: Option<&Self::Held>) -> io::Result<bool>;
    fn final_name(&self, opened: bool) -> io::Result<Vec<u16>>;
    fn fixed(&self, root: &[u16]) -> bool;
    fn volume(&self) -> io::Result<(bool, bool)>;
    fn open(&self, root: &[u16]) -> io::Result<Self::Held>;
    fn identity(&self, held: Option<&Self::Held>) -> io::Result<RootIdentity>;
}
struct NativeRootQueries<'a>(&'a File);
impl RootQueries for NativeRootQueries<'_> {
    type Held = File;
    fn reparse(&self, held: Option<&File>) -> io::Result<bool> {
        use std::os::windows::fs::MetadataExt;
        Ok(held.unwrap_or(self.0).metadata()?.file_attributes() & 0x400 != 0)
    }
    fn final_name(&self, opened: bool) -> io::Result<Vec<u16>> {
        // Memory is bounded; these synchronous queries still have no latency bound.
        let mut buffer = vec![0u16; if opened { 1024 } else { 64 }];
        let count = unsafe {
            GetFinalPathNameByHandleW(
                self.0.as_raw_handle().cast(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                if opened { 8 } else { 1 },
            )
        };
        if count == 0 {
            return Err(io::Error::last_os_error());
        }
        if count as usize >= buffer.len() {
            return Err(io::Error::from_raw_os_error(122)); // ERROR_INSUFFICIENT_BUFFER; derived from API size result.
        }
        buffer.truncate(count as usize);
        Ok(buffer)
    }
    fn fixed(&self, root: &[u16]) -> bool {
        let terminated: Vec<u16> = root.iter().copied().chain(Some(0)).collect();
        unsafe { GetDriveTypeW(terminated.as_ptr()) == 3 }
    }
    fn volume(&self) -> io::Result<(bool, bool)> {
        let mut filesystem = [0u16; 32];
        let mut flags = 0;
        if unsafe {
            GetVolumeInformationByHandleW(
                self.0.as_raw_handle().cast(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut flags,
                filesystem.as_mut_ptr(),
                32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok((lookup_field_matches(&filesystem, "NTFS"), flags & 8 != 0))
    }
    fn open(&self, root: &[u16]) -> io::Result<File> {
        let terminated: Vec<u16> = root.iter().copied().chain(Some(0)).collect();
        let handle = unsafe {
            CreateFileW(
                terminated.as_ptr(),
                DIRECTORY_GUARD_ACCESS,
                3,
                std::ptr::null(),
                3,
                0x02200000,
                std::ptr::null_mut(),
            )
        };
        if handle as isize == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_handle(handle) })
    }
    fn identity(&self, held: Option<&File>) -> io::Result<RootIdentity> {
        root_identity(held.unwrap_or(self.0))
    }
}
fn query_root_with<Q: RootQueries>(query: &Q) -> Result<RootFacts, QueryError> {
    let mut facts = RootFacts {
        reparse: query
            .reparse(None)
            .map_err(|e| QueryError::new("original_metadata", e))?,
        ..RootFacts::default()
    };
    if facts.reparse {
        return Ok(facts);
    }
    let root = match query.final_name(false) {
        Ok(root) => root,
        Err(primary) => {
            let primary_code = primary.raw_os_error();
            if primary_code == Some(122) {
                // Oversized GUID-root result cannot be a complete root. Preserve the
                // original ambiguous classification rather than probing a descendant.
                facts.query_code = primary_code;
                return Ok(facts);
            }
            match query.final_name(true) {
                Ok(opened) => {
                    facts.network_namespace =
                        opened.starts_with(&"\\\\?\\UNC\\".encode_utf16().collect::<Vec<_>>());
                    if facts.network_namespace {
                        facts.query_code = primary_code;
                        return Ok(facts);
                    }
                    return Err(QueryError {
                        operation: "guid_final_path",
                        code: primary_code,
                        primary: primary_code,
                        fallback: "non_unc_namespace",
                    });
                }
                Err(error) => {
                    return Err(QueryError {
                        operation: "opened_unc_final_path",
                        code: error.raw_os_error(),
                        primary: primary_code,
                        fallback: "query_failed",
                    })
                }
            }
        }
    };
    facts.guid_root = guid_root(&root);
    if !facts.guid_root {
        return Ok(facts);
    }
    facts.local_fixed = query.fixed(&root);
    if !facts.local_fixed {
        return Ok(facts);
    }
    (facts.ntfs, facts.persistent_acls) = query
        .volume()
        .map_err(|e| QueryError::new("volume_information", e))?;
    if !facts.ntfs || !facts.persistent_acls {
        return Ok(facts);
    }
    let independent = query
        .open(&root)
        .map_err(|e| QueryError::new("independent_root_open", e))?;
    facts.reparse = query
        .reparse(Some(&independent))
        .map_err(|e| QueryError::new("independent_metadata", e))?;
    if facts.reparse {
        return Ok(facts);
    }
    let original = query
        .identity(None)
        .map_err(|e| QueryError::new("original_file_id", e))?;
    let other = query
        .identity(Some(&independent))
        .map_err(|e| QueryError::new("independent_file_id", e))?;
    let rechecked = query
        .identity(None)
        .map_err(|e| QueryError::new("original_file_id_recheck", e))?;
    facts.identity_matches = original == other && original == rechecked;
    Ok(facts)
}
fn query_root(file: &File) -> Result<RootFacts, QueryError> {
    query_root_with(&NativeRootQueries(file))
}
fn render_root_query(result: Result<RootFacts, QueryError>) -> String {
    match result {
        Ok(facts) => format!("windows.root_namespace: depth=0; role=volume_root; class={}; identity_matches={}; persistent_acls={}; primary_guid_code={:?}; fallback={}",
            root_class(facts), facts.identity_matches, facts.persistent_acls, facts.query_code,
            if facts.network_namespace { "unc_namespace" } else { "not_attempted" }),
        Err(error) => format!("windows.root_namespace: depth=0; role=volume_root; class=indeterminate; operation={}; code={:?}; primary_guid_code={:?}; fallback={}",
            error.operation, error.code, error.primary, error.fallback),
    }
}
fn diagnose_namespace_root(
    file: &File,
    acl: *const c_void,
    current: &LocalMemory,
    system: &LocalMemory,
    administrators: &LocalMemory,
) {
    eprintln!("{}", render_root_query(query_root(file)));
    // Evaluate the SAME DACL policy independently of owner refusal. This is not AccessCheck.
    match assess_dacl(acl, current, system, administrators, true, false) {
        Ok(reason) => eprintln!(
            "windows.root_dacl: depth=0; role=volume_root; class={}; owner_authorized=false",
            reason.unwrap_or("policy_pass")
        ),
        Err(error) => {
            let payload = error
                .get_ref()
                .and_then(|e| e.downcast_ref::<DiagnosticError>());
            eprintln!("windows.root_dacl: depth=0; role=volume_root; class=indeterminate; operation={}; code={:?}",
                payload.map_or("dacl_query", |e| e.operation),
                payload.map_or(error.raw_os_error(), |e| e.code));
        }
    }
}

fn ancestor_error(operation: &str, error: io::Error, context: Option<(usize, &str)>) -> io::Error {
    if let Some((depth, role)) = context {
        let message = format!(
            "{operation}: code={:?}; class={:?}; depth={depth}; role={role}",
            error.raw_os_error(),
            error.kind()
        );
        eprintln!("{message}");
        io::Error::new(error.kind(), message)
    } else {
        diagnostic(operation, error)
    }
}

fn inspection_handle(path: &Path) -> io::Result<File> {
    inspection_handle_at(path, None)
}
fn inspection_handle_at(path: &Path, context: Option<(usize, &str)>) -> io::Result<File> {
    let path =
        wide(path).map_err(|error| ancestor_error("windows.security_path", error, context))?;
    // Directory ancestry requires LIST_DIRECTORY for effective no-delete sharing;
    // ordinary file security inspection still requests metadata access only.
    let access = if context.is_some() { DIRECTORY_GUARD_ACCESS } else { 0x20080 };
    // OPEN_EXISTING, do not follow the final reparse point, support directories.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            access,
            3,
            std::ptr::null(),
            3,
            0x02200000,
            std::ptr::null_mut(),
        )
    };
    if handle as isize == -1 {
        return Err(ancestor_error(
            "windows.open_security_handle",
            io::Error::last_os_error(),
            context,
        ));
    }
    Ok(unsafe { File::from_raw_handle(handle) })
}
pub fn is_private(path: &Path) -> io::Result<bool> {
    crate::private_storage::check_path(path)?;
    is_private_file(&inspection_handle(path)?)
}

#[cfg(all(debug_assertions, feature = "windows-dev-isolation"))]
pub(super) fn development_private_object(path: &Path) -> io::Result<bool> {
    crate::private_storage::check_path(path)?;
    let file = inspection_handle(path)?;
    if !is_private_file(&file)? { return Ok(false); }
    if file.metadata()?.is_file() {
        // Refuse hard-linked configuration/databases/sidecars, so an isolated
        // pathname cannot alias an existing file outside the development root.
        #[repr(C)]
        struct StandardInfo { allocation: i64, end: i64, links: u32, pending: u8, directory: u8 }
        let mut info = StandardInfo { allocation: 0, end: 0, links: 0, pending: 0, directory: 0 };
        if unsafe { GetFileInformationByHandleEx(file.as_raw_handle().cast(), 1,
            (&mut info as *mut StandardInfo).cast(), std::mem::size_of::<StandardInfo>() as u32) } == 0 {
            return Err(io::Error::last_os_error());
        }
        return Ok(info.links == 1 && info.pending == 0 && info.directory == 0);
    }
    Ok(file.metadata()?.is_dir())
}
fn refused(classification: &str) -> bool {
    // Never print paths, identities or ACL contents.
    eprintln!("windows.inspect_private: code=none; class={classification}");
    false
}
fn is_private_file(file: &File) -> io::Result<bool> {
    inspect_access(file, false, false)
}
fn inspect_access(file: &File, ancestor: bool, require_inheritance: bool) -> io::Result<bool> {
    match assess_access(file, ancestor, require_inheritance, None)? {
        Some(reason) => Ok(refused(reason)),
        None => Ok(true),
    }
}

// Only static role/category labels and component counts may enter diagnostics.
fn assess_access(
    file: &File,
    ancestor: bool,
    require_inheritance: bool,
    context: Option<(usize, &str)>,
) -> io::Result<Option<&'static str>> {
    assess_access_with_owner_policy(file, ancestor, require_inheritance, context, false)
}
fn assess_access_with_owner_policy(file: &File, ancestor: bool, require_inheritance: bool,
    context: Option<(usize, &str)>, current_owner_only: bool) -> io::Result<Option<&'static str>> {
    use std::os::windows::fs::MetadataExt;
    if let Some((depth, role)) = context {
        eprintln!("windows.ancestor.assessment: depth={depth}; role={role}; ancestor_mode={ancestor}; require_inheritance={require_inheritance}");
    }
    if file
        .metadata()
        .map_err(|error| ancestor_error("windows.ancestor.metadata", error, context))?
        .file_attributes()
        & 0x400
        != 0
    {
        return Ok(Some("reparse_point"));
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
        return Err(ancestor_error(
            "windows.get_security_info",
            io::Error::from_raw_os_error(status as i32),
            context,
        ));
    }
    let _descriptor = LocalMemory(descriptor);
    let current = binary_sid(&user_sid()?)?;
    let system = binary_sid("S-1-5-18")?;
    let administrators = binary_sid("S-1-5-32-544")?;
    let (owner_valid, owner_category) =
        owner_sid_category(owner, current.0, system.0, administrators.0);
    if let Some((depth, role)) = context {
        eprintln!("windows.ancestor.owner: depth={depth}; role={role}; sid_valid={owner_valid}; owner_category={owner_category}");
    }
    if !ordinary_owner_allowed(owner, &current, &system, &administrators, ancestor && !current_owner_only)
    {
        if approved_root_owner_scope(ancestor, require_inheritance, context) && owner_valid {
            // Explicit production policy authorization, 2026-10-15: only this
            // exact owner at a proven volume root. Its implicit WRITE_DAC risk
            // is accepted; no service name, signature or trustee is allowlisted.
            let candidate = binary_sid(TRUSTED_INSTALLER_ROOT_SID)?;
            if unsafe { EqualSid(owner, candidate.0) } != 0
                && trusted_installer_root_proof(owner_valid, owner, candidate.0, query_root(file)) {
                return assess_dacl(acl, &current, &system, &administrators, ancestor, require_inheritance);
            }
        }
        if root_probe_required(owner_valid, owner_category, context) && root_diagnostics_enabled() {
            diagnose_namespace_root(file, acl, &current, &system, &administrators);
            // No account or SCM queries: their latency cannot be bounded here.
        }
        return Ok(Some("owner_mismatch"));
    }
    if current_owner_only {
        return assess_dacl_with_file_sharing(acl, &current, &system, &administrators, false, false, ancestor);
    }
    assess_dacl(
        acl,
        &current,
        &system,
        &administrators,
        ancestor,
        require_inheritance,
    )
}

fn ordinary_owner_allowed(owner: *const c_void, current: &LocalMemory, system: &LocalMemory,
    administrators: &LocalMemory, ancestor: bool) -> bool {
    !owner.is_null() && unsafe { IsValidSid(owner) } != 0
        && (unsafe { EqualSid(owner, current.0) } != 0
            || ancestor && (unsafe { EqualSid(owner, system.0) } != 0
                || unsafe { EqualSid(owner, administrators.0) } != 0))
}

#[cfg(test)]
pub(super) fn test_owner_rights_cannot_authorize_foreign_owner() {
    let current = binary_sid(&user_sid().unwrap()).unwrap();
    let system = binary_sid("S-1-5-18").unwrap();
    let administrators = binary_sid("S-1-5-32-544").unwrap();
    // Controlled native descriptor; no unsupported foreign-owner assignment on
    // disk (normal tokens cannot assign Administrators: ERROR_INVALID_OWNER).
    let text: Vec<u16> = format!("O:LSD:P(A;OICI;FA;;;OW)(A;OICI;FA;;;{})", user_sid().unwrap())
        .encode_utf16().chain(Some(0)).collect();
    let mut raw = std::ptr::null_mut();
    assert_ne!(unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(text.as_ptr(),1,&mut raw,std::ptr::null_mut()) },0);
    let descriptor = LocalMemory(raw);
    let mut owner = std::ptr::null_mut(); let mut defaulted=0;
    assert_ne!(unsafe { GetSecurityDescriptorOwner(descriptor.0,&mut owner,&mut defaulted) },0);
    let mut acl=std::ptr::null_mut(); let mut present=0;
    assert_ne!(unsafe { GetSecurityDescriptorDacl(descriptor.0,&mut present,&mut acl,&mut defaulted) },0);
    assert_ne!(present,0);
    assert_eq!(assess_dacl(acl,&current,&system,&administrators,false,false).unwrap(),None);
    assert!(!ordinary_owner_allowed(owner,&current,&system,&administrators,false));
    assert!(!ordinary_owner_allowed(owner,&current,&system,&administrators,true));
    assert!(ordinary_owner_allowed(current.0,&current,&system,&administrators,false));
}

const TRUSTED_INSTALLER_ROOT_SID: &str = "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464";

fn approved_root_owner_scope(ancestor: bool, require_inheritance: bool, context: Option<(usize, &str)>) -> bool {
    ancestor && !require_inheritance && context == Some((0, "volume_root"))
}

fn trusted_installer_root_proof(valid: bool, owner: *const c_void, candidate: *const c_void,
    facts: Result<RootFacts, QueryError>) -> bool {
    valid && unsafe { EqualSid(owner, candidate) } != 0
        && facts.is_ok_and(|facts| facts.query_code.is_none() && root_class(facts) == "verified_local_ntfs_root")
}

// Shared policy evaluator: diagnostics never bypass or alter admission.
fn assess_dacl(
    acl: *const c_void,
    current: &LocalMemory,
    system: &LocalMemory,
    administrators: &LocalMemory,
    ancestor: bool,
    require_inheritance: bool,
) -> io::Result<Option<&'static str>> {
    assess_dacl_with_file_sharing(acl, current, system, administrators, ancestor, require_inheritance, false)
}
fn assess_dacl_with_file_sharing(acl: *const c_void, current: &LocalMemory, system: &LocalMemory,
    administrators: &LocalMemory, ancestor: bool, require_inheritance: bool, file_read_sharing: bool)
    -> io::Result<Option<&'static str>> {
    if acl.is_null() || unsafe { IsValidAcl(acl) } == 0 {
        return Ok(Some("null_or_invalid_dacl"));
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
        return Err(dacl_diagnostic(
            "windows.get_acl_information",
            io::Error::last_os_error(),
        ));
    }
    let owner_rights = binary_sid("S-1-3-4")?;
    let creator_owner = binary_sid("S-1-3-0")?;
    let mut inheritable_current_full_access = false;
    for index in 0..info.count {
        let mut ace = std::ptr::null_mut();
        if unsafe { GetAce(acl, index, &mut ace) } == 0 {
            return Err(dacl_diagnostic(
                "windows.get_ace",
                io::Error::last_os_error(),
            ));
        }
        // Only basic ALLOW/DENY ACEs have this layout. Unknown forms fail closed.
        let bytes = ace.cast::<u8>();
        let kind = unsafe { *bytes };
        if kind == 1 {
            continue;
        }
        if kind != 0 {
            return Ok(Some("unsupported_ace"));
        }
        let size = unsafe { std::ptr::read_unaligned(bytes.add(2).cast::<u16>()) } as usize;
        if size < 16 {
            return Ok(Some("invalid_ace_size"));
        }
        let trustee = unsafe { bytes.add(8) };
        let sid_size = 8 + 4 * unsafe { *trustee.add(1) } as usize;
        if sid_size > size - 8 || unsafe { IsValidSid(trustee.cast()) } == 0 {
            return Ok(Some("invalid_ace_sid"));
        }
        let matches = |sid: &LocalMemory| unsafe { EqualSid(trustee.cast(), sid.0) != 0 };
        let inherit_only = unsafe { *bytes.add(1) } & 0x08 != 0;
        let access = unsafe { std::ptr::read_unaligned(bytes.add(4).cast::<u32>()) };
        if matches(current)
            && unsafe { *bytes.add(1) } & 0x01 != 0
            && (access & 0x001f01ff == 0x001f01ff || access & 0x10000000 != 0)
        {
            inheritable_current_full_access = true;
        }
        // For existing ancestor directories, allow read/traverse and child
        // creation, but refuse broad delete, reparse/attribute or ACL changes.
        // Inherit-only ACEs do not grant access to this ancestor itself.
        const SUBSTITUTION_ACCESS: u32 = 0x10000000 | 0x40000000 | 0x000d0000 | 0x00000152;
        if ancestor && (inherit_only || access & SUBSTITUTION_ACCESS == 0) {
            continue;
        }
        // FILE_APPEND_DATA has the same bit as directory ADD_SUBDIRECTORY.
        // Directory additive-access rules must never admit file append access.
        const FILE_READ_ONLY_ACCESS: u32 = 0xa01200a9;
        if file_read_sharing && (inherit_only || access & !FILE_READ_ONLY_ACCESS == 0) {
            continue;
        }
        if !matches(current)
            && !matches(system)
            && !matches(administrators)
            && !matches(&owner_rights)
            && !(inherit_only && matches(&creator_owner))
        {
            return Ok(Some("untrusted_allow_ace"));
        }
    }
    if require_inheritance && !inheritable_current_full_access {
        return Ok(Some("missing_inheritable_current_access"));
    }
    Ok(None)
}
/// Hold no-delete-sharing handles while checking the whole existing chain.
/// No ACL repair, privilege enablement or pathname fallback is permitted.
pub fn database_ancestor_guard(path: &Path) -> io::Result<Vec<File>> {
    ancestor_guard(path, true)
}

fn ancestor_guard(path: &Path, private_parent: bool) -> io::Result<Vec<File>> {
    #[cfg(all(debug_assertions, feature = "windows-dev-isolation"))]
    let _development_scope = crate::development::database_scope(path)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let absolute = std::path::absolute(parent)?;
    let mut prefix = std::path::PathBuf::new();
    let mut held = Vec::new();
    for component in absolute.components() {
        prefix.push(component);
        if matches!(component, std::path::Component::Prefix(_)) {
            continue;
        }
        let depth = held.len();
        let role = if matches!(component, std::path::Component::RootDir) {
            "volume_root"
        } else if prefix == absolute {
            "immediate_parent"
        } else {
            "existing_ancestor"
        };
        eprintln!("windows.ancestor.open: depth={depth}; role={role}; access={DIRECTORY_GUARD_ACCESS:#x}; share=3");
        let context = Some((depth, role));
        let file = inspection_handle_at(&prefix, context)?;
        let reason = if !file
            .metadata()
            .map_err(|error| ancestor_error("windows.ancestor.metadata", error, context))?
            .is_dir()
        {
            Some("not_directory")
        } else {
            assess_access(&file, true, false, Some((depth, role)))?
        };
        if let Some(reason) = reason {
            let message = format!("windows.database_ancestor_guard: code=none; class=unsafe_ancestor; reason={reason}; depth={depth}; role={role}");
            eprintln!("{message}");
            return Err(io::Error::other(message));
        }
        held.push((prefix.clone(), file));
    }
    let Some((_, parent)) = held.last() else {
        return Err(io::Error::other(
            "windows.database_ancestor_guard: code=none; class=missing_parent",
        ));
    };
    if private_parent { if let Some(reason) = assess_access(
        parent,
        false,
        true,
        Some((held.len() - 1, "immediate_parent")),
    )? {
        let message = format!("windows.database_ancestor_guard: code=none; class=unsafe_sidecar_inheritance; reason={reason}; depth={}; role=immediate_parent", held.len() - 1);
        eprintln!("{message}");
        return Err(io::Error::other(message));
    } }
    for (depth, (name, file)) in held.iter().enumerate() {
        let role = if depth == 0 {
            "volume_root"
        } else if name == &absolute {
            "immediate_parent"
        } else {
            "existing_ancestor"
        };
        let context = Some((depth, role));
        let original = same_file::Handle::from_file(
            file.try_clone()
                .map_err(|error| ancestor_error("windows.ancestor.clone", error, context))?,
        )
        .map_err(|error| ancestor_error("windows.ancestor.original_identity", error, context))?;
        let current = same_file::Handle::from_file(inspection_handle_at(name, context)?)
            .map_err(|error| ancestor_error("windows.ancestor.recheck_identity", error, context))?;
        if original != current {
            return Err(io::Error::other(
                "windows.database_ancestor_guard: code=none; class=path_substitution",
            ));
        }
    }
    Ok(held.into_iter().map(|(_, file)| file).collect())
}

// Private files are created with FILE_ATTRIBUTE_NORMAL, not TEMPORARY. Rename
// directly on the same volume; never clear read-only, copy across volumes, or
// fall back to a permission repair. Native acceptance remains to be verified.
// Test probes use the identical operation but retain the structured OS code.
#[cfg(test)]
pub fn publication_probe(file: &File, source: &Path, destination: &Path, overwrite: bool) -> io::Result<()> {
    publication::publish(file, source, destination, overwrite)
}

// Opaque snapshots let tests compare policy without rendering identities/ACLs.
#[cfg(test)]
pub struct TestSecurityPolicy(String);
// Snapshot only: retaining an observation handle would itself affect the
// directory rename regression. Fixtures keep the objects alive until cleanup.
#[cfg(test)]
#[derive(PartialEq, Eq)]
pub struct TestFileIdentity(RootIdentity);
#[cfg(test)]
impl TestFileIdentity {
    pub fn capture(path: &Path) -> io::Result<Self> {
        Ok(Self(root_identity(&inspection_handle(path)?)?))
    }
}
#[cfg(test)]
impl TestSecurityPolicy {
    pub fn capture(path: &Path) -> io::Result<Self> {
        Ok(Self(sddl(&security(path)?, 7)?))
    }
    pub fn matches(&self, path: &Path) -> io::Result<bool> {
        Ok(self.0 == sddl(&security(path)?, 7)?)
    }
}

pub fn preserve_access(temporary: &Path, destination: &Path) -> io::Result<()> {
    // Refuse unsafe destination grants before applying them to new bytes.
    let _publication = publication_guard(temporary, destination)?;
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
        return Err(diagnostic(
            "windows.GetSecurityDescriptorControl",
            io::Error::last_os_error(),
        ));
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
        return Err(diagnostic(
            "windows.SetFileSecurityW",
            io::Error::last_os_error(),
        ));
    }
    std::fs::set_permissions(temporary, std::fs::metadata(destination)?.permissions())
        .map_err(|error| diagnostic("windows.preserve_readonly", error))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phase1_windows_approved_root_native_scope_and_policy_preservation() {
        let path = Path::new(r"C:\");
        let policy = TestSecurityPolicy::capture(path).unwrap();
        let file = inspection_handle(path).unwrap();
        let before = root_identity(&file).unwrap();
        let mut owner = std::ptr::null_mut();
        let mut descriptor = std::ptr::null_mut();
        assert_eq!(unsafe { GetSecurityInfo(file.as_raw_handle().cast(), 1, 1,
            &mut owner, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), &mut descriptor) }, 0);
        let _descriptor = LocalMemory(descriptor);
        let exact = binary_sid(TRUSTED_INSTALLER_ROOT_SID).unwrap();
        assert_ne!(unsafe { IsValidSid(owner) }, 0);
        assert_ne!(unsafe { EqualSid(owner, exact.0) }, 0, "Molly regression requires the observed exact root owner");
        assert_eq!(root_class(query_root(&file).unwrap()), "verified_local_ntfs_root");
        assert_eq!(assess_access(&file, true, false, Some((0, "volume_root"))).unwrap(), None);
        for (ancestor, inheritance, context) in [
            (true, false, Some((1, "existing_ancestor"))),
            (true, false, Some((0, "immediate_parent"))),
            (false, false, Some((0, "volume_root"))),
            (true, true, Some((0, "volume_root"))),
            (false, false, None),
        ] {
            assert_eq!(assess_access(&file, ancestor, inheritance, context).unwrap(), Some("owner_mismatch"));
        }
        assert!(before == root_identity(&file).unwrap());
        assert!(policy.matches(path).unwrap());
    }
    #[test]
    fn phase1_windows_approved_root_requires_exact_owner_and_complete_proof() {
        assert!(approved_root_owner_scope(true, false, Some((0, "volume_root"))));
        for context in [None, Some((1, "volume_root")), Some((0, "existing_ancestor")), Some((1, "immediate_parent"))] {
            assert!(!approved_root_owner_scope(true, false, context));
        }
        assert!(!approved_root_owner_scope(false, false, Some((0, "volume_root"))));
        assert!(!approved_root_owner_scope(true, true, Some((0, "volume_root"))));
        let owner = binary_sid(TRUSTED_INSTALLER_ROOT_SID).unwrap();
        let foreign = binary_sid("S-1-5-19").unwrap();
        let positive = RootFacts { guid_root: true, local_fixed: true, ntfs: true,
            persistent_acls: true, identity_matches: true, ..RootFacts::default() };
        assert!(trusted_installer_root_proof(true, owner.0, owner.0, Ok(positive)));
        assert!(!trusted_installer_root_proof(false, owner.0, owner.0, Ok(positive)));
        assert!(!trusted_installer_root_proof(true, foreign.0, owner.0, Ok(positive)));
        for facts in [
            RootFacts { query_code: Some(5), ..positive },
            RootFacts { network_namespace: true, ..positive },
            RootFacts { reparse: true, ..positive },
            RootFacts { guid_root: false, ..positive },
            RootFacts { local_fixed: false, ..positive },
            RootFacts { ntfs: false, ..positive },
            RootFacts { persistent_acls: false, ..positive },
            RootFacts { identity_matches: false, ..positive },
        ] {
            assert!(!trusted_installer_root_proof(true, owner.0, owner.0, Ok(facts)));
        }
        assert!(!trusted_installer_root_proof(true, owner.0, owner.0,
            Err(QueryError::new("synthetic", io::Error::from_raw_os_error(5)))));
    }
    struct SyntheticHeld(std::rc::Rc<std::cell::Cell<usize>>);
    impl Drop for SyntheticHeld {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    struct SyntheticQueries {
        fail: &'static str,
        guid_failure: bool,
        oversized: bool,
        unc: bool,
        mismatch: bool,
        substitute: bool,
        originals: std::cell::Cell<usize>,
        opened: std::cell::Cell<usize>,
        closed: std::rc::Rc<std::cell::Cell<usize>>,
    }
    impl SyntheticQueries {
        fn new(fail: &'static str) -> Self {
            Self {
                fail,
                guid_failure: false,
                oversized: false,
                unc: false,
                mismatch: false,
                substitute: false,
                originals: std::cell::Cell::new(0),
                opened: std::cell::Cell::new(0),
                closed: std::rc::Rc::new(std::cell::Cell::new(0)),
            }
        }
        fn stage(&self, stage: &'static str) -> io::Result<()> {
            if self.fail == stage {
                Err(io::Error::from_raw_os_error(5))
            } else {
                Ok(())
            }
        }
    }
    impl RootQueries for SyntheticQueries {
        type Held = SyntheticHeld;
        fn reparse(&self, held: Option<&SyntheticHeld>) -> io::Result<bool> {
            self.stage(if held.is_some() {
                "independent_metadata"
            } else {
                "original_metadata"
            })?;
            Ok(held.is_some() && self.substitute)
        }
        fn final_name(&self, opened: bool) -> io::Result<Vec<u16>> {
            if !opened && self.oversized {
                return Err(io::Error::from_raw_os_error(122));
            }
            if !opened && self.guid_failure {
                return Err(io::Error::from_raw_os_error(3));
            }
            self.stage(if opened {
                "opened_unc_final_path"
            } else {
                "guid_final_path"
            })?;
            Ok(if opened && self.unc {
                "\\\\?\\UNC\\synthetic\\share\\"
            } else {
                "\\\\?\\Volume{01234567-89ab-cdef-0123-456789abcdef}\\"
            }
            .encode_utf16()
            .collect())
        }
        fn fixed(&self, _: &[u16]) -> bool {
            true
        }
        fn volume(&self) -> io::Result<(bool, bool)> {
            self.stage("volume_information")?;
            Ok((true, true))
        }
        fn open(&self, _: &[u16]) -> io::Result<SyntheticHeld> {
            self.stage("independent_root_open")?;
            self.opened.set(self.opened.get() + 1);
            Ok(SyntheticHeld(self.closed.clone()))
        }
        fn identity(&self, held: Option<&SyntheticHeld>) -> io::Result<RootIdentity> {
            let stage = if held.is_some() {
                "independent_file_id"
            } else {
                let count = self.originals.get();
                self.originals.set(count + 1);
                if count == 0 {
                    "original_file_id"
                } else {
                    "original_file_id_recheck"
                }
            };
            self.stage(stage)?;
            Ok(RootIdentity {
                volume: 1,
                id: [if held.is_some() && self.mismatch {
                    2
                } else {
                    1
                }; 16],
            })
        }
    }
    #[test]
    fn phase1_windows_root_query_stages_and_raii_cleanup() {
        let positive = SyntheticQueries::new("");
        assert_eq!(
            root_class(query_root_with(&positive).unwrap()),
            "verified_local_ntfs_root"
        );
        assert_eq!((positive.opened.get(), positive.closed.get()), (1, 1));
        for stage in [
            "original_metadata",
            "guid_final_path",
            "volume_information",
            "independent_root_open",
            "independent_metadata",
            "original_file_id",
            "independent_file_id",
            "original_file_id_recheck",
        ] {
            let query = SyntheticQueries::new(stage);
            let error = query_root_with(&query).unwrap_err();
            assert_eq!(error.operation, stage);
            assert_eq!(error.code, Some(5));
            assert_eq!(query.opened.get(), query.closed.get());
            if matches!(
                stage,
                "independent_metadata"
                    | "original_file_id"
                    | "independent_file_id"
                    | "original_file_id_recheck"
            ) {
                assert_eq!(query.closed.get(), 1);
            } else {
                assert_eq!(query.closed.get(), 0);
            }
        }
        let mut mismatch = SyntheticQueries::new("");
        mismatch.mismatch = true;
        assert_eq!(
            root_class(query_root_with(&mismatch).unwrap()),
            "root_identity_mismatch"
        );
        assert_eq!(mismatch.closed.get(), 1);
        let mut substitute = SyntheticQueries::new("");
        substitute.substitute = true;
        assert_eq!(
            root_class(query_root_with(&substitute).unwrap()),
            "reparse_point"
        );
        assert_eq!(substitute.closed.get(), 1);
    }
    #[test]
    fn phase1_windows_root_query_retains_primary_and_fallback_outcomes() {
        let mut failed = SyntheticQueries::new("opened_unc_final_path");
        failed.guid_failure = true;
        let error = query_root_with(&failed).unwrap_err();
        assert_eq!(error.primary, Some(3));
        assert_eq!(error.code, Some(5));
        assert_eq!(error.fallback, "query_failed");
        assert_eq!(error.operation, "opened_unc_final_path");
        let rendered = render_root_query(Err(error));
        assert!(rendered.contains("depth=0; role=volume_root"));
        assert!(rendered.contains("primary_guid_code=Some(3)"));
        let mut non_unc = SyntheticQueries::new("");
        non_unc.guid_failure = true;
        let error = query_root_with(&non_unc).unwrap_err();
        assert_eq!(error.primary, Some(3));
        assert_eq!(error.fallback, "non_unc_namespace");
        let mut unc = SyntheticQueries::new("");
        unc.guid_failure = true;
        unc.unc = true;
        let facts = query_root_with(&unc).unwrap();
        assert_eq!(root_class(facts), "network_namespace");
        assert_eq!(facts.query_code, Some(3));
        assert_eq!(unc.opened.get(), 0);
        let mut oversized = SyntheticQueries::new("");
        oversized.oversized = true;
        let facts = query_root_with(&oversized).unwrap();
        assert_eq!(root_class(facts), "ambiguous_or_network_namespace");
        assert_eq!(facts.query_code, Some(122));
        assert_eq!(oversized.opened.get(), 0);
    }
    #[test]
    fn phase1_windows_dacl_wrapped_errors_preserve_raw_code_and_operation() {
        for operation in ["windows.get_acl_information", "windows.get_ace"] {
            let error = dacl_diagnostic(operation, io::Error::from_raw_os_error(5));
            let payload = error
                .get_ref()
                .unwrap()
                .downcast_ref::<DiagnosticError>()
                .unwrap();
            assert_eq!(payload.operation, operation);
            assert_eq!(payload.code, Some(5));
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
            assert_eq!(
                error.to_string(),
                format!("{operation}: code=Some(5); class=PermissionDenied")
            );
        }
    }
    #[test]
    fn phase1_windows_root_namespace_injected_classification() {
        let positive = RootFacts {
            guid_root: true,
            local_fixed: true,
            ntfs: true,
            persistent_acls: true,
            identity_matches: true,
            reparse: false,
            ..RootFacts::default()
        };
        assert_eq!(root_class(positive), "verified_local_ntfs_root");
        for (facts, expected) in [
            (
                RootFacts {
                    network_namespace: true,
                    ..positive
                },
                "network_namespace",
            ),
            (
                RootFacts {
                    reparse: true,
                    ..positive
                },
                "reparse_point",
            ),
            (
                RootFacts {
                    guid_root: false,
                    ..positive
                },
                "ambiguous_or_network_namespace",
            ),
            (
                RootFacts {
                    local_fixed: false,
                    ..positive
                },
                "unsupported_or_network_volume",
            ),
            (
                RootFacts {
                    ntfs: false,
                    ..positive
                },
                "unsupported_filesystem",
            ),
            (
                RootFacts {
                    persistent_acls: false,
                    ..positive
                },
                "unsupported_filesystem",
            ),
            (
                RootFacts {
                    identity_matches: false,
                    ..positive
                },
                "root_identity_mismatch",
            ),
        ] {
            assert_eq!(root_class(facts), expected);
        }
        let root = "\\\\?\\Volume{01234567-89ab-cdef-0123-456789abcdef}\\";
        assert!(guid_root(&root.encode_utf16().collect::<Vec<_>>()));
        for value in [
            "\\\\server\\share\\",
            "\\\\?\\UNC\\server\\share\\",
            "C:\\",
            "\\\\?\\GLOBALROOT\\",
            &format!("{root}child"),
            "\\\\?\\Volume{zzzzzzzz-89ab-cdef-0123-456789abcdef}\\",
        ] {
            assert!(!guid_root(&value.encode_utf16().collect::<Vec<_>>()));
        }
        assert!(!guid_root(&[0xd800]));
    }
    #[test]
    fn phase1_windows_root_failed_queries_never_render_sensitive_error_text() {
        for code in [5, 6, 3, 50, 87] {
            let rendered = render_root_query(Err(QueryError::new(
                "original_metadata",
                io::Error::from_raw_os_error(code),
            )));
            assert!(rendered.contains("class=indeterminate"));
            assert!(rendered.contains(&format!("code=Some({code})")));
            assert!(!rendered.contains("verified_local_ntfs_root"));
        }
        let rendered = render_root_query(Err(QueryError::new(
            "original_metadata",
            io::Error::other("sensitive synthetic path and identity"),
        )));
        assert!(!rendered.contains("sensitive"));
        assert!(rendered.contains("code=None"));
    }
    #[test]
    fn phase1_windows_root_dacl_synthetic_policy_does_not_authorize_owner() {
        // Inject a candidate/foreign owner and DACL; never alter a machine root.
        let current = binary_sid("S-1-1-0").unwrap();
        let system = binary_sid("S-1-5-18").unwrap();
        let administrators = binary_sid("S-1-5-32-544").unwrap();
        for (text, expected) in [
            ("O:LS D:P(A;;FA;;;WD)", None),
            ("O:LS D:P(A;;FA;;;LS)", Some("untrusted_allow_ace")),
            ("O:LS D:P(A;;WD;;;LS)", Some("untrusted_allow_ace")),
            ("O:LS D:P(A;;FA;;;OW)", None),
        ] {
            let text: Vec<u16> = text
                .replace(' ', "")
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
            let mut present = 0;
            let mut defaulted = 0;
            let mut acl = std::ptr::null_mut();
            assert_ne!(
                unsafe {
                    GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut acl, &mut defaulted)
                },
                0
            );
            assert_ne!(present, 0);
            assert_eq!(
                assess_dacl(acl, &current, &system, &administrators, true, false).unwrap(),
                expected
            );
            // Owner is still foreign even when its DACL passes the conservative policy.
            let foreign = binary_sid("S-1-5-19").unwrap();
            assert_eq!(
                owner_sid_category(foreign.0, current.0, system.0, administrators.0),
                (true, "other")
            );
        }
        assert_eq!(
            assess_dacl(
                std::ptr::null(),
                &current,
                &system,
                &administrators,
                true,
                false
            )
            .unwrap(),
            Some("null_or_invalid_dacl")
        );
    }
    #[test]
    fn phase1_windows_root_query_fixture_preserves_identity_and_policy() {
        let root = crate::private_storage::tempdir().unwrap();
        let file = inspection_handle(root.path()).unwrap();
        let before = root_identity(&file).unwrap();
        let policy = TestSecurityPolicy::capture(root.path()).unwrap();
        let facts = query_root(&file).unwrap(); // An ordinary private directory is NOT a volume root.
        assert_eq!(root_class(facts), "ambiguous_or_network_namespace");
        assert!(before == root_identity(&file).unwrap());
        assert!(policy.matches(root.path()).unwrap());
        // Invalid handles produce a structured native query failure; never a proof.
        let mut buffer = [0u16; 64];
        assert_eq!(
            unsafe { GetFinalPathNameByHandleW(std::ptr::null_mut(), buffer.as_mut_ptr(), 64, 1) },
            0
        );
        assert!(io::Error::last_os_error().raw_os_error().is_some());
    }
    #[test]
    fn phase1_windows_root_probe_gate_and_native_owner_categories() {
        let current = binary_sid("S-1-1-0").unwrap(); // Synthetic current stand-in.
        let system = binary_sid("S-1-5-18").unwrap();
        let administrators = binary_sid("S-1-5-32-544").unwrap();
        let other = binary_sid("S-1-5-19").unwrap();
        for (owner, expected) in [
            (current.0, "current"),
            (system.0, "system"),
            (administrators.0, "administrators"),
            (other.0, "other"),
        ] {
            let (valid, category) =
                owner_sid_category(owner, current.0, system.0, administrators.0);
            assert!(valid);
            assert_eq!(category, expected);
            assert_eq!(
                root_probe_required(valid, category, Some((0, "volume_root"))),
                expected == "other"
            );
        }
        assert_eq!(
            owner_sid_category(std::ptr::null(), current.0, system.0, administrators.0),
            (false, "invalid")
        );
        assert!(!root_probe_required(
            false,
            "other",
            Some((0, "volume_root"))
        ));
        assert!(!root_probe_required(
            true,
            "other",
            Some((1, "existing_ancestor"))
        ));
        assert!(!root_probe_required(
            true,
            "other",
            Some((0, "immediate_parent"))
        ));
        assert!(!root_probe_required(true, "other", None));
    }
    #[test]
    fn phase1_windows_root_probe_unresolved_unverified_and_false_match() {
        let mut report = RootProbe::default();
        assert_eq!(report.category(), "indeterminate");
        report.resolved = true;
        report.owner_matches = Some(true);
        assert_eq!(report.category(), "indeterminate");
        report.identity_verified = true;
        assert_eq!(report.category(), "indeterminate");
        report.configuration_verified = true;
        assert_eq!(report.category(), "verified_candidate_service_sid");
        let system = binary_sid("S-1-5-18").unwrap();
        let other = binary_sid("S-1-5-19").unwrap();
        report.owner_matches = Some(unsafe { EqualSid(system.0, other.0) } != 0);
        assert_eq!(report.category(), "other");
        report.configuration_verified = false;
        assert_eq!(report.category(), "indeterminate");
        assert!(lookup_field_matches(
            &"nt service\0".encode_utf16().collect::<Vec<_>>(),
            "NT SERVICE"
        ));
        assert!(!lookup_field_matches(
            &"synthetic wrong domain\0"
                .encode_utf16()
                .collect::<Vec<_>>(),
            "NT SERVICE"
        ));
        assert!(!lookup_field_matches(&[0xd800, 0], ROOT_SERVICE_CANDIDATE));
    }
    #[test]
    fn phase1_windows_synthetic_observation_is_single_use_and_unwind_safe() {
        // Isolated subprocess environments avoid mutating the parallel test process.
        if std::env::var_os("DPT_SYNTHETIC_GATE_CHILD").is_none() {
            for setting in [None, Some("0"), Some("true"), Some("1")] {
                let mut command = std::process::Command::new(std::env::current_exe().unwrap());
                command.args([
                    "private_storage::platform::tests::phase1_windows_synthetic_observation_is_single_use_and_unwind_safe",
                    "--exact", "--test-threads=1",
                ]).env("DPT_SYNTHETIC_GATE_CHILD", "1")
                  .env_remove("DPT_WINDOWS_ROOT_DIAGNOSTICS");
                if let Some(value) = setting {
                    command.env("DPT_WINDOWS_ROOT_DIAGNOSTICS", value);
                }
                let output = command.output().unwrap();
                assert!(
                    output.status.success(),
                    "windows.synthetic.gate: class=child_failure; code={:?}",
                    output.status.code()
                );
                assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
            }
            return;
        }
        let enabled = std::env::var_os("DPT_WINDOWS_ROOT_DIAGNOSTICS").is_some_and(|v| v == "1");
        assert!(!root_diagnostics_enabled());
        with_synthetic_root_observation(|| {
            assert!(!std::thread::spawn(root_diagnostics_enabled).join().unwrap());
            assert_eq!(root_diagnostics_enabled(), enabled);
            assert!(!root_diagnostics_enabled());
            // A consumed observation must not permit nested rearming.
            assert!(std::panic::catch_unwind(|| with_synthetic_root_observation(|| ())).is_err());
            assert!(!root_diagnostics_enabled());
        });
        assert!(!root_diagnostics_enabled());
        assert!(std::panic::catch_unwind(|| {
            with_synthetic_root_observation(|| panic!("synthetic unwind"));
        })
        .is_err());
        assert!(!root_diagnostics_enabled());
        with_synthetic_root_observation(|| assert_eq!(root_diagnostics_enabled(), enabled));
        assert!(!root_diagnostics_enabled());
    }
    #[test]
    fn phase1_windows_root_probe_diagnostic_privacy() {
        let report = RootProbe {
            operation: "lookup_candidate",
            code: Some(1332),
            ..RootProbe::default()
        };
        let rendered = report.render();
        assert!(rendered.contains("class=indeterminate"));
        assert!(rendered.contains("code=Some(1332)"));
        for forbidden in [
            ROOT_SERVICE_CANDIDATE,
            "NT SERVICE",
            "S-1-",
            "domain=",
            "path=",
            "username=",
            "acl=",
        ] {
            assert!(!rendered.contains(forbidden));
        }
    }
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

#[cfg(test)]
mod about_read_only_policy_tests {
    use super::*;

    #[test]
    fn about_windows_dacl_assessor_refuses_untrusted_allow_without_repair() {
        // Tests the unchanged evaluator on one owned synthetic directory, not
        // admission of its full ancestry or successful SQLite opening.
        let root = crate::private_storage::tempdir().unwrap();
        let parent = root.path().join("synthetic-parent");
        crate::private_storage::create_private_dir(&parent).unwrap();
        assert_eq!(
            assess_access(&inspection_handle(&parent).unwrap(), true, false, None).unwrap(),
            None
        );
        let mut raw = std::ptr::null_mut();
        let descriptor_text: Vec<u16> = "D:P(A;OICI;FA;;;WD)".encode_utf16().chain(Some(0)).collect();
        assert_ne!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    descriptor_text.as_ptr(),
                    1,
                    &mut raw,
                    std::ptr::null_mut(),
                )
            },
            0,
            "about.synthetic: descriptor_failure"
        );
        let descriptor = LocalMemory(raw);
        assert_ne!(
            unsafe { SetFileSecurityW(wide(&parent).unwrap().as_ptr(), 0x80000004, descriptor.0) },
            0,
            "about.synthetic: fixture_dacl_failure"
        );
        let policy = TestSecurityPolicy::capture(&parent).unwrap();
        assert_eq!(
            assess_access(&inspection_handle(&parent).unwrap(), true, false, None).unwrap(),
            Some("untrusted_allow_ace")
        );
        assert!(policy.matches(&parent).unwrap());
        assert_eq!(std::fs::read_dir(&parent).unwrap().count(), 0);
    }
}
