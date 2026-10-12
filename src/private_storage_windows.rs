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
    fn MoveFileExW(source: *const u16, destination: *const u16, flags: u32) -> i32;
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
    // Local, fully qualified service SID; never resolve the owner to a printable name.
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
    if owner.is_null()
        || unsafe { IsValidSid(owner) } == 0
        || (unsafe { EqualSid(owner, current.0) } == 0
            && !(ancestor
                && (unsafe { EqualSid(owner, system.0) } != 0
                    || unsafe { EqualSid(owner, administrators.0) } != 0)))
    {
        if root_probe_required(owner_valid, owner_category, context) {
            eprintln!("{}", probe_root_service_owner(owner).render());
        }
        return Ok(Some("owner_mismatch"));
    }
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
        return Err(diagnostic(
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
            return Err(diagnostic("windows.get_ace", io::Error::last_os_error()));
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
        if matches(&current)
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
        if !matches(&current)
            && !matches(&system)
            && !matches(&administrators)
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
        eprintln!("windows.ancestor.open: depth={depth}; role={role}; access=0x20080; share=3");
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
    if let Some(reason) = assess_access(
        parent,
        false,
        true,
        Some((held.len() - 1, "immediate_parent")),
    )? {
        let message = format!("windows.database_ancestor_guard: code=none; class=unsafe_sidecar_inheritance; reason={reason}; depth={}; role=immediate_parent", held.len() - 1);
        eprintln!("{message}");
        return Err(io::Error::other(message));
    }
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
pub fn publish(source: &Path, destination: &Path, overwrite: bool) -> io::Result<()> {
    let source = wide(source)?;
    let destination = wide(destination)?;
    move_file(&source, &destination, overwrite)
        .map_err(|error| diagnostic("windows.MoveFileExW", error))
}

// Test probes use the identical operation but retain the structured OS code.
#[cfg(test)]
pub fn publication_probe(source: &Path, destination: &Path, overwrite: bool) -> io::Result<()> {
    let source = wide(source)?;
    let destination = wide(destination)?;
    move_file(&source, &destination, overwrite)
}

// Opaque snapshots let tests compare policy without rendering identities/ACLs.
#[cfg(test)]
pub struct TestSecurityPolicy(String);
#[cfg(test)]
impl TestSecurityPolicy {
    pub fn capture(path: &Path) -> io::Result<Self> {
        Ok(Self(sddl(&security(path)?, 7)?))
    }
    pub fn matches(&self, path: &Path) -> io::Result<bool> {
        Ok(self.0 == sddl(&security(path)?, 7)?)
    }
}

fn move_file(source: &[u16], destination: &[u16], overwrite: bool) -> io::Result<()> {
    const MOVEFILE_REPLACE_EXISTING: u32 = 1;
    let flags = if overwrite {
        MOVEFILE_REPLACE_EXISTING
    } else {
        0
    };
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), flags) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
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
