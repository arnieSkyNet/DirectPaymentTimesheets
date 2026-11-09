#requires -Version 7.0
# Standalone synthetic owner/provider diagnostic. No Cargo, ACL repair or cleanup.
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'This verifier requires native Windows and NTFS' }
if ([IntPtr]::Size -ne 8 -or [Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString() -ne 'X64') {
    throw 'This verifier requires the native x64 execution context'
}
if ($pwd.Provider.Name -ne 'FileSystem') { throw 'Run this verifier from the filesystem checkout' }
# Reuse only the already-reviewed small token-owner type, never execute the
# package wrapper or Cargo. This matches its default-owner context for children.
$packageSource = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'windows-package-tests.ps1'))
$ownerType = [regex]::Match($packageSource, "(?s)Add-Type -TypeDefinition @'\r?\n(.*?)\r?\n'@")
if (-not $ownerType.Success -or -not $ownerType.Groups[1].Value.Contains('public static class DptCiDefaultOwner')) {
    throw 'Reviewed package default-owner helper was not found'
}
Add-Type -TypeDefinition $ownerType.Groups[1].Value
$previousOwner = [DptCiDefaultOwner]::Owner()
try {
    $current = [Security.Principal.WindowsIdentity]::GetCurrent().User
    [DptCiDefaultOwner]::SetOwner($current)
    if (-not ([DptCiDefaultOwner]::Owner().Equals($current))) {
        throw 'Package-equivalent default owner verification failed'
    }
Add-Type -Path (Join-Path $PSScriptRoot 'windows-owner-fixture-native.cs')
$query = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'windows-owner-query.ps1'))
$query64 = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($query))
$child = @'
$ErrorActionPreference = 'Stop'
try {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    $command = Get-Command Get-Acl -ErrorAction SilentlyContinue
    $context = [ordered]@{
        ps_major = $PSVersionTable.PSVersion.Major
        ps_minor = $PSVersionTable.PSVersion.Minor
        edition = $PSVersionTable.PSEdition
        runtime_major = [Environment]::Version.Major
        runtime_minor = [Environment]::Version.Minor
        bits = [IntPtr]::Size * 8
        filesystem_provider = ($pwd.Provider.Name -eq 'FileSystem')
        get_acl_cmdlet = ($null -ne $command -and $command.CommandType -eq 'Cmdlet')
        security_module = ($null -ne $command -and $command.ModuleName -eq 'Microsoft.PowerShell.Security')
        default_owner_is_user = $identity.Owner.Equals($identity.User)
        admin_role_enabled = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    }
    $identity.Dispose()
    [Console]::Out.WriteLine(('DPT_OWNER_CONTEXT|' + ($context | ConvertTo-Json -Compress)))
} catch {
    [Console]::Out.WriteLine('DPT_OWNER_CONTEXT_FAILED')
    exit 2
}
& ([ScriptBlock]::Create([Text.Encoding]::Unicode.GetString([Convert]::FromBase64String('__QUERY__'))))
'@
$child = $child.Replace('__QUERY__', $query64)
$kinds = @('none','provider_invocation','cmdlet_invocation','action_preference_stop','runtime',
    'privilege_not_held','unauthorized_access','win32','directory_not_found','file_not_found',
    'io','argument','not_supported','invalid_operation','system','other')
