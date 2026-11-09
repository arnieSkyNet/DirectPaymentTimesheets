# Shared synthetic owner probe: included verbatim by Rust and the native verifier.
# Never emit exception messages, paths, account names, SIDs or raw error records.
$ErrorActionPreference = 'Stop'
function Get-DptExceptionKind($Exception) {
    if ($null -eq $Exception) { return 'none' }
    switch ($Exception.GetType().FullName) {
        'System.Management.Automation.ProviderInvocationException' { return 'provider_invocation' }
        'System.Management.Automation.CmdletInvocationException' { return 'cmdlet_invocation' }
        'System.Management.Automation.ActionPreferenceStopException' { return 'action_preference_stop' }
        'System.Management.Automation.RuntimeException' { return 'runtime' }
        'System.Security.AccessControl.PrivilegeNotHeldException' { return 'privilege_not_held' }
        'System.UnauthorizedAccessException' { return 'unauthorized_access' }
        'System.ComponentModel.Win32Exception' { return 'win32' }
        'System.IO.DirectoryNotFoundException' { return 'directory_not_found' }
        'System.IO.FileNotFoundException' { return 'file_not_found' }
        'System.IO.IOException' { return 'io' }
        'System.ArgumentException' { return 'argument' }
        'System.NotSupportedException' { return 'not_supported' }
        'System.InvalidOperationException' { return 'invalid_operation' }
        'System.SystemException' { return 'system' }
        default { return 'other' }
    }
}
$stage = 'get_acl'
try {
    $acl = Get-Acl -LiteralPath $env:DPT_SYNTHETIC_PATH -ErrorAction Stop
    $stage = 'get_owner'
    $owner = $acl.GetOwner([System.Security.Principal.SecurityIdentifier])
    $stage = 'current_token'
    $current = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
    $stage = 'compare'
    $owner.Equals($current)
} catch {
    $category = 'exception'
    $errorId = 'other'
    switch -Wildcard ($_.FullyQualifiedErrorId) {
        'GetAcl_PathNotFound,*' { $category = 'path_not_found'; $errorId = 'get_acl_path_not_found' }
        'GetAcl_OperationNotSupported,*' { $errorId = 'get_acl_operation_not_supported' }
        'GetAclFail,*' { $errorId = 'get_acl_fail' }
    }
    if ($_.CategoryInfo.Category -eq [Management.Automation.ErrorCategory]::PermissionDenied) {
        $category = 'access_denied'
    }
    # Keep at most four exception levels; absent levels have kind none/code zero.
    $kinds = @('none','none','none','none')
    $codes = @(0,0,0,0)
    $native = @(0,0,0,0)
    $exception = $_.Exception
    for ($i = 0; $i -lt 4 -and $null -ne $exception; $i++) {
        $kinds[$i] = Get-DptExceptionKind $exception
        $codes[$i] = [int]$exception.HResult
        if ($exception -is [ComponentModel.Win32Exception]) {
            $native[$i] = [int]$exception.NativeErrorCode
        }
        $exception = $exception.InnerException
    }
    $fields = @('DPT_OWNER_QUERY_V2',$stage,$category,$codes[0],$kinds[0],
        $kinds[1],$codes[1],$kinds[2],$codes[2],$kinds[3],$codes[3],
        $native[0],$native[1],$native[2],$native[3],$errorId)
    [Console]::Out.WriteLine(($fields -join '|'))
    exit 1
}
