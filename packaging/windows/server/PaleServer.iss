#define MyAppName "Pale Server"
#ifndef MyAppVersion
#define MyAppVersion "0.1.1"
#endif
#define MyAppPublisher "Palephone"
#define MyAppURL "https://drcpbx.com"
#define MyAppExeName "pale-server.exe"

[Setup]
AppId={{A0827E8E-4A95-45D9-9E6D-F8D13B0B7C48}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/#docs
AppUpdatesURL={#MyAppURL}/#downloads
DefaultDirName={autopf}\Pale Server
DefaultGroupName=Pale Server
DisableProgramGroupPage=yes
LicenseFile=..\..\..\LICENSE
OutputDir=..\..\..\dist\windows-server
OutputBaseFilename=PaleServerSetup-{#MyAppVersion}-x64
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=admin
SetupIconFile=..\..\..\src-tauri\icons\icon.ico
WizardSmallImageFile=pale-server-wizard-small.bmp
UninstallDisplayName={#MyAppName}
UninstallDisplayIcon={app}\pale-server.ico
VersionInfoDescription={#MyAppName} Installer
VersionInfoProductName={#MyAppName}
VersionInfoProductVersion={#MyAppVersion}
VersionInfoCompany={#MyAppPublisher}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "configure"; Description: "Run Pale Server configuration after install"; GroupDescription: "Post-install setup:"; Flags: checkedonce

[Files]
Source: "..\..\..\dist\windows-server\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\..\dist\windows-server\PaleServerService.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "Configure-PaleServer.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "Run-PaleServer.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "THIRD_PARTY_NOTICES.txt"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\..\src-tauri\icons\icon.ico"; DestDir: "{app}"; DestName: "pale-server.ico"; Flags: ignoreversion

[Dirs]
Name: "{commonappdata}\Pale Server"
Name: "{commonappdata}\Pale Server\data"
Name: "{commonappdata}\Pale Server\logs"

[Icons]
Name: "{group}\Configure Pale Server"; Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\Configure-PaleServer.ps1"""; WorkingDir: "{app}"; IconFilename: "{app}\pale-server.ico"
Name: "{group}\Start Pale Server"; Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -Command ""Start-Service PaleServer"""; WorkingDir: "{app}"; IconFilename: "{app}\pale-server.ico"
Name: "{group}\Stop Pale Server"; Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -Command ""Stop-Service PaleServer"""; WorkingDir: "{app}"; IconFilename: "{app}\pale-server.ico"
Name: "{group}\Restart Pale Server"; Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -Command ""Restart-Service PaleServer"""; WorkingDir: "{app}"; IconFilename: "{app}\pale-server.ico"
Name: "{group}\Open Pale Server Health Check"; Filename: "http://127.0.0.1:8080/health"; IconFilename: "{app}\pale-server.ico"
Name: "{group}\Uninstall Pale Server"; Filename: "{uninstallexe}"

[Run]
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\Configure-PaleServer.ps1"""; WorkingDir: "{app}"; Description: "Configure Pale Server"; Flags: postinstall skipifsilent; Tasks: configure

[UninstallRun]
Filename: "{app}\PaleServerService.exe"; Parameters: "stop"; Flags: runhidden; RunOnceId: "PaleServerStop"
Filename: "{app}\PaleServerService.exe"; Parameters: "uninstall"; Flags: runhidden; RunOnceId: "PaleServerUninstall"
