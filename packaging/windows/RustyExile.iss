#define MyAppName "RustyExile"
#ifndef MyAppVersion
  #define MyAppVersion "1.0.8"
#endif
#define MyAppPublisher "atlazlp"
#define MyAppURL "https://github.com/atlazlp/RustyExile"
#define MyAppExeName "RustyExile.exe"

[Setup]
AppId={{B6E8F1A2-4C3D-4F9E-9A71-8D2E5F0A1B3C}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}/releases
DefaultDirName={code:DefaultDir}
DisableProgramGroupPage=yes
LicenseFile=..\..\LICENSE
OutputDir=..\..\dist
OutputBaseFilename=RustyExile-windows-x86_64-setup
SetupLogging=yes
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
SetupIconFile=portal.ico
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\{#MyAppExeName}
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=commandline
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: checkedonce

[Files]
Source: "..\..\dist\stage\RustyExile.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\dist\stage\portal.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\dist\stage\assets\*"; DestDir: "{app}\assets"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; IconFilename: "{app}\portal.ico"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; IconFilename: "{app}\portal.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[Code]
function DefaultDir(Param: String): String;
begin
  if IsAdminInstallMode then
    Result := ExpandConstant('{autopf}\{#MyAppName}')
  else
    Result := ExpandConstant('{localappdata}\{#MyAppName}');
end;

function CommandTail: String;
var
  I: Integer;
  Param: String;
begin
  Result := '';
  for I := 1 to ParamCount do
  begin
    Param := ParamStr(I);
    if (CompareText(Param, '/ALLUSERS') = 0) or (CompareText(Param, '/CURRENTUSER') = 0) then
      Continue;
    Result := Result + ' ' + AddQuotes(Param);
  end;
end;

procedure ExitProcess(uExitCode: UINT);
  external 'ExitProcess@kernel32.dll stdcall';

function InitializeSetup: Boolean;
var
  Code: Integer;
begin
  Result := True;
  if IsAdminInstallMode then
    Exit;
  if not RegKeyExists(HKLM, 'Software\Microsoft\Windows\CurrentVersion\Uninstall\{B6E8F1A2-4C3D-4F9E-9A71-8D2E5F0A1B3C}_is1') then
    Exit;
  if ShellExec('runas', ExpandConstant('{srcexe}'), '/ALLUSERS' + CommandTail, '', SW_SHOWNORMAL, ewNoWait, Code) then
    ExitProcess(0);
  MsgBox('RustyExile is in a protected folder. Allow the Windows permission prompt to update it.', mbInformation, MB_OK);
  Result := False;
end;
