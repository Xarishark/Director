# Install Director

## Requirements

- **Windows 11, version 24H2 (build 26100) or newer**, x64. HDR switching relies on display requests added in 24H2, so setup refuses older versions and points you to Windows Update.
- The **Microsoft Windows App Runtime 2.6**. Setup installs it for you when it's missing.

## Install

1. Download **Director-x64-setup.exe** from the [latest release](https://github.com/Xarishark/Director/releases/latest).
2. Run it and choose **Install**. Director installs for everyone on the PC, so Windows asks for administrator approval once. Leave **Create a desktop shortcut** ticked if you want one.
3. Choose **Launch**, or open **Director** from Start.

Setup and the app aren't code-signed yet, so Windows SmartScreen may warn before setup runs. Choose **More info → Run anyway** if you trust the download.

What setup does:

- Installs Director to `%ProgramFiles%\Director` with its license files, and adds Start menu and (optionally) desktop shortcuts.
- Adds the install folder to the system PATH, so `director` works in any **new** terminal.
- Registers the `director-run:` link that CLI failure notifications use to open their result.
- If the Windows App Runtime 2.6 is missing, downloads Microsoft's signed installer (about 125 MB) from Microsoft, checks its SHA-512 hash, installs it for all users and deletes the download. An offline PC needs the runtime installed beforehand.

### Quiet install

From an elevated terminal:

```powershell
.\Director-x64-setup.exe /quiet
.\Director-x64-setup.exe /quiet DesktopShortcut=0
```

## Update

Download and run the newer setup. It replaces the installed version and keeps your settings. `director --version` or **Options → About** shows the installed version.

## Uninstall

Uninstall **Director** from **Settings → Apps → Installed apps**. This removes the program, shortcuts and PATH entry. Your scenes, presets, TV pairings and logs stay in `%LOCALAPPDATA%\Director`; delete that folder to remove them too. The Windows App Runtime stays installed, since other apps can use it.

## Build from source

See [Development](DEVELOPMENT.md).
