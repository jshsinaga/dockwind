# Run the release executable against the Windows desktop in a disposable profile.
[CmdletBinding()]
param(
    [string]$Executable = (Join-Path $PSScriptRoot '../target/release/dockwind.exe'),
    [string]$Screenshots = (Join-Path $PSScriptRoot '../target/dock-smoke'),
    [switch]$BackdropOnly
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

    [StructLayout(LayoutKind.Sequential)]
    private struct Point { public int X, Y; }

    [StructLayout(LayoutKind.Sequential)]
    private struct GuiThreadInfo {
        public int Size, Flags;
        public IntPtr Active, Focus, Capture, MenuOwner, MoveSize, Caret;
        public Rect CaretRect;
    }

    [DllImport("dwmapi.dll")]
    public static extern int DwmGetWindowAttribute(IntPtr window, int attribute, out int value, int size);

    [DllImport("dwmapi.dll")]
    public static extern int DwmFlush();

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern IntPtr FindWindow(string className, string windowName);

    public static IntPtr FindByTitle(string title) => FindWindow(null, title);
    public static IntPtr FindByClass(string name) => FindWindow(name, null);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr window, out Rect rect);

    [DllImport("user32.dll")]
    private static extern bool GetClientRect(IntPtr window, out Rect rect);

    [DllImport("user32.dll")]
    private static extern bool ClientToScreen(IntPtr window, ref Point point);

    public static bool IsFrameless(IntPtr window) {
        var origin = new Point();
        return GetWindowRect(window, out var frame) && GetClientRect(window, out var client)
            && ClientToScreen(window, ref origin)
            && frame.Left == origin.X && frame.Top == origin.Y
            && frame.Right - frame.Left == client.Right && frame.Bottom - frame.Top == client.Bottom;
    }

    [DllImport("user32.dll")]
    public static extern bool SetCursorPos(int x, int y);

    [DllImport("user32.dll")]
    private static extern void mouse_event(uint flags, uint x, uint y, uint data, UIntPtr extra);

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr window, IntPtr processId);

    [DllImport("user32.dll", EntryPoint = "GetWindowThreadProcessId")]
    private static extern uint GetWindowAndProcessId(IntPtr window, out uint processId);

    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr window);

    [DllImport("user32.dll")]
    private static extern bool GetGUIThreadInfo(uint thread, ref GuiThreadInfo info);

    [DllImport("user32.dll")]
    private static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);

    public static void Click() {
        mouse_event(2, 0, 0, 0, UIntPtr.Zero);
        mouse_event(4, 0, 0, 0, UIntPtr.Zero);
    }

    public static uint WindowProcess(IntPtr window) {
        GetWindowAndProcessId(window, out var processId);
        return processId;
    }

    public static bool IsDockMenuOpen(IntPtr window) {
        var info = new GuiThreadInfo { Size = Marshal.SizeOf<GuiThreadInfo>() };
        return GetGUIThreadInfo(GetWindowThreadProcessId(window, IntPtr.Zero), ref info)
            && info.MenuOwner == window;
    }

    public static void DismissMenu(IntPtr window) {
        PostMessage(window, 0x001F, IntPtr.Zero, IntPtr.Zero); // WM_CANCELMODE
    }

    public static void CloseWindow(IntPtr window) {
        PostMessage(window, 0x0010, IntPtr.Zero, IntPtr.Zero); // WM_CLOSE
    }
}
'@
Add-Type -AssemblyName System.Drawing
if (!$BackdropOnly) { Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes }

$fixtureTitle = 'Dockwind smoke background'
$fixtureSecondTitle = 'Dockwind smoke second window'

