; ThinkVision Control installer (Inno Setup 6.5+). Build with scripts/make-windows-setup.ps1, which publishes the app
; to build/win-x64 and passes the version from Directory.Build.props.
;
; The app uses the shared .NET 10 Desktop Runtime. If it's missing, setup offers to download and install it from
; Microsoft (ticked by default). If the user skips it, the app still doesn't crash: the .NET host shows
; "You must install or update .NET to run this application" with a download button.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#define AppName "ThinkVision Control"
#define AppExe "ThinkVisionControl.exe"
#define RuntimeUrl "https://aka.ms/dotnet/10.0/windowsdesktop-runtime-win-x64.exe"
#define RuntimeFile "windowsdesktop-runtime-win-x64.exe"

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
Name: dotnet; Description: "Download and install the .NET 10 Desktop Runtime from Microsoft (about 60 MB, required)"; GroupDescription: "Required software:"; Check: not IsDesktopRuntimeInstalled
Name: startmenuicon; Description: "Create a Start menu shortcut"; GroupDescription: "Shortcuts:"
Name: desktopicon; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"

[Files]
Source: "..\..\build\win-x64\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs

[InstallDelete]
; Leftovers of the portable 0.0.1 and 0.0.2 exe, which Windows never listed under Apps: the Start menu shortcut it made
; itself (pointing wherever the exe was run from) and the files it unpacked into Temp on first run.
Type: files; Name: "{userprograms}\{#AppName}.lnk"
Type: filesandordirs; Name: "{%TEMP}\.net\ThinkVisionControl"

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: startmenuicon
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
; runasoriginaluser: setup runs elevated, the app must not.
Filename: "{app}\{#AppExe}"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent runasoriginaluser; Check: IsDesktopRuntimeInstalled

[Code]
var
  DownloadPage: TDownloadWizardPage;
  RuntimeWasInstalled: Boolean;

function IsDesktopRuntimeInstalled: Boolean;
var
  FindRec: TFindRec;
begin
  Result := False;
  if FindFirst(ExpandConstant('{commonpf64}\dotnet\shared\Microsoft.WindowsDesktop.App\10.*'), FindRec) then
  try
    repeat
      if FindRec.Attributes and FILE_ATTRIBUTE_DIRECTORY <> 0 then
        Result := True;
    until Result or not FindNext(FindRec);
  finally
    FindClose(FindRec);
  end;
end;

procedure InitializeWizard;
begin
  RuntimeWasInstalled := IsDesktopRuntimeInstalled;
  DownloadPage := CreateDownloadPage(SetupMessage(msgWizardPreparing), SetupMessage(msgPreparingDesc), nil);
  DownloadPage.ShowBaseNameInsteadOfUrl := True;
end;

function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;

  if (CurPageID = wpSelectTasks) and not RuntimeWasInstalled and not WizardIsTaskSelected('dotnet') then
    Result := SuppressibleMsgBox('{#AppName} won''t start until the .NET 10 Desktop Runtime is installed.' + #13#10#13#10 +
      'When you open the app, Windows will offer to download it.' + #13#10#13#10 + 'Continue without it?',
      mbConfirmation, MB_YESNO, IDYES) = IDYES;

  if (CurPageID = wpReady) and WizardIsTaskSelected('dotnet') then
  begin
    DownloadPage.Clear;
    DownloadPage.Add('{#RuntimeUrl}', '{#RuntimeFile}', '');
    DownloadPage.Show;
    try
      try
        DownloadPage.Download;
      except
        if not DownloadPage.AbortedByUser then
          SuppressibleMsgBox('Couldn''t download the .NET 10 Desktop Runtime: ' + GetExceptionMessage + #13#10#13#10 +
            'Check the internet connection and try again, or untick it on the previous page.', mbCriticalError, MB_OK, IDOK);
        Result := False;
      end;
    finally
      DownloadPage.Hide;
    end;
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ResultCode: Integer;
begin
  Result := '';
  if not WizardIsTaskSelected('dotnet') then exit;

  WizardForm.PreparingLabel.Caption := 'Installing the .NET 10 Desktop Runtime. This can take a minute...';
  WizardForm.PreparingLabel.Visible := True;
  if not Exec(ExpandConstant('{tmp}\{#RuntimeFile}'), '/install /quiet /norestart', '', SW_HIDE, ewWaitUntilTerminated, ResultCode) then
    ResultCode := -1;
  // 1638: a newer version is already installed. 3010: installed, restart needed later.
  if ResultCode = 3010 then
    NeedsRestart := True
  else if (ResultCode <> 0) and (ResultCode <> 1638) then
    Result := Format('The .NET 10 Desktop Runtime installer failed (code %d). Run setup again, or install it from https://dotnet.microsoft.com/download/dotnet/10.0', [ResultCode]);
end;

function UpdateReadyMemo(Space, NewLine, MemoUserInfoInfo, MemoDirInfo, MemoTypeInfo, MemoComponentsInfo,
  MemoGroupInfo, MemoTasksInfo: String): String;
begin
  Result := MemoDirInfo;
  if MemoTasksInfo <> '' then
    Result := Result + NewLine + NewLine + MemoTasksInfo;
  if RuntimeWasInstalled then
    Result := Result + NewLine + NewLine + '.NET 10 Desktop Runtime:' + NewLine + Space + 'Already installed, skipped';
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  // "Start with Windows" lives in the app's settings; remove its Run entry with the app.
  if CurUninstallStep = usPostUninstall then
    RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'ThinkVisionControl');
end;
