; SmartBlink — Inno Setup 6 Installer Script (M10, ARCHITECTURE.md §4)
; Build command: iscc installer/SmartBlink.iss

#define MyAppName "SmartBlink"
#ifndef MyAppVersion
  #define MyAppVersion "0.1.0-beta"
#endif
#define MyAppPublisher "VKV Studio"
#define MyAppURL "https://vkvstudio.com"
#define MyAppExeName "smartblink.exe"
#ifndef MyAppBinPath
  #define MyAppBinPath "..\target\release\smartblink.exe"
#endif

[Setup]
AppId={{8D6E4A21-7C91-4E3B-9A42-5E10B8C7D901}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
AppCopyright=Copyright (c) 2026 VKV Studio
LicenseFile=..\LICENSE
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
MinVersion=10.0.17763
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\target\installer
OutputBaseFilename=SmartBlink-Setup-{#MyAppVersion}
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
UninstallDisplayName={#MyAppName}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"

[Tasks]
Name: "autostart"; Description: "Run {#MyAppName} at Windows startup (recommended, per-user HKCU)"; GroupDescription: "Startup options:"; Flags: unchecked
Name: "elevatedtask"; Description: "Register elevated logon scheduled task (work above Administrator windows without UAC prompt)"; GroupDescription: "Startup options:"; Flags: unchecked

[Files]
Source: "{#MyAppBinPath}"; DestDir: "{app}"; DestName: "{#MyAppExeName}"; Flags: ignoreversion
Source: "..\assets\icon.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\assets\profiles\LICENSES.md"; DestDir: "{app}\licenses"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\{#MyAppExeName}"

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "{#MyAppName}"; ValueData: """{app}\{#MyAppExeName}"""; Flags: uninsdeletevalue; Tasks: autostart

[Run]
Filename: "schtasks.exe"; Parameters: "/Create /TN ""{#MyAppName}"" /TR ""\""{app}\{#MyAppExeName}\"""" /SC ONLOGON /RL HIGHEST /F"; Flags: runhidden waituntilterminated; Tasks: elevatedtask
Filename: "{app}\{#MyAppExeName}"; Description: "Launch {#MyAppName}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "schtasks.exe"; Parameters: "/Delete /TN ""{#MyAppName}"" /F"; Flags: runhidden waituntilterminated; RunOnceId: "DeleteSmartBlinkTask"

[Code]
const
  WM_CLOSE = $0010;

procedure StopRunningSmartBlink();
var
  Wnd: HWND;
  ResultCode: Integer;
begin
  Wnd := FindWindowByClassName('SmartBlinkTrayWnd');
  if Wnd <> 0 then
  begin
    PostMessage(Wnd, WM_CLOSE, 0, 0);
    Sleep(400);
  end;
  Wnd := FindWindowByClassName('SmartBlinkTrayWnd');
  if Wnd <> 0 then
  begin
    Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM {#MyAppExeName}', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopRunningSmartBlink();
  Result := '';
end;

function InitializeUninstall(): Boolean;
begin
  StopRunningSmartBlink();
  Result := True;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  ConfigDir: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    RegDeleteValue(HKEY_CURRENT_USER, 'Software\Microsoft\Windows\CurrentVersion\Run', '{#MyAppName}');
    ConfigDir := ExpandConstant('{userappdata}\{#MyAppName}');
    if DirExists(ConfigDir) then
    begin
      if not UninstallSilent then
      begin
        if MsgBox('Do you want to remove your SmartBlink settings in ' + ConfigDir + '?', mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        begin
          DelTree(ConfigDir, True, True, True);
        end;
      end;
    end;
  end;
end;
