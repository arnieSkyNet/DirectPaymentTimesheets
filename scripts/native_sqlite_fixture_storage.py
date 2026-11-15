"""Private, synthetic-only terminal fixture storage. No production imports.

Refuse unfamiliar native ACLs rather than repair existing objects. These checks
are admission checks, not a SQLite VFS or protection against the same user.
"""
import ctypes
import os
from pathlib import Path
import platform
import stat
import tempfile
import uuid


class StorageRefusal(Exception):
    pass


def no_links(path):
    path = Path(path).absolute()
    for item in (*reversed(path.parents), path):
        metadata = item.lstat()
        if stat.S_ISLNK(metadata.st_mode) or getattr(metadata, 'st_file_attributes', 0) & 0x400:
            raise StorageRefusal('fixture_namespace_refused')
    return path


def mac_acl_empty(path):
    # Darwin sys/acl.h: ACL_TYPE_EXTENDED, ACL_FIRST_ENTRY. Unknown/error refuses.
    native = ctypes.CDLL('/usr/lib/libSystem.B.dylib', use_errno=True)
    native.acl_get_file.argtypes = [ctypes.c_char_p, ctypes.c_int]
    native.acl_get_file.restype = ctypes.c_void_p
    native.acl_get_entry.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.POINTER(ctypes.c_void_p)]
    native.acl_free.argtypes = [ctypes.c_void_p]
    native.acl_valid.argtypes = [ctypes.c_void_p]
    acl = native.acl_get_file(os.fsencode(path), 0x100)
    if not acl:
        raise StorageRefusal('fixture_acl_unconfirmed')
    try:
        if native.acl_valid(acl) != 0:
            raise StorageRefusal('fixture_acl_unconfirmed')
        entry = ctypes.c_void_p()
        ctypes.set_errno(0)
        result = native.acl_get_entry(acl, 0, ctypes.byref(entry))
        if result != -1 or ctypes.get_errno() != 22:
            raise StorageRefusal('fixture_acl_refused')
    finally:
        if native.acl_free(acl) != 0:
            raise StorageRefusal('fixture_acl_unconfirmed')


class WindowsSecurity:
    """Explicit private DACL at creation; inspect owner and every resulting ACE."""
    def __init__(self):
        from ctypes import wintypes as w
        self.w, self.ptr = w, ctypes.c_void_p
        self.adv = ctypes.WinDLL('advapi32', use_last_error=True)
        self.kernel = ctypes.WinDLL('kernel32', use_last_error=True)
        def bind(lib, name, result, args):
            fn = getattr(lib, name)
            fn.restype, fn.argtypes = result, args
            return fn
        ptr = self.ptr
        self.free = bind(self.kernel, 'LocalFree', ptr, [ptr])
        self.close = bind(self.kernel, 'CloseHandle', w.BOOL, [w.HANDLE])
        current = bind(self.kernel, 'GetCurrentProcess', w.HANDLE, [])
        architecture = bind(self.kernel, 'IsWow64Process2', w.BOOL, [w.HANDLE, ctypes.POINTER(w.USHORT), ctypes.POINTER(w.USHORT)])
        process_machine, native_machine = w.USHORT(), w.USHORT()
        self.require(architecture(current(), ctypes.byref(process_machine), ctypes.byref(native_machine)))
        self.require(process_machine.value == 0 and native_machine.value == 0x8664)
        token_open = bind(self.adv, 'OpenProcessToken', w.BOOL, [w.HANDLE, w.DWORD, ctypes.POINTER(w.HANDLE)])
        token_info = bind(self.adv, 'GetTokenInformation', w.BOOL, [w.HANDLE, ctypes.c_int, ptr, w.DWORD, ctypes.POINTER(w.DWORD)])
        self.sid_string = bind(self.adv, 'ConvertSidToStringSidW', w.BOOL, [ptr, ctypes.POINTER(w.LPWSTR)])
        self.convert = bind(self.adv, 'ConvertStringSecurityDescriptorToSecurityDescriptorW', w.BOOL, [w.LPCWSTR, w.DWORD, ctypes.POINTER(ptr), ptr])
        self.get = bind(self.adv, 'GetNamedSecurityInfoW', w.DWORD, [w.LPWSTR, ctypes.c_int, w.DWORD, ctypes.POINTER(ptr), ptr, ctypes.POINTER(ptr), ptr, ctypes.POINTER(ptr)])
        self.control = bind(self.adv, 'GetSecurityDescriptorControl', w.BOOL, [ptr, ctypes.POINTER(w.WORD), ctypes.POINTER(w.DWORD)])
        self.ace = bind(self.adv, 'GetAce', w.BOOL, [ptr, w.DWORD, ctypes.POINTER(ptr)])
        self.mkdir = bind(self.kernel, 'CreateDirectoryW', w.BOOL, [w.LPCWSTR, ptr])
        self.open = bind(self.kernel, 'CreateFileW', w.HANDLE, [w.LPCWSTR, w.DWORD, w.DWORD, ptr, w.DWORD, w.DWORD, w.HANDLE])
        token, length = w.HANDLE(), w.DWORD()
        self.require(token_open(current(), 8, ctypes.byref(token)))
        try:
            elevation = w.DWORD()
            self.require(token_info(token, 20, ctypes.byref(elevation), ctypes.sizeof(elevation), ctypes.byref(length)))
            self.require(elevation.value == 0)  # No elevated local/native test profile.
            token_info(token, 1, None, 0, ctypes.byref(length))
            self.require(length.value > 0)
            buffer = ctypes.create_string_buffer(length.value)
            self.require(token_info(token, 1, buffer, length, ctypes.byref(length)))
            self.user = self.sid(ctypes.cast(buffer, ctypes.POINTER(ptr))[0])
        finally:
            self.require(self.close(token))

    @staticmethod
    def require(ok):
        if not ok:
            raise StorageRefusal('fixture_security_unconfirmed')

    def sid(self, pointer):
        value = self.w.LPWSTR()
        self.require(self.sid_string(pointer, ctypes.byref(value)))
        try:
            return value.value
        finally:
            self.require(not self.free(ctypes.cast(value, self.ptr)))

    def create(self, path):
        class Attributes(ctypes.Structure):
            _fields_ = [('length', self.w.DWORD), ('descriptor', self.ptr), ('inherit', self.w.BOOL)]
        descriptor = self.ptr()
        # Only current token and SYSTEM; inheritable to fixture files/staging.
        self.require(self.convert(f'O:{self.user}D:P(A;OICI;FA;;;{self.user})(A;OICI;FA;;;SY)', 1, ctypes.byref(descriptor), None))
        try:
            attributes = Attributes(ctypes.sizeof(Attributes), descriptor, False)
            self.require(self.mkdir(str(path), ctypes.byref(attributes)))
        finally:
            self.require(not self.free(descriptor))

    def check(self, path, protected=False):
        owner, acl, descriptor = self.ptr(), self.ptr(), self.ptr()
        self.require(self.get(str(path), 1, 5, ctypes.byref(owner), None, ctypes.byref(acl), None, ctypes.byref(descriptor)) == 0)
        try:
            self.require(self.sid(owner) == self.user and bool(acl))
            control, revision = self.w.WORD(), self.w.DWORD()
            self.require(self.control(descriptor, ctypes.byref(control), ctypes.byref(revision)))
            self.require(not protected or control.value & 0x1000)
            # ACL header: revision/u8, reserved/u8, size/u16, ACE count/u16.
            count = ctypes.c_ushort.from_address(acl.value + 4).value
            self.require(count > 0)
            for index in range(count):
                entry = self.ptr()
                self.require(self.ace(acl, index, ctypes.byref(entry)))
                header = (ctypes.c_ubyte * 4).from_address(entry.value)
                # Only ordinary ACCESS_ALLOWED_ACE, SID begins at offset 8.
                self.require(header[0] == 0 and int.from_bytes(bytes(header[2:4]), 'little') >= 20)
                self.require(self.sid(entry.value + 8) in (self.user, 'S-1-5-18'))
        finally:
            self.require(not self.free(descriptor))

    def hold(self, path):
        # No FILE_SHARE_DELETE: retain the root's Windows namespace identity.
        handle = self.open(str(path), 0x20080, 3, None, 3, 0x02200000, None)
        self.require(handle not in (None, ctypes.c_void_p(-1).value))
        return handle


