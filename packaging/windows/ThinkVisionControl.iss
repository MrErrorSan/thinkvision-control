; ThinkVision Control installer (Inno Setup 6.5+). Build with scripts/make-windows-setup.ps1, which builds the app
; with cargo (target\release\thinkvision-control.exe) and passes the version from Cargo.toml.
;
; The app is one self-contained exe: no runtime to install.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#define AppName "ThinkVision Control"
#define AppExe "thinkvision-control.exe"

[Setup]
AppId={{C040F988-B1EE-407E-AAC8-81D5912B5E47}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=ThinkVision Control contributors
AppPublisherURL=https://github.com/MrErrorSan/thinkvision-control
AppSupportURL=https://github.com/MrErrorSan/thinkvision-control/issues
VersionInfoVersion={#AppVersion}
DefaultDirName={autopf}\{#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=admin
; "setup.exe /CURRENTUSER" installs for this user only, without admin (scripted installs and testing).
PrivilegesRequiredOverridesAllowed=commandline
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
WizardStyle=modern dynamic
SetupIconFile=..\..\assets\brand\ico\ThinkVisionControl.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
; Restart Manager closes a running copy, also in a silent install (winget): 0.0.8 and later exit cleanly when asked,
; older ones are ended. No AppMutex: in a silent install its prompt defaults to Cancel and setup gives up.
; [Run] starts the app again if it was running; the uninstaller closes it in [Code].
CloseApplications=force
RestartApplications=no
OutputDir=..\..\build
OutputBaseFilename=ThinkVisionControl-Setup-{#AppVersion}
Compression=lzma2/max
SolidCompression=yes

[Tasks]
Name: startmenuicon; Description: "Create a Start menu shortcut"; GroupDescription: "Shortcuts:"
Name: desktopicon; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"

[Files]
Source: "..\..\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
Source: "..\..\crates\tvc-app\ui\fonts\LICENSE-FluentSystemIcons.txt"; DestDir: "{app}"; Flags: ignoreversion

[InstallDelete]
; Leftovers of the portable 0.0.1 and 0.0.2 exe, which Windows never listed under Apps: the Start menu shortcut it made
; itself (pointing wherever the exe was run from) and the files it unpacked into Temp on first run.
Type: files; Name: "{userprograms}\{#AppName}.lnk"
Type: filesandordirs; Name: "{%TEMP}\.net\ThinkVisionControl"
; The .NET build that 0.0.3 and 0.0.4 installed here.
Type: files; Name: "{app}\ThinkVisionControl.exe"
Type: files; Name: "{app}\ThinkVisionControl.dll"
Type: files; Name: "{app}\ThinkVisionControl.Core.dll"
Type: files; Name: "{app}\ThinkVisionControl.Ddc.Windows.dll"
Type: files; Name: "{app}\ThinkVisionControl.deps.json"
Type: files; Name: "{app}\ThinkVisionControl.runtimeconfig.json"

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: startmenuicon
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Registry]
; thinkvision:// links (thinkvision://preset/night, ...). HKA is HKLM for an all-users install, HKCU with /CURRENTUSER.
Root: HKA; Subkey: "Software\Classes\thinkvision"; ValueType: string; ValueName: ""; ValueData: "URL:{#AppName}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\thinkvision"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\thinkvision\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"",0"
Root: HKA; Subkey: "Software\Classes\thinkvision\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExe}"" ""%1"""

[Run]
; runasoriginaluser: setup runs elevated, the app must not. Running before the upgrade: back in the tray, also silently.
Filename: "{app}\{#AppExe}"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent runasoriginaluser; Check: not AppWasRunning
Filename: "{app}\{#AppExe}"; Parameters: "--startup"; Flags: nowait runasoriginaluser; Check: AppWasRunning

[Code]
const
  // The app's single-instance mutex (platform/instance.rs) and hidden event window (platform/monitor_events.rs).
  AppMutexName = 'ThinkVisionControl.SingleInstance';
  EventsWindowClass = 'ThinkVisionControl.MonitorEvents';
  WM_ENDSESSION = $0016;
  ENDSESSION_CLOSEAPP = 1;

var
  WasRunning: Boolean;

function InitializeSetup(): Boolean;
begin
  WasRunning := CheckForMutexes(AppMutexName);
  Result := True;
end;

function AppWasRunning(): Boolean;
begin
  Result := WasRunning;
end;

// Asks a running copy to exit the way Windows does at sign-out, and waits up to 5 s for it.
procedure CloseRunningApp();
var
  Wnd: HWND;
  I: Integer;
begin
  if not CheckForMutexes(AppMutexName) then
    Exit;
  Wnd := FindWindowByClassName(EventsWindowClass);
  if Wnd <> 0 then
    PostMessage(Wnd, WM_ENDSESSION, 1, ENDSESSION_CLOSEAPP);
  for I := 1 to 50 do
  begin
    if not CheckForMutexes(AppMutexName) then
      Exit;
    Sleep(100);
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  // After the user confirmed, before any file is removed.
  if CurUninstallStep = usUninstall then
    CloseRunningApp();
  // "Start with Windows" lives in the app's settings; remove its Run entry with the app.
  if CurUninstallStep = usPostUninstall then
  begin
    RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'ThinkVisionControl');
    // The app also registers thinkvision:// for the current user at start; remove that copy too.
    RegDeleteKeyIncludingSubkeys(HKCU, 'Software\Classes\thinkvision');
  end;
end;
