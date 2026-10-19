param(
    [string]$Version = 'latest',
    [string]$InstallDir = (Join-Path $HOME '.local/bin'),
    [switch]$Replace
)
$ErrorActionPreference = 'Stop'
$Repo = 'abutheone/uncle-skills-manager'
if ($env:OS -ne 'Windows_NT') { throw 'Use install.sh on Unix systems' }
if ($Version -ne 'latest' -and $Version -notmatch '^v[0-9][a-zA-Z0-9._-]*$') { throw 'Invalid release tag' }
if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') { throw 'Only Windows x64 release binaries are available' }
$Destination = Join-Path $InstallDir 'uncle-skills.exe'
if ($null -ne (Get-Item -LiteralPath $Destination -Force -ErrorAction SilentlyContinue)) {
    if (-not $Replace) { throw "Destination already exists: $Destination. Re-run with -Replace to replace it explicitly." }
    if (Test-Path $Destination -PathType Container) { throw 'Destination is a directory' }
}
$Asset = 'uncle-skills-x86_64-pc-windows-msvc.zip' 
$Base = if ($Version -eq 'latest') { "https://github.com/$Repo/releases/latest/download" } else { "https://github.com/$Repo/releases/download/$Version" }
$Work = Join-Path ([System.IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $Work | Out-Null
try {
    Invoke-WebRequest "$Base/$Asset" -OutFile (Join-Path $Work $Asset)
    Invoke-WebRequest "$Base/$Asset.sha256" -OutFile (Join-Path $Work "$Asset.sha256")
    $Checksum = (Get-Content (Join-Path $Work "$Asset.sha256") -Raw).Trim()
    if ($Checksum -notmatch ('^([0-9a-fA-F]{64})\s+' + [regex]::Escape($Asset) + '$')) { throw 'Invalid checksum file' }
    $Expected = $Matches[1]
    $Actual = (Get-FileHash (Join-Path $Work $Asset) -Algorithm SHA256).Hash
    if ($Actual -ne $Expected) { throw 'Checksum mismatch; nothing installed' }
    Expand-Archive (Join-Path $Work $Asset) -DestinationPath (Join-Path $Work 'unpacked')
    $Binary = Join-Path $Work 'unpacked/uncle-skills.exe'
    if (-not (Test-Path $Binary -PathType Leaf)) { throw 'Binary missing from release archive' }
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    $Destination = Join-Path $InstallDir 'uncle-skills.exe'
    if (Test-Path $Destination -PathType Container) { throw 'Destination is a directory' }
    if (($null -ne (Get-Item -LiteralPath $Destination -Force -ErrorAction SilentlyContinue)) -and -not $Replace) { throw 'Destination appeared during installation; nothing replaced' }
    $Staged = Join-Path $InstallDir ('.uncle-skills.' + [guid]::NewGuid().ToString() + '.exe')
    try {
        Copy-Item $Binary $Staged
        Move-Item $Staged $Destination -Force:$Replace
    } finally {
        if (Test-Path $Staged) { Remove-Item $Staged }
    }
    Write-Host "Installed $Destination"
    Write-Host "Add $InstallDir to PATH if needed. No profile or execution policy was changed."
    Write-Host 'Run: uncle-skills --help'
} finally {
    Remove-Item $Work -Recurse -Force
}
