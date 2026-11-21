# Run the release executable against the Windows desktop in a disposable profile.
[CmdletBinding()]
param(
    [string]$Executable = (Join-Path $PSScriptRoot '../target/release/dockwind.exe'),
    [string]$Screenshots = (Join-Path $PSScriptRoot '../target/dock-smoke')
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
    [StructLayout(LayoutKind.Sequential)]
    public struct Rect { public int Left, Top, Right, Bottom; }

    [DllImport("dwmapi.dll")]
    public static extern int DwmGetWindowAttribute(IntPtr window, int attribute, out int value, int size);

    [DllImport("dwmapi.dll")]
    public static extern int DwmFlush();

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr FindWindow(string className, string windowName);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr window, out Rect rect);
}
'@
Add-Type -AssemblyName System.Drawing

$fixtureTitle = 'Dockwind smoke background'

function Start-Fixture {
    Start-Job -ArgumentList $fixtureTitle -ScriptBlock {
        param([string]$Title)
        Add-Type -AssemblyName System.Windows.Forms
        $form = [Windows.Forms.Form]::new()
        $form.Text = $Title
        $form.FormBorderStyle = 'None'
        $form.StartPosition = 'Manual'
        $form.Bounds = [Windows.Forms.Screen]::PrimaryScreen.WorkingArea
        $form.Add_Paint({
            param($sender, $eventArgs)
            $eventArgs.Graphics.Clear([Drawing.Color]::Black)
            for ($x = 0; $x -lt $sender.ClientSize.Width; $x += 12) {
                $eventArgs.Graphics.FillRectangle([Drawing.Brushes]::White, $x, 0, 6, $sender.ClientSize.Height)
            }
        })
        try {
            [Windows.Forms.Application]::Run($form)
        }
        finally {
            $form.Dispose()
        }
    }
}

function Write-SmokeConfig([string]$Mode) {
    @"
[appearance]
blur = "$Mode"
background = "#20406040"
hover_zoom = true
hover_background = true

[behavior]
auto_hide = false
hide_native_taskbar = false
reserve_space = false
show_running_apps = false
"@ | Set-Content -LiteralPath $configPath -Encoding utf8NoBOM
}

function Read-DockLogs {
    $logFiles = Get-ChildItem -LiteralPath $logs -Filter 'dockwind.log*' -ErrorAction SilentlyContinue
    ($logFiles | ForEach-Object { Get-Content -LiteralPath $_.FullName -Tail 60 }) -join "`n"
}

function Wait-DockState([string]$Description, [scriptblock]$Check) {
    $timeout = [DateTime]::UtcNow.AddSeconds(20)
    do {
        if ($process -and $process.HasExited) {
            throw "Dockwind exited with code $($process.ExitCode) while waiting for $Description."
        }
        $result = & $Check
        if ($result) { return $result }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $timeout)
    throw "Timed out waiting for $Description."
}

function Read-DockPixels([IntPtr]$Window, [string]$Name) {
    $rect = [DockwindSmoke+Rect]::new()
    if (![DockwindSmoke]::GetWindowRect($Window, [ref]$rect)) {
        throw 'Could not read the dock bounds.'
    }
    Start-Sleep -Milliseconds 180
    $null = [DockwindSmoke]::DwmFlush()
    $bitmap = [Drawing.Bitmap]::new(($rect.Right - $rect.Left), ($rect.Bottom - $rect.Top))
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
        $bitmap.Save((Join-Path $Screenshots "$Name.png"))
        # Measure adjacent pixels across the empty portion to the left of the clock.
        $y = [int]($bitmap.Height / 2)
        $sum = 0.0
        for ($x = 21; $x -le 140; $x++) {
            $sum += [Math]::Abs($bitmap.GetPixel($x, $y).GetBrightness() - $bitmap.GetPixel($x - 1, $y).GetBrightness())
        }
        return ($sum / 120)
    }
    finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

$null = New-Item -ItemType Directory -Path $configDir -Force
$null = New-Item -ItemType Directory -Path $Screenshots -Force
$process = $null
$fixture = $null
try {
    Write-SmokeConfig 'none'
    $fixture = Start-Fixture
    $null = Wait-DockState 'the patterned desktop fixture' {
        [DockwindSmoke]::FindWindow($null, $fixtureTitle) -ne [IntPtr]::Zero
    }
    $process = Start-Process -FilePath $Executable -PassThru
    $window = Wait-DockState 'the first dock frame' {
        $log = Read-DockLogs
        if ($log -match 'dock window shown after first frame' -and
            $log -match 'dock window styles applied hwnd=0x([0-9A-Fa-f]+)') {
            return [IntPtr][Convert]::ToInt64($Matches[1], 16)
        }
    }

    $baseline = 0.0
    foreach ($sample in @('none', 'mica', 'acrylic', 'reset')) {
        $mode = if ($sample -eq 'reset') { 'none' } else { $sample }
        $expected = @{ none = 1; mica = 2; acrylic = 3 }[$mode]
        Write-SmokeConfig $mode
        $null = Wait-DockState "the $mode backdrop on the dock" {
            $actual = 0
            $status = [DockwindSmoke]::DwmGetWindowAttribute($window, 38, [ref]$actual, 4)
            $status -eq 0 -and $actual -eq $expected
        }
        Write-Host "[smoke] Windows reported the $mode backdrop on the running dock"
        $contrast = Read-DockPixels $window $sample
        Write-Host "[smoke] $sample edge contrast = $contrast"
        switch ($sample) {
            'none' {
                if ($contrast -lt 0.04) { throw 'The plain dock did not show the test pattern.' }
                $baseline = $contrast
            }
            'reset' {
                if ($contrast -lt $baseline * 0.85) { throw 'Turning blur off did not restore the pattern.' }
            }
            default {
                if ($contrast -gt $baseline * 0.75) { throw "The $mode material did not soften the test pattern." }
            }
        }
    }
}
catch {
    Write-Host (Read-DockLogs)
    if ($fixture) { Receive-Job -Job $fixture -ErrorAction Continue }
    throw
}
finally {
    if ($process -and !$process.HasExited) {
        Stop-Process -Id $process.Id
        $process.WaitForExit()
    }
    if ($fixture) { Remove-Job -Job $fixture -Force }
    Remove-Item -LiteralPath $configPath -ErrorAction SilentlyContinue
}
