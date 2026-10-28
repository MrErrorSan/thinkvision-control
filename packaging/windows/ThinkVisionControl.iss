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
; The app's single-instance mutex: setup and uninstall ask the user to close a running copy.
AppMutex=ThinkVisionControl.SingleInstance
CloseApplications=yes
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
; runasoriginaluser: setup runs elevated, the app must not.
Filename: "{app}\{#AppExe}"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent runasoriginaluser

[Code]
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  // "Start with Windows" lives in the app's settings; remove its Run entry with the app.
  if CurUninstallStep = usPostUninstall then
  begin
    RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'ThinkVisionControl');
    // The app also registers thinkvision:// for the current user at start; remove that copy too.
    RegDeleteKeyIncludingSubkeys(HKCU, 'Software\Classes\thinkvision');
  end;
end;
