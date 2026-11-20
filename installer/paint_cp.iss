; Script Inno Setup per l'installer di Paint CP.
; Si compila con: ISCC.exe installer\paint_cp.iss  (dopo cargo build --release)

#define AppNome "Paint CP"
#define AppVersione "1.3"
#define AppEditore "Peppe CP"
#define AppExe "paint_cp.exe"

[Setup]
AppId={{B7C4E5A2-3D6F-4E19-9C2A-5F1B8D7A6E30}
AppName={#AppNome}
AppVersion={#AppVersione}
AppVerName={#AppNome} {#AppVersione}
AppPublisher={#AppEditore}
DefaultDirName={autopf}\{#AppNome}
DefaultGroupName={#AppNome}
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppNome}
OutputDir=..\dist
OutputBaseFilename=PaintCP-Setup-{#AppVersione}
SetupIconFile=..\assets\icon.ico
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64
DisableProgramGroupPage=yes

[Languages]
Name: "italiano"; MessagesFile: "compiler:Languages\Italian.isl"

[Tasks]
Name: "desktopicon"; Description: "Crea un'icona sul &desktop"; GroupDescription: "Icone aggiuntive:"

[Files]
Source: "..\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppNome}"; Filename: "{app}\{#AppExe}"
Name: "{group}\Disinstalla {#AppNome}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppNome}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExe}"; Description: "Avvia {#AppNome}"; Flags: nowait postinstall skipifsilent
