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
