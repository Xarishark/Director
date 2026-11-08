# Native Windows interface and runtime

## Application layout

The root Cargo package produces target/release/director.exe. Its WinUI 3 controls are provided through pinned Microsoft windows-reactor 0.100.0. The library crate holds the device engines, Windows adapters, settings storage and CLI. There is one frontend, build script, package lockfile, and executable name. CLI arguments are handled before GUI startup.

The GUI uses native NavigationView, ListView, Button, TextBox, ComboBox, NumberBox, ToggleSwitch, Expander, InfoBar, and ContentDialog controls. Rust background jobs return plain-data messages; Windows/COM objects stay in their creating context. A native close guard protects pending work from normal window closure.

## Deployment

The GUI is framework-dependent. windows-reactor 0.100.0 requires the x64 Microsoft.WindowsAppRuntime.2 framework package, version 2.4 or newer. If it is missing, it shows a download prompt at startup. The CLI does not need the runtime. Setup installs the pinned 2.6.5 runtime when needed (see below). This is not a portable self-contained distribution. The installer places `LICENSE.txt` and `THIRD_PARTY_LICENSES.md` beside the executable.

## Supported Windows

Director supports **Windows 11, version 24H2 (build 26100) or newer**, x64. HDR switching uses the display-configuration requests for advanced colour info and HDR state that Windows added in 24H2, and only current Windows 11 is tested. Most other features (layouts, audio, FPS, TVs, apps) use long-standing APIs, but older Windows isn't supported or tested. setup.exe (Burn `WindowsBuildNumber`) and the MSI (a launch condition on the registry `CurrentBuildNumber`) both refuse older builds with a message pointing to Windows Update. The executable embeds `build/director.manifest`, which declares Windows 10/11 support (so version APIs report the real build), `maxversiontested` 10.0.26100.0 and per-monitor v2 DPI awareness (WinUI sets the same at startup; the manifest makes CLI runs match). Raise `maxversiontested` when testing on a newer Windows release.

Keep director.ico beside director.exe for the title-bar icon; the executable also embeds the same artwork. Build instructions are in [development](DEVELOPMENT.md).

Reactor is a preview API. Published 0.100.0 differs from current upstream examples; keep the version and Cargo.lock pinned and consult the matching source before changing APIs. The backend uses Windows bindings 0.62 while Reactor supplies its own WinRT types. Never exchange those framework objects across the boundary.

## Upstream references

- [Microsoft Reactor](https://github.com/microsoft/windows-rs/blob/master/crates/libs/reactor/readme.md)
- [Runtime staging and deployment](https://github.com/microsoft/windows-rs/blob/master/crates/libs/reactor-setup/readme.md)
## Setup and MSI

Build with WiX 6.0.2 using `./scripts/package-msi.ps1` from the repository root. Pass `-Wix` with the path to the WiX CLI if it is outside PATH. Pass `-UseExistingRelease` to package an existing release build. The script adds the pinned WiX Util and bootstrapper extensions to the global WiX cache if they are missing. It writes two files to `target/installer`:

- `Director-<version>-x64-setup.exe` is the file users run. It is a WiX Burn bundle built from [DirectorSetup.wxs](../installer/DirectorSetup.wxs), using the standard WiX bootstrapper UI with a [Director theme](../installer/theme). It has a welcome page with a desktop shortcut checkbox, a progress bar, and a finish page with **Launch**. Releases publish only this file.
- `Director-<version>-x64.msi` is the application package from [Director.wxs](../installer/Director.wxs). setup.exe embeds and installs it. It has no dialogs of its own, and CI and releases do not publish it separately. Build it locally for managed deployment on PCs that already have the runtime.

Director installs for everyone in `%ProgramFiles%\Director` and asks for administrator approval once. Setup adds Start menu and optional desktop shortcuts, registers the `director-run` link handler, and appends the install folder to the system PATH. Open a new terminal to use `director` from any folder. Director appears once in **Installed apps**. Uninstalling removes the files, shortcuts and PATH entry. Presets, TV pairing data and logs stay in `%LOCALAPPDATA%\Director`.

setup.exe checks the current user's registration for the pinned runtime, `Microsoft.WindowsAppRuntime.2_2.6.5.0_x64__8wekyb3d8bbwe`. If the runtime is missing, setup downloads Microsoft's signed `windowsappruntimeinstall-x64.exe` 2.6.5 (about 125 MB) from the version-pinned aka.ms link and checks its SHA-512 hash. It then runs the installer elevated with `--quiet`, which provisions the runtime for all users, and removes the download afterwards. The runtime stays installed when Director is uninstalled. To move to a newer runtime, update the version, URL, hash and size in DirectorSetup.wxs (`wix burn remotepayload` generates them) along with `$runtimeVersion` and `$runtimePackage` in the packaging script. Offline installation needs the runtime installed beforehand.

Director 0.9.1 and earlier offered a per-user install. Windows Installer cannot upgrade that copy from a machine-wide install, so setup stops with a message asking you to uninstall it first. Your presets are not affected.

For a quiet install, run `Director-<version>-x64-setup.exe /quiet` from an elevated shell. Add `DesktopShortcut=0` to skip the desktop shortcut.

setup.exe and the MSI are unsigned, so SmartScreen may warn before running setup.
