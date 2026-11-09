#requires -Version 7.0
# Disposable CI test environment only. No existing directory/ACL is repaired.
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('x86_64-pc-windows-msvc')]
    [string] $Target
)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Windows native package tests require Windows' }

# Elevated hosted runners may default new file ownership to Administrators.
# Set only this PowerShell process token's default owner for its test children;
# retain all SIDs, privileges and admission policies, and restore it afterwards.
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Security.Principal;
public static class DptCiDefaultOwner {
    [DllImport("advapi32.dll", SetLastError=true)]
    static extern bool OpenProcessToken(IntPtr process, uint access, out IntPtr token);
    [DllImport("advapi32.dll", SetLastError=true)]
    static extern bool GetTokenInformation(IntPtr token, int kind, IntPtr buffer, uint length, out uint needed);
    [DllImport("advapi32.dll", SetLastError=true)]
    static extern bool SetTokenInformation(IntPtr token, int kind, IntPtr buffer, uint length);
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool CloseHandle(IntPtr handle);
    static IntPtr Open(uint access) {
        IntPtr token;
        if (!OpenProcessToken(new IntPtr(-1), access, out token))
            throw new Win32Exception(Marshal.GetLastWin32Error());
        return token;
    }
    public static SecurityIdentifier Owner() {
        IntPtr token = Open(0x8); // TOKEN_QUERY
        IntPtr buffer = IntPtr.Zero;
        try {
            uint needed;
            if (GetTokenInformation(token, 4, IntPtr.Zero, 0, out needed) ||
                Marshal.GetLastWin32Error() != 122 || needed < IntPtr.Size)
                throw new InvalidOperationException("Cannot size TOKEN_OWNER");
            buffer = Marshal.AllocHGlobal(checked((int)needed));
            if (!GetTokenInformation(token, 4, buffer, needed, out needed))
                throw new Win32Exception(Marshal.GetLastWin32Error());
            return new SecurityIdentifier(Marshal.ReadIntPtr(buffer));
        } finally {
            if (buffer != IntPtr.Zero) Marshal.FreeHGlobal(buffer);
            if (!CloseHandle(token)) throw new Win32Exception(Marshal.GetLastWin32Error());
        }
    }
    public static void SetOwner(SecurityIdentifier owner) {
        IntPtr token = Open(0x80); // TOKEN_ADJUST_DEFAULT, no privilege changes
        byte[] sid = new byte[owner.BinaryLength];
        owner.GetBinaryForm(sid, 0);
        GCHandle pinned = GCHandle.Alloc(sid, GCHandleType.Pinned);
        IntPtr buffer = IntPtr.Zero;
        try {
            buffer = Marshal.AllocHGlobal(IntPtr.Size);
            Marshal.WriteIntPtr(buffer, pinned.AddrOfPinnedObject());
            if (!SetTokenInformation(token, 4, buffer, (uint)IntPtr.Size))
                throw new Win32Exception(Marshal.GetLastWin32Error());
        } finally {
            if (buffer != IntPtr.Zero) Marshal.FreeHGlobal(buffer);
            pinned.Free();
            if (!CloseHandle(token)) throw new Win32Exception(Marshal.GetLastWin32Error());
        }
    }
}
'@
$previousOwner = [DptCiDefaultOwner]::Owner()
$previousTmp = $env:TMP
$previousTemp = $env:TEMP
$current = [Security.Principal.WindowsIdentity]::GetCurrent().User
try {
    [DptCiDefaultOwner]::SetOwner($current)
    if (-not ([DptCiDefaultOwner]::Owner().Equals($current))) {
        throw 'Test process default owner verification failed'
    }
    $root = [IO.DirectoryInfo]::new((Join-Path $env:LOCALAPPDATA ('DptPackageTests-' + [guid]::NewGuid().ToString('N'))))
    if ($root.Exists) { throw 'New private test root already exists' }
    $acl = [Security.AccessControl.DirectorySecurity]::new()
    $acl.SetOwner($current)
    $acl.SetAccessRuleProtection($true, $false)
    foreach ($sid in @($current, [Security.Principal.SecurityIdentifier]::new('S-1-5-18'))) {
        $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new(
            $sid, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow'))
    }
    [IO.FileSystemAclExtensions]::Create($root, $acl)
    $env:TMP = $root.FullName
    $env:TEMP = $root.FullName
    # Verify actual default-owner and inherited-policy behavior before Cargo.
    $probe = Join-Path $root.FullName 'ordinary-file-owner-probe'
    [IO.File]::WriteAllText($probe, 'synthetic owner probe')
    if (-not ((Get-Acl -LiteralPath $probe).GetOwner([Security.Principal.SecurityIdentifier]).Equals($current))) {
        throw 'Ordinary test-file owner verification failed'
    }
    if ([IO.DriveInfo]::new([IO.Path]::GetPathRoot($root.FullName)).DriveFormat -ne 'NTFS') {
        throw 'Native security fixtures require NTFS'
    }
    Write-Host 'Private NTFS test root and current-user default ownership verified; evidence retained.'
    # Run the ordinary suite without skips, diagnostic opt-ins or policy bypasses.
    & cargo test --locked --target $Target
    if ($LASTEXITCODE -ne 0) { throw 'Native Windows package tests failed' }
} finally {
    $env:TMP = $previousTmp
    $env:TEMP = $previousTemp
    [DptCiDefaultOwner]::SetOwner($previousOwner)
    if (-not ([DptCiDefaultOwner]::Owner().Equals($previousOwner))) {
        throw 'Original process default owner was not restored'
    }
}
