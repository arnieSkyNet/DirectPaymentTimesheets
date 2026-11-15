# Prepared only. Operator desktop approval is required before any invocation.
param([switch]$ApprovedDesktopIdentityProbe)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
if (-not $ApprovedDesktopIdentityProbe) { exit 2 }
# Refuse SSH/noninteractive service contexts; never attempt to lower their token.
if ($env:SSH_CONNECTION -or $env:SSH_CLIENT -or -not [Environment]::UserInteractive) { exit 2 }
$result = [ordered]@{account=$null;integrity_level=$null;elevated=$null}
$identity = $null
$integrityBuffer = $null
$elevationBuffer = $null
$exitCode = 2
try {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $result.account = $identity.Name
    $flags = [Reflection.BindingFlags]'Static,NonPublic'
    $query = @([Security.Principal.WindowsIdentity].GetMethods($flags) | Where-Object {
        $_.Name -eq 'GetTokenInformation' -and -not $_.IsGenericMethod -and $_.GetParameters().Length -eq 2
    })
    if ($query.Count -ne 1) { throw 'query_unavailable' }
    $kind = $query[0].GetParameters()[1].ParameterType
    # Query only this process: TokenIntegrityLevel=25, TokenElevation=20.
    $integrityBuffer = $query[0].Invoke($null,@($identity.AccessToken,[Enum]::ToObject($kind,25)))
    $elevationBuffer = $query[0].Invoke($null,@($identity.AccessToken,[Enum]::ToObject($kind,20)))
    $sidPointer = [Runtime.InteropServices.Marshal]::ReadIntPtr($integrityBuffer.DangerousGetHandle())
    $sid = [Security.Principal.SecurityIdentifier]::new($sidPointer)
    $levels = @{'S-1-16-4096'='LOW';'S-1-16-8192'='MEDIUM';'S-1-16-8448'='MEDIUM_PLUS';'S-1-16-12288'='HIGH';'S-1-16-16384'='SYSTEM'}
    if (-not $levels.ContainsKey($sid.Value)) { throw 'integrity_unavailable' }
    $result.integrity_level = $levels[$sid.Value]
    $result.elevated = [Runtime.InteropServices.Marshal]::ReadInt32($elevationBuffer.DangerousGetHandle()) -ne 0
    if ($result.account -ieq 'IMOLLY\mark' -and $result.integrity_level -eq 'MEDIUM' -and $result.elevated -eq $false) { $exitCode = 0 }
} catch {
    $exitCode = 2
} finally {
    foreach ($resource in @($integrityBuffer,$elevationBuffer,$identity)) {
        if ($resource) { try { $resource.Dispose() } catch { $exitCode = 2 } }
    }
}
$result | ConvertTo-Json -Compress
exit $exitCode
