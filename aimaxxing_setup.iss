; AIMAXXING Windows Setup Script (Inno Setup)
; Generates a professional installer with Lite and Full (offline-ready) versions.

#define MyAppName "AIMAXXING"
#define MyAppVersion "0.3.0"
#define MyAppPublisher "AIMAXXING Team"
#define MyAppURL "https://aimaxxing.com"
#define MyAppExeName "aimaxxing-gw.exe"

[Setup]
AppId={{C6D21822-7717-4B2B-B3A9-F7B4A0E1F203}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={localappdata}\{#MyAppName}
DisableProgramGroupPage=yes
OutputDir=.
OutputBaseFilename=aimaxxing_setup
Compression=lzma/max
SolidCompression=yes
WizardStyle=modern
; Allow the user to choose any drive
AllowUNCPath=yes
DefaultGroupName={#MyAppName}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "chinesesimp"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"

[Types]
Name: "recommended"; Description: "Recommended Version (Managers + Bash included)"
Name: "lite"; Description: "Lite Version (Smallest download, requires internet for managers)"
Name: "custom"; Description: "Custom installation"; Flags: iscustom

[Components]
Name: "main"; Description: "Main Application (AIMAXXING Core)"; Types: lite recommended custom; Flags: fixed
Name: "tools"; Description: "Environment Managers (uv, pixi) & Bash"; Types: recommended custom

[Files]
; Lite Version Files
Source: "target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion; Components: main

; Local Binaries (Bundled in Recommended Version)
Source: "bin\uv.exe"; DestDir: "{app}\data\bin"; Flags: ignoreversion; Components: tools
Source: "bin\pixi.exe"; DestDir: "{app}\data\bin"; Flags: ignoreversion; Components: tools

; Pre-provisioned Bash Environment (Recommended Version)
Source: "data\envs\bash\*"; DestDir: "{app}\data\envs\bash"; Flags: ignoreversion recursesubdirs createallsubdirs; Components: tools

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[Code]
// Logic to write the chosen path to a config file or Registry so the binary knows its BASE_DIR
procedure CurStepChanged(CurStep: TSetupStep);
var
  ConfigContent: String;
begin
  if CurStep = ssPostInstall then
  begin
    // Create a local .portable marker (or config file) so the app knows to use the install dir
    ConfigContent := 'portable = true' + #13#10 + 'data_dir = "' + ExpandConstant('{app}') + '"';
    SaveStringToFile(ExpandConstant('{app}\aimaxxing.toml'), ConfigContent, False);
  end;
end;
