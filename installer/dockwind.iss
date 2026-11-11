#define AppName "Dockwind"
#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#define AppPublisher "Dockwind"
#define AppExeName "dockwind.exe"

[Setup]
AppId={{8B7C5E3A-2F4D-4A6B-9C1D-7E8F0A1B2C3D}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
OutputDir=..\dist
OutputBaseFilename=Dockwind-{#AppVersion}-setup
SetupIconFile=..\assets\dockwind.ico
UninstallDisplayIcon={app}\{#AppExeName}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
CloseApplications=yes
CloseApplicationsFilter=dockwind.exe
RestartApplications=no
MinVersion=10.0

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional icons:"; Flags: unchecked

[Files]
Source: "..\target\release\{#AppExeName}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExeName}"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
; The app is a tray/tool window and does not close reliably through Restart
; Manager. Stop it before restoring the taskbar and removing files.
Filename: "{sys}\taskkill.exe"; Parameters: "/IM dockwind.exe /F"; Flags: runhidden; RunOnceId: "StopDockwind"
Filename: "{app}\{#AppExeName}"; Parameters: "--disable-autostart"; Flags: runhidden; RunOnceId: "DisableDockwindAutostart"
Filename: "{app}\{#AppExeName}"; Parameters: "--restore-taskbar"; Flags: runhidden; RunOnceId: "RestoreDockwindTaskbar"

[UninstallDelete]
Type: filesandordirs; Name: "{app}"