function Convert-DptFailure([string] $Text) {
    $parts = $Text.Split('|')
    if ($parts.Count -ne 16 -or $parts[0] -ne 'DPT_OWNER_QUERY_V2' -or
        $parts[1] -notin @('get_acl','get_owner','current_token','compare') -or
        $parts[2] -notin @('path_not_found','access_denied','exception') -or
        $parts[15] -notin @('get_acl_path_not_found','get_acl_operation_not_supported','get_acl_fail','other')) {
        return @{ classification = 'unclassified' }
    }
    foreach ($i in @(4,5,7,9)) {
        if ($parts[$i] -notin $kinds) { return @{ classification = 'unclassified' } }
    }
    $numbers = @{}
    foreach ($i in @(3,6,8,10,11,12,13,14)) {
        $value = 0
        if (-not [int]::TryParse($parts[$i], [ref]$value)) { return @{ classification = 'unclassified' } }
        $numbers[$i] = $value
    }
    return [ordered]@{
        stage = $parts[1]; category = $parts[2]; error_id = $parts[15]
        chain = @(
            @{ kind=$parts[4]; hresult=$numbers[3]; native_error=$numbers[11] },
            @{ kind=$parts[5]; hresult=$numbers[6]; native_error=$numbers[12] },
            @{ kind=$parts[7]; hresult=$numbers[8]; native_error=$numbers[13] },
            @{ kind=$parts[9]; hresult=$numbers[10]; native_error=$numbers[14] }
        )
    }
}
# Objects are exclusively created with explicit current-user ownership and
# protected current-user/System-only DACLs, matching the Rust fixture's intent.
# No existing objects/ACLs are modified; the scoped token default is restored.
$root = Join-Path $env:LOCALAPPDATA ('DptOwnerProbe-' + [guid]::NewGuid().ToString('N'))
if ([IO.DriveInfo]::new([IO.Path]::GetPathRoot($root)).DriveFormat -ne 'NTFS') {
    throw 'Native verification requires NTFS'
}
[DptOwnerFixtureNative]::Create($root, $true)
$file = Join-Path $root 'synthetic-file'
[DptOwnerFixtureNative]::Create($file, $false)
$shells = [ordered]@{
    windows_powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    powershell_core = (Get-Process -Id $PID).Path
}
$rows = @()
$stage = 'native_baseline'
try {
    foreach ($object in @(@{kind='directory';path=$root;identity=[DptOwnerFixtureNative]::CaptureIdentity($root)},
        @{kind='file';path=$file;identity=[DptOwnerFixtureNative]::CaptureIdentity($file)})) {
        $ordinary = $object.path
        $verbatim = '\\?\' + $ordinary
        if (-not [DptOwnerFixtureNative]::SameIdentity($ordinary,$verbatim) -or
            -not [DptOwnerFixtureNative]::IsPrivate($ordinary) -or
            -not [DptOwnerFixtureNative]::IsPrivate($verbatim)) {
            throw 'Mandatory native identity/owner/private-ACL baseline failed'
        }
        foreach ($shell in $shells.Keys) {
            foreach ($style in @('ordinary','verbatim')) {
                $stage = 'query'
                if (-not [DptOwnerFixtureNative]::MatchesIdentity($ordinary,$object.identity) -or
                    -not [DptOwnerFixtureNative]::MatchesIdentity($verbatim,$object.identity)) {
                    throw 'Original fixture identity changed before query'
                }
                $path = if ($style -eq 'ordinary') { $ordinary } else { $verbatim }
                $psi = [Diagnostics.ProcessStartInfo]::new()
                $psi.FileName = $shells[$shell]
                $psi.WorkingDirectory = $pwd.ProviderPath
                $psi.UseShellExecute = $false
                $psi.RedirectStandardOutput = $true
                $psi.RedirectStandardError = $true
                foreach ($arg in @('-NoLogo','-NoProfile','-NonInteractive','-Command',$child)) {
                    $psi.ArgumentList.Add($arg)
                }
                $psi.Environment['DPT_SYNTHETIC_PATH'] = $path
                $process = [Diagnostics.Process]::new()
                $process.StartInfo = $psi
                try {
                    if (-not $process.Start()) { throw 'Child shell did not start' }
                    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
                    $stderrTask = $process.StandardError.ReadToEndAsync()
                    if (-not $process.WaitForExit(20000)) {
                        $process.Kill($true); $process.WaitForExit()
                        throw 'Bounded synthetic child shell timed out'
                    }
                    $stdout = $stdoutTask.GetAwaiter().GetResult()
                    $stderr = $stderrTask.GetAwaiter().GetResult()
                    $exitCode = $process.ExitCode
                } finally { $process.Dispose() }
                $lines = @($stdout -split '\r?\n' | Where-Object { $_.Length -gt 0 })
                $contextLines = @($lines | Where-Object { $_.StartsWith('DPT_OWNER_CONTEXT|') })
                $probeLines = @($lines | Where-Object { -not $_.StartsWith('DPT_OWNER_CONTEXT|') })
                $context = $null
                if ($contextLines.Count -eq 1) {
                    $raw = $contextLines[0].Substring(18) | ConvertFrom-Json
                    # Project only known booleans/numbers and fixed edition names.
                    $edition = if ($raw.edition -in @('Desktop','Core')) { $raw.edition } else { 'other' }
                    $context = [ordered]@{ ps_major=[int]$raw.ps_major; ps_minor=[int]$raw.ps_minor; edition=$edition;
                        runtime_major=[int]$raw.runtime_major; runtime_minor=[int]$raw.runtime_minor; bits=[int]$raw.bits;
                        filesystem_provider=[bool]$raw.filesystem_provider; get_acl_cmdlet=[bool]$raw.get_acl_cmdlet;
                        security_module=[bool]$raw.security_module; default_owner_is_user=[bool]$raw.default_owner_is_user;
                        admin_role_enabled=[bool]$raw.admin_role_enabled }
                }
                $success = $exitCode -eq 0 -and $probeLines.Count -eq 1 -and $probeLines[0] -eq 'True' -and
                    $null -ne $context -and $context.get_acl_cmdlet -and $context.security_module -and
                    $context.filesystem_provider -and $context.bits -eq 64 -and $context.default_owner_is_user
                $failure = if ($probeLines.Count -eq 1) { Convert-DptFailure $probeLines[0] } else { @{classification='unclassified'} }
                $native = [DptOwnerFixtureNative]::SameIdentity($ordinary,$verbatim) -and
                    [DptOwnerFixtureNative]::MatchesIdentity($ordinary,$object.identity) -and
                    [DptOwnerFixtureNative]::MatchesIdentity($verbatim,$object.identity) -and
                    [DptOwnerFixtureNative]::IsPrivate($ordinary) -and [DptOwnerFixtureNative]::IsPrivate($verbatim)
                if (-not $native) { throw 'Mandatory native checks failed after query' }
                # Ordinary paths reproduce the current Rust fixture and must pass.
                # Verbatim queries are explicit provider comparisons, never fallbacks.
                $rows += [ordered]@{ shell=$shell; object=$object.kind; style=$style; required=($style -eq 'ordinary');
                    identity_owner_acl=$native; owner_query_passed=[bool]$success; exit_code=$exitCode;
                    stderr_present=($stderr.Length -gt 0); context=$context; failure=$failure }
            }
        }
    }
} catch {
    # Never format the exception or its message/path. Child query failures are
    # classified above; infrastructure failures report only fixed stage/code.
    [ordered]@{ infrastructure_failure=$true; stage=$stage; hresult=[int]$_.Exception.HResult } | ConvertTo-Json -Compress | Write-Output
    exit 1
}
$complete = $rows.Count -eq 8 -and @($rows | Where-Object { $_.required }).Count -eq 4
$passed = $complete -and @($rows | Where-Object { $_.required -and -not $_.owner_query_passed }).Count -eq 0
[ordered]@{ cases=$rows; complete_matrix=$complete; required_passed=$passed;
    synthetic_objects_retained=$true; native_execution=$true } | ConvertTo-Json -Depth 8 | Write-Output
if (-not $passed) { exit 1 }

} finally {
    [DptCiDefaultOwner]::SetOwner($previousOwner)
    if (-not ([DptCiDefaultOwner]::Owner().Equals($previousOwner))) {
        throw 'Original process default owner was not restored'
    }
}
