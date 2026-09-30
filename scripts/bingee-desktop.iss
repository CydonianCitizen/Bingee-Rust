#ifndef Version
  #error Pass /DVersion from package-windows.ps1
#endif
[Setup]
AppId={{B4E1B0A1-EB4F-4E6E-9077-0E5BBE2CD4AC}
AppName=Bingee Desktop
AppVersion={#Version}
AppPublisher=Bingee Desktop
DefaultDirName={%LOCALAPPDATA}\Programs\Bingee Desktop
DefaultGroupName=Bingee Desktop
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=bingee-desktop-windows-x64-setup
Compression=lzma2
SolidCompression=yes
CloseApplications=yes
RestartApplications=no
UninstallDisplayIcon={app}\bingee-desktop.exe
WizardStyle=modern

[Files]
Source: "..\dist\bingee-desktop-windows-x64\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Bingee Desktop"; Filename: "{app}\bingee-desktop.exe"
Name: "{autodesktop}\Bingee Desktop"; Filename: "{app}\bingee-desktop.exe"; Tasks: desktopicon

[Tasks]
Name: desktopicon; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Run]
Filename: "{app}\bingee-desktop.exe"; Description: "Launch Bingee Desktop"; Flags: nowait postinstall skipifsilent
