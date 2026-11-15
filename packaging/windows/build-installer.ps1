param(
    [ValidateSet('x86_64', 'arm64')]
    [string]$Architecture = 'x86_64',
    [string]$MakeNsisPath = 'makensis.exe',
    [string]$SourceBinary = '',
    [string]$OutputDirectory = ''
)

$ErrorActionPreference = 'Stop'
$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$manifest = Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') -Raw
$package = [regex]::Match($manifest, '(?ms)^\[package\]\s*\r?\n(.*?)(?=^\[|\z)').Groups[1].Value
$version = [regex]::Match($package, '(?m)^version\s*=\s*"(\d+\.\d+\.\d+)"\s*$').Groups[1].Value
if (-not $version) { throw 'Cargo.toml must contain a numeric package version.' }
$compiler = (Get-Command $MakeNsisPath -ErrorAction Stop).Source
$binary = if ($SourceBinary) { [System.IO.Path]::GetFullPath($SourceBinary) } else { Join-Path $root 'target/release/direct_payment_timesheets.exe' }
$target = if ($Architecture -eq 'arm64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
python (Join-Path $root 'packaging/package.py') verify-binary $binary $target windows
if ($LASTEXITCODE -ne 0) { throw 'Binary provenance does not match this checkout and target.' }
$metadata = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($binary)
if ($metadata.ProductVersion -ne $version -or $metadata.FileVersion -ne $version -or
    $metadata.ProductName -ne 'Direct Payments Timesheets' -or $metadata.CompanyName -ne 'Mark Worsdall') {
    throw 'The release executable does not contain the expected Windows version resources.'
}
# Validate the payload, not the NSIS bootstrap executable (which is x86).
python (Join-Path $root 'packaging/package.py') pe $target $binary
if ($LASTEXITCODE -ne 0) { throw 'Wrong Windows payload architecture or subsystem.' }

$buildDir = Join-Path $root "dist/windows-installer-build-$Architecture"
$artifactDir = if ($OutputDirectory) { [System.IO.Path]::GetFullPath($OutputDirectory) } else { Join-Path $root 'dist/windows-test' }
New-Item -ItemType Directory -Path $buildDir, $artifactDir -Force | Out-Null
$output = Join-Path $artifactDir "DirectPaymentTimesheets-$version-windows-$Architecture-setup.exe"

# Generate exact install/uninstall lists from the checked-in notice files.
# The uninstaller removes these files individually and only removes empty directories.
$noticeRoot = Join-Path $root 'third-party'
$notices = @(Get-ChildItem -LiteralPath $noticeRoot -File -Recurse | Sort-Object FullName)
$install = [System.Collections.Generic.List[string]]::new()
$uninstall = [System.Collections.Generic.List[string]]::new()
$directories = [System.Collections.Generic.HashSet[string]]::new()
foreach ($notice in $notices) {
    $relative = $notice.FullName.Substring($noticeRoot.Length + 1)
    $parent = [System.IO.Path]::GetDirectoryName($relative)
    $escapedRelative = $relative.Replace('$', '$$')
    $escapedParent = $parent.Replace('$', '$$')
    $install.Add('SetOutPath "$INSTDIR\third-party\' + $escapedParent + '"')
    $install.Add('File "' + $notice.FullName.Replace('$', '$$') + '"')
    $uninstall.Add('Delete "$INSTDIR\third-party\' + $escapedRelative + '"')
    while ($parent) {
        [void]$directories.Add($parent)
        $parent = [System.IO.Path]::GetDirectoryName($parent)
    }
}
foreach ($directory in ($directories | Sort-Object Length -Descending)) {
    $uninstall.Add('RMDir "$INSTDIR\third-party\' + $directory.Replace('$', '$$') + '"')
}
$uninstall.Add('RMDir "$INSTDIR\third-party"')
$utf8 = [System.Text.UTF8Encoding]::new($false)
[System.IO.File]::WriteAllLines((Join-Path $buildDir 'third-party-install.nsh'), $install, $utf8)
[System.IO.File]::WriteAllLines((Join-Path $buildDir 'third-party-uninstall.nsh'), $uninstall, $utf8)

& $compiler /NOCONFIG /INPUTCHARSET UTF8 "/DARCHITECTURE=$Architecture" "/DSOURCE_BINARY=$binary" "/DPROJECT_ROOT=$root" "/DBUILD_DIR=$buildDir" "/DVERSION=$version" "/DOUTPUT_FILE=$output" (Join-Path $PSScriptRoot 'installer.nsi')
if ($LASTEXITCODE -ne 0) { throw "NSIS failed with exit code $LASTEXITCODE" }
if (-not (Test-Path -LiteralPath $output -PathType Leaf)) { throw 'NSIS did not produce the installer.' }

# Keep licence notices beside the installer candidate as well as inside it.
# The validated executable is distributed only inside the installer.
Copy-Item -LiteralPath (Join-Path $root 'LICENSE'), (Join-Path $root 'THIRD_PARTY_LICENSES.md'), (Join-Path $root 'assets/fonts/DejaVu-LICENSE.txt') -Destination $artifactDir
foreach ($notice in $notices) {
    $relative = $notice.FullName.Substring($noticeRoot.Length + 1)
    $destination = Join-Path (Join-Path $artifactDir 'third-party') $relative
    New-Item -ItemType Directory -Path ([System.IO.Path]::GetDirectoryName($destination)) -Force | Out-Null
    Copy-Item -LiteralPath $notice.FullName -Destination $destination
}
Write-Output $output

python (Join-Path $root 'packaging/package.py') record $output $target windows
if ($LASTEXITCODE -ne 0) { throw 'Could not record installer provenance.' }
