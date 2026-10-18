; Inno Setup script of the Windows installer. build-installer.ps1 sets the environment and runs
; it; see that script for the variables.

#define AppName "Razer Battery Report"
#define AppPublisher "xzeldon"
#define AppUrl "https://github.com/xzeldon/razer-battery-report"
#define AppExeName "razer-battery-report.exe"
; platform::APP_ID: the toast AUMID and the name of the mutex a running tray holds
#define AppId "xzeldon.RazerBatteryReport"
#define AppVersion GetEnv("RBR_VERSION")
#define SourceExe GetEnv("RBR_EXE")
#define SourceIcon GetEnv("RBR_ICON")
#define OutputDir GetEnv("RBR_DIST")
#define SetupName GetEnv("RBR_SETUP")
#define UpdateWaitMs 15000
#define UpdatePollMs 250
; Where Setup records a per-user installation; the GUID is AppId below
#define UninstallKey "Software\Microsoft\Windows\CurrentVersion\Uninstall\{8EC7685F-05CE-4DE1-B33E-262D8A56D8C3}_is1"

[Setup]
AppId={{8EC7685F-05CE-4DE1-B33E-262D8A56D8C3}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppUrl}
AppSupportURL={#AppUrl}/issues
AppUpdatesURL={#AppUrl}/releases
; Setup and uninstall both refuse to go on while the tray runs.
AppMutex={#AppId}
DefaultDirName={autopf}\{#AppName}
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\{#AppExeName}
UninstallDisplayName={#AppName}
OutputDir={#OutputDir}
OutputBaseFilename={#SetupName}
SetupIconFile={#SourceIcon}
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Everything the app stores is per user, so it installs per user and never asks for admin rights.
PrivilegesRequired=lowest
WizardStyle=modern

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "{#AppExeName}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"; AppUserModelID: "{#AppId}"

[Run]
Filename: "{app}\{#AppExeName}"; Description: "Launch {#AppName}"; Flags: nowait postinstall skipifsilent

[Code]
// The app removes what it stored itself (autostart, notification registration, its files), so
// the uninstaller only runs `cleanup` while the exe is still there. A silent uninstall keeps the
// settings and logs.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Params: String;
  ResultCode: Integer;
begin
  if CurUninstallStep <> usUninstall then
    Exit;
  Params := 'cleanup';
  if not UninstallSilent and
    (MsgBox('Also delete your settings and logs?', mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES) then
    Params := Params + ' --all';
  if not Exec(ExpandConstant('{app}\{#AppExeName}'), Params, '', SW_HIDE, ewWaitUntilTerminated, ResultCode) then
    Log('cannot run cleanup: ' + SysErrorMessage(ResultCode))
  else if ResultCode <> 0 then
    Log('cleanup failed with exit code ' + IntToStr(ResultCode));
end;

// The tray starts the installer with /UPDATE and exits right after. Setup checks AppMutex only
// after InitializeSetup, so it waits here until the tray has gone.
function IsUpdate: Boolean;
var
  I: Integer;
begin
  Result := False;
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), '/UPDATE') = 0 then
      Result := True;
end;

function InitializeSetup: Boolean;
var
  Waited: Integer;
begin
  if IsUpdate then
  begin
    Waited := 0;
    while CheckForMutexes('{#AppId}') and (Waited < {#UpdateWaitMs}) do
    begin
      Sleep({#UpdatePollMs});
      Waited := Waited + {#UpdatePollMs};
    end;
  end;
  Result := True;
end;

// The app is started again whether or not the update worked; it reports the result itself.
// Setup that gave up before choosing the folder has no {app}, so the folder of the existing
// installation comes from the registry then.
procedure DeinitializeSetup;
var
  Dir: String;
  ResultCode: Integer;
begin
  if not IsUpdate then
    Exit;
  try
    Dir := ExpandConstant('{app}');
  except
    if not RegQueryStringValue(HKCU, '{#UninstallKey}', 'InstallLocation', Dir) then
    begin
      Log('cannot start the app: no installation folder');
      Exit;
    end;
  end;
  if not Exec(AddBackslash(Dir) + '{#AppExeName}', '', '', SW_SHOWNORMAL, ewNoWait, ResultCode) then
    Log('cannot start the app: ' + SysErrorMessage(ResultCode));
end;
