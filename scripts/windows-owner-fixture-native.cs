// Standalone synthetic verification only; never linked into the application.
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Security.AccessControl;
using System.Security.Principal;
using Microsoft.Win32.SafeHandles;
public sealed class DptOwnerFixtureIdentity {
    internal uint Volume, High, Low;
}
public static class DptOwnerFixtureNative {
    [StructLayout(LayoutKind.Sequential)] struct SecurityAttributes {
        public uint Length; public IntPtr Descriptor; public int Inherit;
    }
    [StructLayout(LayoutKind.Sequential)] struct Stamp { public uint Low, High; }
    [StructLayout(LayoutKind.Sequential)] struct FileInfo {
        public uint Attributes; public Stamp Created, Accessed, Written;
        public uint Volume, SizeHigh, SizeLow, Links, IndexHigh, IndexLow;
    }
    [DllImport("advapi32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern bool ConvertStringSecurityDescriptorToSecurityDescriptorW(string text, uint revision, out IntPtr descriptor, IntPtr size);
    [DllImport("advapi32.dll")]
    static extern uint GetSecurityInfo(SafeFileHandle handle, uint type, uint information, out IntPtr owner, IntPtr group, out IntPtr dacl, IntPtr sacl, out IntPtr descriptor);
    [DllImport("advapi32.dll")]
    static extern uint GetSecurityDescriptorLength(IntPtr descriptor);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern bool CreateDirectoryW(string path, ref SecurityAttributes security);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern SafeFileHandle CreateFileW(string path, uint access, uint share, ref SecurityAttributes security, uint disposition, uint flags, IntPtr template);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true, EntryPoint="CreateFileW")]
    static extern SafeFileHandle OpenFile(string path, uint access, uint share, IntPtr security, uint disposition, uint flags, IntPtr template);
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool GetFileInformationByHandle(SafeFileHandle file, out FileInfo info);
    [DllImport("kernel32.dll")]
    static extern IntPtr LocalFree(IntPtr memory);
    public static void Create(string path, bool directory) {
        string user;
        using (WindowsIdentity identity = WindowsIdentity.GetCurrent()) { user = identity.User.Value; }
        string flags = directory ? "OICI" : "";
        string sddl = "O:" + user + "D:P(A;" + flags + ";FA;;;" + user + ")(A;" + flags + ";FA;;;SY)";
        IntPtr descriptor;
        if (!ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl, 1, out descriptor, IntPtr.Zero))
            throw new Win32Exception(Marshal.GetLastWin32Error());
        try {
            SecurityAttributes sa = new SecurityAttributes { Length = (uint)Marshal.SizeOf(typeof(SecurityAttributes)), Descriptor = descriptor, Inherit = 0 };
            if (directory) {
                if (!CreateDirectoryW(path, ref sa)) throw new Win32Exception(Marshal.GetLastWin32Error());
            } else {
                using (SafeFileHandle file = CreateFileW(path, 0xC0000000, 7, ref sa, 1, 0x80, IntPtr.Zero)) {
                    if (file.IsInvalid) throw new Win32Exception(Marshal.GetLastWin32Error());
                }
            }
        } finally { LocalFree(descriptor); }
    }
    static SafeFileHandle Open(string path) {
        SafeFileHandle file = OpenFile(path, 0x20080, 7, IntPtr.Zero, 3, 0x02200000, IntPtr.Zero);
        if (file.IsInvalid) { int error = Marshal.GetLastWin32Error(); file.Dispose(); throw new Win32Exception(error); }
        return file;
    }
    static FileInfo Information(SafeFileHandle file) {
        FileInfo info;
        if (!GetFileInformationByHandle(file, out info)) throw new Win32Exception(Marshal.GetLastWin32Error());
        if ((info.Attributes & 0x400) != 0) throw new InvalidOperationException("Reparse fixture refused");
        return info;
    }
    public static DptOwnerFixtureIdentity CaptureIdentity(string path) {
        using (SafeFileHandle file = Open(path)) {
            FileInfo info = Information(file);
            return new DptOwnerFixtureIdentity { Volume = info.Volume, High = info.IndexHigh, Low = info.IndexLow };
        }
    }
    public static bool MatchesIdentity(string path, DptOwnerFixtureIdentity expected) {
        if (expected == null) return false;
        using (SafeFileHandle file = Open(path)) {
            FileInfo info = Information(file);
            return info.Volume == expected.Volume && info.IndexHigh == expected.High && info.IndexLow == expected.Low;
        }
    }
    public static bool SameIdentity(string first, string second) {
        using (SafeFileHandle a = Open(first)) using (SafeFileHandle b = Open(second)) {
            FileInfo x = Information(a), y = Information(b);
            return x.Volume == y.Volume && x.IndexHigh == y.IndexHigh && x.IndexLow == y.IndexLow;
        }
    }
    // Pure validation also exercised locally with synthetic descriptors. This is
    // deliberately strict: actual owner, protected DACL, exactly user/System FA.
    public static bool PrivateDescriptor(byte[] bytes, SecurityIdentifier current) {
        RawSecurityDescriptor sd = new RawSecurityDescriptor(bytes, 0);
        if (sd.Owner == null || !sd.Owner.Equals(current) || sd.DiscretionaryAcl == null ||
            (sd.ControlFlags & ControlFlags.DiscretionaryAclProtected) == 0 || sd.DiscretionaryAcl.Count != 2) return false;
        SecurityIdentifier system = new SecurityIdentifier("S-1-5-18");
        bool userSeen = false, systemSeen = false;
        foreach (GenericAce raw in sd.DiscretionaryAcl) {
            CommonAce ace = raw as CommonAce;
            if (ace == null || ace.AceQualifier != AceQualifier.AccessAllowed || ace.AccessMask != 0x1f01ff || ace.IsInherited || ace.IsCallback) return false;
            if (ace.SecurityIdentifier.Equals(current)) userSeen = true;
            else if (ace.SecurityIdentifier.Equals(system)) systemSeen = true;
            else return false;
        }
        return userSeen && systemSeen;
    }
    public static bool IsPrivate(string path) {
        using (SafeFileHandle file = Open(path)) {
            Information(file);
            IntPtr owner, dacl, descriptor;
            uint status = GetSecurityInfo(file, 1, 5, out owner, IntPtr.Zero, out dacl, IntPtr.Zero, out descriptor);
            if (status != 0) throw new Win32Exception((int)status);
            try {
                uint length = GetSecurityDescriptorLength(descriptor);
                if (length == 0 || length > 65536) throw new InvalidOperationException("Invalid descriptor length");
                byte[] bytes = new byte[length]; Marshal.Copy(descriptor, bytes, 0, bytes.Length);
                using (WindowsIdentity identity = WindowsIdentity.GetCurrent()) { return PrivateDescriptor(bytes, identity.User); }
            } finally { LocalFree(descriptor); }
        }
    }
}
