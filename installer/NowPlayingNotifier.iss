; Inno Setup script for NowPlayingNotifier.
; Build (from the repository root, after `cargo build --release`):
;   iscc /DAppVersion=1.2.3 installer\NowPlayingNotifier.iss
; Paths are relative to this file. Output goes to installer-output\.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\target\release"
#endif

#define AppName "NowPlayingNotifier"
#define AppExe "NowPlayingNotifier.exe"
; Former name of the app (before 1.3.0). Its files, shortcuts and autostart entry are removed on upgrade.
#define LegacyName "OtherPlayingNotifier"

[Setup]
; Never change AppId: it identifies the app for upgrades and uninstall.
; (Kept from the OtherPlayingNotifier era, so installing over the old version upgrades it.)
AppId={{7BAD8B7C-37A3-439C-A447-3AD7D44C5B34}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=Shun4211
AppPublisherURL=https://github.com/Shun4211/NowPlayingNotifier
VersionInfoVersion={#AppVersion}
; Per-user install: no admin rights needed (installs to %LOCALAPPDATA%\Programs).
PrivilegesRequired=lowest
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
; Don't reuse the old install folder / Start menu group named after the former name.
UsePreviousAppDir=no
UsePreviousGroup=no
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.19041
SetupIconFile=..\res\appicon.ico
UninstallDisplayIcon={app}\{#AppExe}
UninstallDisplayName={#AppName}
LicenseFile=..\LICENSE
; Warn if the app is running (same mutex as src\system.rs; also the one used under the former name).
AppMutex=Local\NowPlayingNotifier.SingleInstance,Local\{#LegacyName}.SingleInstance
CloseApplications=yes
OutputDir=..\installer-output
OutputBaseFilename={#AppName}-{#AppVersion}-win-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[InstallDelete]
; Leftovers of the version installed under the former name.
Type: filesandordirs; Name: "{userpf}\{#LegacyName}"
Type: filesandordirs; Name: "{userprograms}\{#LegacyName}"
Type: files; Name: "{userdesktop}\{#LegacyName}.lnk"

[Files]
Source: "{#SourceDir}\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{group}\{cm:UninstallProgram,{#AppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Registry]
; The app itself writes this value when "Start with Windows" is enabled.
; ValueType none = never create it here, only remove it on uninstall.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "{#AppName}"; ValueType: none; Flags: uninsdeletevalue dontcreatekey
; The autostart entry written under the former name points to the old exe.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "{#LegacyName}"; ValueType: none; Flags: deletevalue dontcreatekey

[Run]
Filename: "{app}\{#AppExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
; In-app update (src\update.rs) runs setup with /SILENT /RELAUNCH=1: start the app again afterwards.
Filename: "{app}\{#AppExe}"; Flags: nowait; Check: ShouldRelaunch

[Code]
function ShouldRelaunch: Boolean;
begin
  Result := WizardSilent and (ExpandConstant('{param:RELAUNCH|0}') = '1');
end;
