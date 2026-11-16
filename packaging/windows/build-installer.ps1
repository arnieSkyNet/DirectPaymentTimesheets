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

$buildDir = Join-Path $root ('dist/windows-installer-build-' + $Architecture + '-' + [guid]::NewGuid().ToString('N'))
$artifactDir = if ($OutputDirectory) { [System.IO.Path]::GetFullPath($OutputDirectory) } else { Join-Path $root 'dist/windows-test' }
$extractor = (Get-Command '7z.exe' -ErrorAction Stop).Source
$output = Join-Path $artifactDir "DirectPaymentTimesheets-$version-windows-$Architecture-setup.exe"
$manifestHash = & python (Join-Path $root 'packaging/package.py') prepare-windows-build $binary $compiler $extractor $buildDir $output $target
if ($LASTEXITCODE -ne 0) { throw 'Windows build input staging failed.' }
& python (Join-Path $root 'packaging/package.py') compile-windows-build (Join-Path $buildDir 'build.json') $manifestHash
if ($LASTEXITCODE -ne 0) { throw 'Installer compilation/payload verification failed.' }
python (Join-Path $root 'packaging/package.py') licenses $artifactDir
if ($LASTEXITCODE -ne 0) { throw 'Installer sidecar notice verification failed.' }
Write-Output $output
