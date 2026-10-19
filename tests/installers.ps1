# Network-free functional tests. Marker bytes stand in for a release executable;
# the installer must only copy them, never execute them. No Pester dependency.
$ErrorActionPreference = 'Stop'
$Installer = Join-Path $PSScriptRoot '../install.ps1'
$Root = Join-Path ([System.IO.Path]::GetTempPath()) ('uncle-installer-test-' + [guid]::NewGuid().ToString())
$Fixture = Join-Path $Root 'fixture'
$InstallDir = Join-Path $Root 'bin'
$Destination = Join-Path $InstallDir 'uncle-skills.exe'
$Asset = 'uncle-skills-x86_64-pc-windows-msvc.zip'
$Marker = [System.Text.Encoding]::UTF8.GetBytes('installer-fixture-not-an-executable')
New-Item -ItemType Directory -Path $Fixture -Force | Out-Null
$global:UncleInstallerFixtureRoot = $Fixture
$global:UncleInstallerMockCalls = 0

# This shadows the network cmdlet for the child installer script.
function Invoke-WebRequest {
    param([string]$Uri, [string]$OutFile)
    $global:UncleInstallerMockCalls += 1
    $Name = [System.IO.Path]::GetFileName(([uri]$Uri).AbsolutePath)
    Copy-Item (Join-Path $global:UncleInstallerFixtureRoot $Name) $OutFile
}

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Expect-Failure {
    param([scriptblock]$Action, [string]$ExpectedMessage)
    $Caught = $null
    try { & $Action } catch { $Caught = $_ }
    if ($null -eq $Caught) { throw "Expected failure: $ExpectedMessage" }
    if ($Caught.Exception.Message -notlike "*$ExpectedMessage*") {
        throw "Wrong failure: $($Caught.Exception.Message); expected $ExpectedMessage"
    }
}

try {
    [System.IO.File]::WriteAllBytes((Join-Path $Fixture 'uncle-skills.exe'), $Marker)
    Compress-Archive (Join-Path $Fixture 'uncle-skills.exe') -DestinationPath (Join-Path $Fixture $Asset)
    $Hash = (Get-FileHash (Join-Path $Fixture $Asset) -Algorithm SHA256).Hash.ToLowerInvariant()
    "$Hash  $Asset" | Set-Content (Join-Path $Fixture "$Asset.sha256") -Encoding ascii
    $ExpectedBinaryHash = (Get-FileHash (Join-Path $Fixture 'uncle-skills.exe')).Hash

    & $Installer -InstallDir $InstallDir
    Assert-True (Test-Path $Destination -PathType Leaf) 'Fresh install did not create the binary'
    Assert-True ((Get-FileHash $Destination).Hash -eq $ExpectedBinaryHash) 'Fresh install changed marker bytes'
    Write-Host 'PASS: fresh install'

    [System.IO.File]::WriteAllText($Destination, 'unrelated existing command')
    $OriginalHash = (Get-FileHash $Destination).Hash
    $CallsBeforeRefusal = $global:UncleInstallerMockCalls
    Expect-Failure { & $Installer -InstallDir $InstallDir } '-Replace'
    Assert-True ((Get-FileHash $Destination).Hash -eq $OriginalHash) 'Existing command was overwritten without consent'
    Assert-True ($global:UncleInstallerMockCalls -eq $CallsBeforeRefusal) 'Existing-command refusal unexpectedly downloaded assets'
    Write-Host 'PASS: existing command refused and preserved'

    & $Installer -InstallDir $InstallDir -Replace
    Assert-True ((Get-FileHash $Destination).Hash -eq $ExpectedBinaryHash) 'Explicit replacement did not install marker bytes'
    Write-Host 'PASS: explicit replacement'

    [System.IO.File]::WriteAllText($Destination, 'preserve on bad checksum')
    $OriginalHash = (Get-FileHash $Destination).Hash
    (('0' * 64) + "  $Asset") | Set-Content (Join-Path $Fixture "$Asset.sha256") -Encoding ascii
    Expect-Failure { & $Installer -InstallDir $InstallDir -Replace } 'Checksum mismatch'
    Assert-True ((Get-FileHash $Destination).Hash -eq $OriginalHash) 'Bad checksum replaced existing command'
    Write-Host 'PASS: corrupt checksum preserves existing command'

    Remove-Item $Destination
    New-Item -ItemType Directory -Path $Destination | Out-Null
    $Sentinel = Join-Path $Destination 'keep.txt'
    [System.IO.File]::WriteAllText($Sentinel, 'preserve directory')
    Expect-Failure { & $Installer -InstallDir $InstallDir -Replace } 'Destination is a directory'
    Assert-True (Test-Path $Destination -PathType Container) 'Directory destination was removed'
    Assert-True ([System.IO.File]::ReadAllText($Sentinel) -eq 'preserve directory') 'Directory content was changed'
    Write-Host 'PASS: directory protection'
    Write-Host 'All five network-free Windows installer tests passed.'
} finally {
    Remove-Item $Root -Recurse -Force
    Remove-Variable UncleInstallerFixtureRoot, UncleInstallerMockCalls -Scope Global -ErrorAction SilentlyContinue
}