class PrivateRoot:
    def __init__(self, prefix='dpt-native-sqlite-'):
        self.system = platform.system()
        self.security = WindowsSecurity() if self.system == 'Windows' else None
        base = no_links(Path(tempfile.gettempdir()).resolve(strict=True))
        self.path = base / (prefix + uuid.uuid4().hex)
        self.handle = None
        self.ancestors = []
        if self.security:
            # Retain each ancestor without DELETE sharing, not just inspect it.
            for ancestor in reversed(base.parents):
                self.ancestors.append(self.security.hold(ancestor))
            self.ancestors.append(self.security.hold(base))
            self.security.create(self.path)
            self.handle = self.security.hold(self.path)
        elif self.system == 'Darwin':
            for ancestor in (*reversed(base.parents), base):
                metadata = ancestor.lstat()
                mode = stat.S_IMODE(metadata.st_mode)
                trusted_sticky = mode & stat.S_ISVTX and metadata.st_uid in (0, os.geteuid())
                if metadata.st_uid not in (0, os.geteuid()) or (mode & 0o022 and not trusted_sticky):
                    raise StorageRefusal('fixture_ancestor_refused')
                mac_acl_empty(ancestor)
            os.mkdir(self.path, 0o700)
            self.handle = os.open(self.path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        else:
            raise StorageRefusal('unsupported_platform')
        self.identity = self.path.stat().st_dev, self.path.stat().st_ino
        self.verify()

    def verify(self):
        no_links(self.path)
        observed = self.path.stat()
        if (observed.st_dev, observed.st_ino) != self.identity:
            raise StorageRefusal('fixture_identity_changed')
        for path in (self.path, *self.path.rglob('*')):
            no_links(path)
            info = path.lstat()
            if not (stat.S_ISDIR(info.st_mode) or stat.S_ISREG(info.st_mode)) or (stat.S_ISREG(info.st_mode) and info.st_nlink != 1):
                raise StorageRefusal('fixture_object_refused')
            if self.security:
                self.security.check(path, protected=path == self.path)
            else:
                expected = 0o700 if stat.S_ISDIR(info.st_mode) else 0o600
                if info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != expected:
                    raise StorageRefusal('fixture_permissions_refused')
                mac_acl_empty(path)
        if not self.security:
            held = os.fstat(self.handle)
            if (held.st_dev, held.st_ino) != self.identity:
                raise StorageRefusal('fixture_identity_changed')

    def close(self):
        if self.handle is not None:
            if self.security:
                self.security.require(self.security.close(self.handle))
            else:
                os.close(self.handle)
            self.handle = None
        for handle in reversed(self.ancestors):
            self.security.require(self.security.close(handle))
        self.ancestors.clear()
