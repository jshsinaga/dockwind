#requires -Version 5.1
<#
.SYNOPSIS
Updates the package version in Cargo.toml and refreshes Cargo.lock.

.DESCRIPTION
Use a MAJOR.MINOR.PATCH version. The release workflow requires the pushed tag
(vX.Y.Z) to match this value. Commit the two files, tag, and push the tag.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Version
)

$ErrorActionPreference = 'Stop'

if ($Version -notmatch '^\d+\.\d+\.\d+$') {
    throw "Version '$Version' must be MAJOR.MINOR.PATCH"
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptRoot
Set-Location -LiteralPath $repoRoot

$contents = Get-Content -Raw Cargo.toml
$updated = [regex]::Replace($contents, '(?m)^version = "[^"]+"', "version = `"$Version`"", 1)
if ($updated -eq $contents) {
    throw "Cargo.toml does not contain a package version line"
}
Set-Content -LiteralPath Cargo.toml -Value $updated -NoNewline

cargo check
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

Write-Host "Version is now $Version. Commit Cargo.toml and Cargo.lock, then run:"
Write-Host "  git tag v$Version"
Write-Host "  git push origin v$Version"