function Start-Fixture {
    Start-Job -ArgumentList $fixtureTitle, $fixtureSecondTitle -ScriptBlock {
        param([string]$Title, [string]$SecondTitle)
        Write-Output $PID
        Add-Type -AssemblyName System.Windows.Forms
        $form = [Windows.Forms.Form]::new()
        $second = [Windows.Forms.Form]::new()
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
        $second.Text = $SecondTitle
        $second.StartPosition = 'Manual'
        $second.Bounds = [Drawing.Rectangle]::new($form.Left + 12, $form.Top + 12, 250, 120)
        $second.Show()
        try {
            [Windows.Forms.Application]::Run($form)
        }
        finally {
            $second.Dispose()
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

function Get-DockRect([IntPtr]$Window) {
    $rect = [DockwindSmoke+Rect]::new()
    if (![DockwindSmoke]::GetWindowRect($Window, [ref]$rect)) {
        throw 'Could not read the dock bounds.'
    }
    return $rect
}

function Save-DockImage([IntPtr]$Window, [string]$Name) {
    $rect = Get-DockRect $Window
    Start-Sleep -Milliseconds 250
    $null = [DockwindSmoke]::DwmFlush()
    $bitmap = [Drawing.Bitmap]::new(($rect.Right - $rect.Left), ($rect.Bottom - $rect.Top))
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
        $path = Join-Path $Screenshots "$Name.png"
        $bitmap.Save($path)
        return $path
    }
    finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

function Read-EdgeContrast([string]$Path) {
    $bitmap = [Drawing.Bitmap]::new($Path)
    try {
        # Measure adjacent pixels across the empty portion to the left of the clock.
        $y = [int]($bitmap.Height / 2)
        $sum = 0.0
        for ($x = 21; $x -le 140; $x++) {
            $sum += [Math]::Abs($bitmap.GetPixel($x, $y).GetBrightness() - $bitmap.GetPixel($x - 1, $y).GetBrightness())
        }
        return ($sum / 120)
    }
    finally {
        $bitmap.Dispose()
    }
}

function Count-IconPixelChanges([string]$Before, [string]$After) {
    $first = [Drawing.Bitmap]::new($Before)
    $second = [Drawing.Bitmap]::new($After)
    try {
        if ($first.Size -ne $second.Size) {
            throw 'Hovering resized the dock window.'
        }
        $centerX = [int]($first.Width / 2)
        $centerY = [int]($first.Height / 2)
        $changes = 0
        for ($y = $centerY - 24; $y -le $centerY + 24; $y++) {
            for ($x = $centerX - 27; $x -le $centerX + 27; $x++) {
                if ($first.GetPixel($x, $y) -ne $second.GetPixel($x, $y)) { $changes++ }
            }
        }
        return $changes
    }
    finally {
        $first.Dispose()
        $second.Dispose()
    }
}

function Test-IconHover([IntPtr]$Window) {
    $app = Join-Path $env:WINDIR 'System32/notepad.exe'
    foreach ($case in @(
        @{ name = 'off'; zoom = 'false'; background = 'false' },
        @{ name = 'zoom'; zoom = 'true'; background = 'false' },
        @{ name = 'background'; zoom = 'false'; background = 'true' },
        @{ name = 'both'; zoom = 'true'; background = 'true' }
    )) {
        $appearanceUpdates = [regex]::Matches((Read-DockLogs), 'dock appearance updated').Count
        @"
[appearance]
width = 560
height = 92
alignment_anchor = "dock"
background = "#172036F0"
hover_zoom = $($case.zoom)
hover_background = $($case.background)

[behavior]
auto_hide = false
hide_native_taskbar = false
show_running_apps = false

[clock]
show = false

[[pinned]]
name = "Notepad"
path = '$app'
"@ | Set-Content -LiteralPath $configPath -Encoding utf8NoBOM
        $null = Wait-DockState "the $($case.name) hover settings" {
            [regex]::Matches((Read-DockLogs), 'dock appearance updated').Count -gt $appearanceUpdates
        }
        $rect = Get-DockRect $Window
        $x = [int](($rect.Left + $rect.Right) / 2)
        $y = [int](($rect.Top + $rect.Bottom) / 2)
        $null = [DockwindSmoke]::SetCursorPos($x, $rect.Top - 24)
        $before = Save-DockImage $Window "hover-$($case.name)-before"
        $null = [DockwindSmoke]::SetCursorPos($x, $y)
        $after = Save-DockImage $Window "hover-$($case.name)-after"
        $changes = Count-IconPixelChanges $before $after
        if (($case.name -eq 'off' -and $changes -gt 10) -or ($case.name -ne 'off' -and $changes -lt 30)) {
            throw "Unexpected icon rendering with hover $($case.name). Pixels changed: $changes."
        }
        Write-Host "[smoke] Hover $($case.name) changed $changes icon pixels at fixed dock bounds"
    }
}

function Open-TrayMenu([IntPtr]$Window) {
    $rect = Get-DockRect $Window
    $inset = [int](($rect.Right - $rect.Left) * 27 / 560)
    $null = [DockwindSmoke]::SetCursorPos(($rect.Right - $inset), [int](($rect.Top + $rect.Bottom) / 2))
    [DockwindSmoke]::Click()
    $null = Wait-DockState 'the Dockwind tray menu' { [DockwindSmoke]::IsDockMenuOpen($Window) }
}

function Test-TrayButton([IntPtr]$Window) {
    Open-TrayMenu $Window
    [DockwindSmoke]::DismissMenu($Window)
    $null = Wait-DockState 'the tray menu to close' { ![DockwindSmoke]::IsDockMenuOpen($Window) }
    Write-Host '[smoke] The Tray button opened and dismissed the native menu'
}

function Select-DockMenuItem([string]$Name) {
    $nameCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::NameProperty, $Name)
    $typeCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty, [Windows.Automation.ControlType]::MenuItem)
    $condition = [Windows.Automation.AndCondition]::new($nameCondition, $typeCondition)
    $item = Wait-DockState "the '$Name' menu item" {
        [Windows.Automation.AutomationElement]::RootElement.FindFirst('Descendants', $condition)
    }
    $rect = $item.Current.BoundingRectangle
    $null = [DockwindSmoke]::SetCursorPos([int]($rect.Left + $rect.Width / 2), [int]($rect.Top + $rect.Height / 2))
    [DockwindSmoke]::Click()
}

function Test-WindowPicker([IntPtr]$Window) {
    $first = [DockwindSmoke]::FindByTitle($fixtureTitle)
    $second = [DockwindSmoke]::FindByTitle($fixtureSecondTitle)
    $fixtureProcess = [DockwindSmoke]::WindowProcess($first)
    if ($fixtureProcess -eq 0 -or $fixtureProcess -ne [DockwindSmoke]::WindowProcess($second)) {
        throw 'The two smoke test windows should belong to the same process.'
    }
    $fixturePath = (Get-Process -Id $fixtureProcess).Path
    $notepadPath = Join-Path $env:WINDIR 'System32/notepad.exe'
    $updates = [regex]::Matches((Read-DockLogs), 'dock appearance updated').Count
    (Get-Content -LiteralPath $configPath -Raw).Replace($notepadPath, $fixturePath) |
        Set-Content -LiteralPath $configPath -Encoding utf8NoBOM
    $null = Wait-DockState 'the pinned window fixture' {
        [regex]::Matches((Read-DockLogs), 'dock appearance updated').Count -gt $updates
    }
    $rect = Get-DockRect $Window
    $null = [DockwindSmoke]::SetCursorPos([int](($rect.Left + $rect.Right) / 2), [int](($rect.Top + $rect.Bottom) / 2))
    $previous = [DockwindSmoke]::GetForegroundWindow()
    $target = if ($previous -eq $second) { $first } else { $second }
    $targetTitle = if ($target -eq $first) { $fixtureTitle } else { $fixtureSecondTitle }
    [DockwindSmoke]::Click()
    $null = Wait-DockState 'the window picker for the grouped apps' { [DockwindSmoke]::IsDockMenuOpen($Window) }
    Select-DockMenuItem $targetTitle
    $null = Wait-DockState 'the selected app window in the foreground' {
        [DockwindSmoke]::GetForegroundWindow() -eq $target
    }
    Write-Host '[smoke] The window picker switched between two windows from the same app'
}

function Test-NativeTrayVisibility([IntPtr]$Window) {
    $taskbar = [DockwindSmoke]::FindByClass('Shell_TrayWnd')
    if ($taskbar -eq [IntPtr]::Zero) {
        Write-Host '[smoke] Explorer taskbar is unavailable on this test desktop'
        return
    }
    $normalConfig = Get-Content -LiteralPath $configPath -Raw
    try {
        $normalConfig.Replace('hide_native_taskbar = false', 'hide_native_taskbar = true') |
            Set-Content -LiteralPath $configPath -Encoding utf8NoBOM
        $null = Wait-DockState 'the native taskbar to hide' { ![DockwindSmoke]::IsWindowVisible($taskbar) }
        foreach ($visible in @($true, $false)) {
            Start-Sleep -Milliseconds 300
            Open-TrayMenu $Window
            $label = if ($visible) { 'Show Windows taskbar and tray' } else { 'Hide Windows taskbar and tray' }
            Select-DockMenuItem $label
            $null = Wait-DockState "native taskbar visibility $visible" {
                [DockwindSmoke]::IsWindowVisible($taskbar) -eq $visible
            }
        }
        Write-Host '[smoke] The dock tray revealed and re-hid the native Windows taskbar'
    }
    finally {
        $normalConfig | Set-Content -LiteralPath $configPath -Encoding utf8NoBOM
        $null = Wait-DockState 'the native taskbar to restore' { [DockwindSmoke]::IsWindowVisible($taskbar) }
    }
}

$null = New-Item -ItemType Directory -Path $configDir -Force
$null = New-Item -ItemType Directory -Path $Screenshots -Force
$process = $null
$fixture = $null
$restoreTaskbar = $false
try {
    Write-SmokeConfig 'none'
    $fixture = Start-Fixture
    $null = Wait-DockState 'the patterned desktop fixture' {
        [DockwindSmoke]::FindByTitle($fixtureTitle) -ne [IntPtr]::Zero
    }
    $process = Start-Process -FilePath $Executable -PassThru
    $window = Wait-DockState 'the first dock frame' {
        $log = Read-DockLogs
        if ($log -match 'dock window shown after first frame' -and
            $log -match 'dock window styles applied hwnd=0x([0-9A-Fa-f]+)') {
            return [IntPtr][Convert]::ToInt64($Matches[1], 16)
        }
    }
    $build = Wait-DockState 'the backdrop backend to start' {
        if ((Read-DockLogs) -match 'dock backdrop applied build=(\d+) mode=None') {
            return [int]$Matches[1]
        }
    }

    $baseline = 0.0
    foreach ($sample in @('none', 'mica', 'acrylic', 'reset')) {
        $mode = if ($sample -eq 'reset') { 'none' } else { $sample }
        $expected = @{ none = 1; mica = 2; acrylic = 3 }[$mode]
        $pattern = "dock backdrop applied build=$build mode=$mode"
        $previous = [regex]::Matches((Read-DockLogs), $pattern, 'IgnoreCase').Count
        Write-SmokeConfig $mode
        $null = Wait-DockState "the $mode backdrop on the dock" {
            if ($build -ge 22621) {
                $actual = 0
                $status = [DockwindSmoke]::DwmGetWindowAttribute($window, 38, [ref]$actual, 4)
                return $status -eq 0 -and $actual -eq $expected
            }
            $count = [regex]::Matches((Read-DockLogs), $pattern, 'IgnoreCase').Count
            return $count -gt $previous -or ($sample -eq 'none' -and $count -gt 0)
        }
        Write-Host "[smoke] The running dock applied $mode on Windows build $build"
        if (![DockwindSmoke]::IsFrameless($window)) {
            throw "The native $mode frame extends beyond the dock client surface."
        }
        $contrast = Read-EdgeContrast (Save-DockImage $window $sample)
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
    if (!$BackdropOnly) {
        Test-IconHover $window
        Test-TrayButton $window
        Test-WindowPicker $window
        $restoreTaskbar = $true
        Test-NativeTrayVisibility $window
        $restoreTaskbar = $false
    }
}
catch {
    Write-Host (Read-DockLogs)
    if ($fixture) { Receive-Job -Job $fixture -Keep -ErrorAction Continue }
    throw
}
finally {
    if ($process -and !$process.HasExited) {
        Stop-Process -Id $process.Id
        $process.WaitForExit()
    }
    if ($restoreTaskbar) {
        Start-Process -FilePath $Executable -ArgumentList '--restore-taskbar' -Wait
    }
    if ($fixture) {
        $fixtureWindow = [DockwindSmoke]::FindByTitle($fixtureTitle)
        if ($fixtureWindow -ne [IntPtr]::Zero) {
            [DockwindSmoke]::CloseWindow($fixtureWindow)
        }
        if (!(Wait-Job -Job $fixture -Timeout 5)) {
            $fixturePid = Receive-Job -Job $fixture -ErrorAction SilentlyContinue |
                Where-Object { $_ -is [int] } | Select-Object -First 1
            if ($fixturePid) { Stop-Process -Id $fixturePid -ErrorAction SilentlyContinue }
        }
        Remove-Job -Job $fixture -Force
    }
    Remove-Item -LiteralPath $configPath -ErrorAction SilentlyContinue
}
