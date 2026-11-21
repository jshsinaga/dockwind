# Run the release executable against the Windows desktop in a disposable profile.
[CmdletBinding()]
param(
    [string]$Executable = (Join-Path $PSScriptRoot '../target/release/dockwind.exe')
)

$ErrorActionPreference = 'Stop'
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$configDir = Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'Dockwind'
$configPath = Join-Path $configDir 'config.toml'
$logs = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Dockwind/logs'
if (Test-Path -LiteralPath $configPath) {
    throw "The dock smoke test needs a profile without an existing Dockwind configuration."
}

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class DockwindSmoke {
    [DllImport("dwmapi.dll")]
    public static extern int DwmGetWindowAttribute(IntPtr window, int attribute, out int value, int size);
}
'@

function Write-SmokeConfig([string]$Mode) {
    @"
[appearance]
blur = "$Mode"
hover_zoom = true
hover_background = true

[behavior]
auto_hide = false
hide_native_taskbar = false
reserve_space = false
"@ | Set-Content -LiteralPath $configPath -Encoding utf8NoBOM
}

function Read-DockLogs {
    $logFiles = Get-ChildItem -LiteralPath $logs -Filter 'dockwind.log*' -ErrorAction SilentlyContinue
    ($logFiles | ForEach-Object { Get-Content -LiteralPath $_.FullName -Tail 60 }) -join "`n"
}

function Wait-DockState([string]$Description, [scriptblock]$Check) {
    $timeout = [DateTime]::UtcNow.AddSeconds(20)
    do {
        if ($process.HasExited) {
            throw "Dockwind exited with code $($process.ExitCode) while waiting for $Description."
        }
        $result = & $Check
        if ($result) { return $result }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $timeout)
    throw "Timed out waiting for $Description."
}

$null = New-Item -ItemType Directory -Path $configDir -Force
$process = $null
try {
    Write-SmokeConfig 'none'
    $process = Start-Process -FilePath $Executable -PassThru
    $window = Wait-DockState 'the first dock frame' {
        $log = Read-DockLogs
        if ($log -match 'dock window shown after first frame' -and
            $log -match 'dock window styles applied hwnd=0x([0-9A-Fa-f]+)') {
            return [IntPtr][Convert]::ToInt64($Matches[1], 16)
        }
    }

    foreach ($mode in @('none', 'mica', 'acrylic', 'none')) {
        $expected = @{ none = 1; mica = 2; acrylic = 3 }[$mode]
        Write-SmokeConfig $mode
        $null = Wait-DockState "the $mode backdrop on the dock" {
            $actual = 0
            $status = [DockwindSmoke]::DwmGetWindowAttribute($window, 38, [ref]$actual, 4)
            $status -eq 0 -and $actual -eq $expected
        }
        Write-Host "[smoke] Windows reported the $mode backdrop on the running dock"
    }
}
catch {
    Write-Host (Read-DockLogs)
    throw
}
finally {
    if ($process -and !$process.HasExited) {
        Stop-Process -Id $process.Id
        $process.WaitForExit()
    }
    Remove-Item -LiteralPath $configPath -ErrorAction SilentlyContinue
}
