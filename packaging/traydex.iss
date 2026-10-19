; Built by .github/workflows/release.yml:
; iscc /DVersion=0.1.0-alpha /DArch=x64 /DSource=<folder with Traydex.exe> /DOutput=<folder> packaging/traydex.iss

#ifndef Version
  #define Version "0.0.0"
#endif
#ifndef Arch
  #define Arch "x64"
#endif
#ifndef Source
  #define Source "..\target\release"
#endif
#ifndef Output
  #define Output "..\dist"
#endif

#if Arch == "x64"
  #define Allowed "x64compatible"
#elif Arch == "arm64"
  #define Allowed "arm64"
#else
  #define Allowed "x86compatible"
#endif

[Setup]
AppId={{7B0E5C2A-4F3D-4E8B-9A61-2D7C0F1E5B93}
AppName=Traydex
AppVersion={#Version}
AppVerName=Traydex {#Version}
AppPublisher=Traydex contributors
AppPublisherURL=https://github.com/ipavlin98/traydex
AppSupportURL=https://github.com/ipavlin98/traydex/issues
AppUpdatesURL=https://github.com/ipavlin98/traydex/releases
DefaultDirName={localappdata}\Programs\Traydex
DefaultGroupName=Traydex
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed={#Allowed}
#if Arch != "x86"
ArchitecturesInstallIn64BitMode={#Allowed}
#endif
MinVersion=10.0.17134
LicenseFile=..\LICENSE
OutputDir={#Output}
OutputBaseFilename=Traydex-{#Version}-{#Arch}-setup
SetupIconFile=..\assets\traydex.ico
UninstallDisplayIcon={app}\Traydex.exe
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "ru"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "es"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"

[CustomMessages]
en.RemoveData=Also delete your shelf, notes and settings?
ru.RemoveData=Удалить также содержимое полки, заметки и настройки?
es.RemoveData=¿Eliminar también el estante, las notas y la configuración?
fr.RemoveData=Supprimer aussi l'étagère, les notes et les paramètres ?

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Source}\Traydex.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\THIRD-PARTY-NOTICES.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Traydex"; Filename: "{app}\Traydex.exe"
Name: "{autodesktop}\Traydex"; Filename: "{app}\Traydex.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "Traydex"; Flags: dontcreatekey uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\*\shell\Traydex"; Flags: dontcreatekey uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\Directory\shell\Traydex"; Flags: dontcreatekey uninsdeletekey

[Run]
Filename: "{app}\Traydex.exe"; Description: "{cm:LaunchProgram,Traydex}"; Flags: nowait postinstall skipifsilent
Filename: "{app}\Traydex.exe"; Flags: nowait; Check: Relaunch

[Code]
procedure StopTraydex();
var
  Code: Integer;
begin
  Exec('powershell.exe',
    '-NoProfile -Command "Get-Process Traydex -ErrorAction SilentlyContinue | Where-Object Path -eq ''' +
    ExpandConstant('{app}\Traydex.exe') + ''' | Stop-Process -Force"',
    '', SW_HIDE, ewWaitUntilTerminated, Code);
end;

function Relaunch(): Boolean;
begin
  Result := WizardSilent and (ExpandConstant('{param:relaunch|0}') = '1');
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopTraydex();
  Result := '';
end;

function InitializeUninstall(): Boolean;
begin
  StopTraydex();
  Result := True;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if (CurUninstallStep = usPostUninstall) and not UninstallSilent and
    (MsgBox(CustomMessage('RemoveData'), mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES) then
    DelTree(ExpandConstant('{localappdata}\Traydex'), True, True, True);
end;
