; cash's per-user installer: Inno Setup 6, no admin, the same release files the zip
; carries, in Scoop's layout so an upgrade never overwrites a running cash.exe.
;
;   iscc /DAppVersion=1.5.0 /DSourceDir=..\..\dist packaging\inno\cash.iss
;
; SourceDir holds what the release zip holds: cash.exe, README.md, LICENSE, NOTICE,
; licenses\ and THIRD-PARTY-LICENSES.html. The files go to
; %LOCALAPPDATA%\Programs\cash\<version>\; `cash --install-finish` then points the
; `current` junction at that folder, puts it on the user PATH, writes the Windows Terminal
; profile, makes the tool links when asked, writes the starter ~/.bashrc once, and sweeps
; old version folders nothing runs. Every step after the copy is cash's own command, so
; the Scoop channel and this one share one implementation.
;
; Silent: cash-vX.Y.Z-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
;         [/DIR="C:\path"] [/TASKS="links,scoop" | /TASKS=""] [/LOG="file"]

#ifndef AppVersion
  #error Pass /DAppVersion=X.Y.Z
#endif
#ifndef SourceDir
  #define SourceDir "..\..\dist"
#endif

[Setup]
AppId={{B7E3A0C4-5D2F-4F4B-9E8A-7C1D2B3E4F50}
AppName=cash
AppVersion={#AppVersion}
AppVerName=cash {#AppVersion}
AppPublisher=Tom Cool
AppPublisherURL=https://github.com/tomcoolpxl/cash
AppSupportURL=https://github.com/tomcoolpxl/cash/issues
AppUpdatesURL=https://github.com/tomcoolpxl/cash/releases
VersionInfoVersion={#AppVersion}
VersionInfoDescription=cash, a Bash-language shell for Windows with its Unix tools built in
DefaultDirName={localappdata}\Programs\cash
DisableDirPage=auto
DisableProgramGroupPage=yes
DisableReadyPage=no
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
LicenseFile={#SourceDir}\LICENSE
SetupIconFile=..\..\assets\cash.ico
UninstallDisplayIcon={app}\current\cash.exe
UninstallDisplayName=cash
OutputDir=..\..\target\installer
OutputBaseFilename=cash-v{#AppVersion}-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; A running cash.exe is never touched: the files go to a new version folder and the
; junction moves, so nothing has to be closed, and a shell must not be sent Ctrl-C.
CloseApplications=no
RestartApplications=no
ChangesEnvironment=yes
; Upgrades and uninstalls are per user, under HKCU.
UsePreviousAppDir=yes
UsePreviousTasks=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "links"; Description: "Make the tools programs on PATH (ls.exe, sed.exe, awk.exe, ...), for editors, make and scripts outside cash"; GroupDescription: "Tools:"
Name: "scoop"; Description: "Also install Scoop, a package manager for command-line tools (runs Scoop's own installer)"; GroupDescription: "Tools:"; Flags: unchecked

[Files]
Source: "{#SourceDir}\cash.exe"; DestDir: "{app}\{#AppVersion}"; Flags: ignoreversion
Source: "{#SourceDir}\README.md"; DestDir: "{app}\{#AppVersion}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE"; DestDir: "{app}\{#AppVersion}"; Flags: ignoreversion
Source: "{#SourceDir}\NOTICE"; DestDir: "{app}\{#AppVersion}"; Flags: ignoreversion
Source: "{#SourceDir}\THIRD-PARTY-LICENSES.html"; DestDir: "{app}\{#AppVersion}"; Flags: ignoreversion
Source: "{#SourceDir}\licenses\*"; DestDir: "{app}\{#AppVersion}\licenses"; Flags: ignoreversion recursesubdirs createallsubdirs

[Run]
; The junction, PATH, the Terminal profile, the links (task), the starter bashrc, the
; sweep of old versions; with the scoop task, Scoop's installer last. Hidden: it prints
; one line per step, which /LOG keeps.
Filename: "{app}\{#AppVersion}\cash.exe"; Parameters: "--install-finish ""{app}"" --links"; Tasks: links; Flags: runhidden waituntilterminated; StatusMsg: "Finishing the install..."
Filename: "{app}\{#AppVersion}\cash.exe"; Parameters: "--install-finish ""{app}"""; Tasks: not links; Flags: runhidden waituntilterminated; StatusMsg: "Finishing the install..."
Filename: "{app}\{#AppVersion}\cash.exe"; Parameters: "--install-finish ""{app}"" --scoop"; Tasks: scoop; Flags: waituntilterminated; StatusMsg: "Installing Scoop..."
Filename: "{app}\current\cash.exe"; Description: "Open cash"; Flags: postinstall nowait skipifsilent

[UninstallRun]
; Before the files go: the links and their PATH entry, current's PATH entry, the Terminal
; profile, the junction. ~/.bashrc, history and config stay.
Filename: "{app}\current\cash.exe"; Parameters: "--install-remove ""{app}"""; Flags: runhidden waituntilterminated; RunOnceId: "remove"

[UninstallDelete]
Type: filesandordirs; Name: "{app}\bin"
Type: filesandordirs; Name: "{app}"

[Code]
// A cash that Scoop installed is Scoop's to upgrade: refuse, pointing there.
function InitializeSetup(): Boolean;
var
  Scoop: String;
begin
  Result := True;
  Scoop := ExpandConstant('{%USERPROFILE}\scoop\apps\cash\current\cash.exe');
  if FileExists(Scoop) then
  begin
    if not WizardSilent() then
      MsgBox('cash is installed by Scoop on this account.' + #13#10 + #13#10 +
             'Upgrade it with:  scoop update cash' + #13#10 +
             'or remove it first with:  scoop uninstall cash', mbInformation, MB_OK);
    Log('Refused: a Scoop-installed cash at ' + Scoop);
    Result := False;
  end;
end;
