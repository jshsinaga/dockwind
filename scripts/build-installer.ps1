#requires -Version 5.1
<#
.SYNOPSIS
Builds the release binary and the Inno Setup installer.

.DESCRIPTION
Resolves the repository root from the script path, builds with the locked
dependencies, then runs Inno Setup 6. The version defaults to the package version
in Cargo.toml. The installer is written to dist\Dockwind-<version>-setup.exe.
#>
[CmdletBinding()]
param(
    [string]$Version
)

$ErrorActionPreference = 'Stop'

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptRoot
Set-Location -LiteralPath $repoRoot

if (-not $Version) {
    $match = Select-String -Path Cargo.toml -Pattern '^version = "([^"]+)"'
    if (-not $match) {
        throw "Cargo.toml does not contain a package version"
    }
    $Version = $match.Matches[0].Groups[1].Value
}

if ($Version -notmatch '^\d+\.\d+\.\d+$') {
    throw "Version '$Version' must be MAJOR.MINOR.PATCH"
}

Write-Host "[installer] building release binary"
cargo build --release --locked
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

$iscc = Get-Command iscc.exe -ErrorAction SilentlyContinue
if (-not $iscc) {
    $candidates = @(
        (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'),
        (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'),
        (Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe')
    )
    $iscc = $candidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
}

if (-not $iscc) {
    throw "Inno Setup 6 was not found. Install it with: choco install innosetup -y"
}

$isccPath = if ($iscc -is [string]) { $iscc } else { $iscc.Source }
Write-Host "[installer] compiling with $isccPath"
& $isccPath "/DAppVersion=$Version" 'installer\dockwind.iss'
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

$output = Join-Path $repoRoot "dist\Dockwind-$Version-setup.exe"
if (-not (Test-Path -LiteralPath $output)) {
    throw "Inno Setup finished but $output is missing"
}
Get-Item -LiteralPath $output
