# Native Windows interface and runtime

Status: WinUI 3 is the selected, sole Director frontend as of 2026-10-31.

## Application layout

The root Cargo package produces target/release/director.exe. Its WinUI 3 controls are provided through pinned Microsoft windows-reactor 0.100.0. The library modules implement the shared audio engine, Windows adapter, preset persistence, and CLI. There is one frontend, build script, package lockfile, and executable name. CLI arguments are handled before GUI startup.

The GUI uses native NavigationView, ListView, Button, TextBox, ComboBox, NumberBox, ToggleSwitch, Expander, InfoBar, and ContentDialog controls. Rust background jobs return plain-data messages; Windows/COM objects stay in their creating context. A native close guard protects pending work from normal window closure.

## Deployment

The current GUI is framework-dependent and uses the installed Windows App Runtime. Local validation used runtime 2.6.4, already present on the host; no runtime, Visual Studio, or .NET installation was performed. This is not a portable self-contained distribution. Release packaging, supported Windows baseline, runtime detection/setup, licensing notices, and validation on a clean PC are still required.

Keep director.ico beside director.exe for the title-bar icon; the executable also embeds the same artwork. The requested local Director desktop shortcut points to this main executable. Build instructions are in [development](DEVELOPMENT.md).

Reactor is a preview API. Published 0.100.0 differs from current upstream examples; keep the version and Cargo.lock pinned and consult the matching source before changing APIs. The backend uses Windows bindings 0.62 while Reactor supplies its own WinRT types. Never exchange those framework objects across the boundary.

## Initial observations

The native release comparison, before promotion into the main package, used about 106 MiB working set and 143.5 MiB private committed memory after settling, with a 3.31 MiB executable excluding the shared runtime. Idle CPU time was zero in a two-second sample. These warm local observations are not benchmarks, installed-size totals, startup guarantees, or budgets for later versions. Detailed machine-specific results remain under ignored .localonly.

The user preferred the native interactions and authorized removing the alternate frontend. Keep optimizing measured costs while preserving native behavior.

## Upstream references

- [Microsoft Reactor](https://github.com/microsoft/windows-rs/blob/master/crates/libs/reactor/readme.md)
- [Runtime staging and deployment](https://github.com/microsoft/windows-rs/blob/master/crates/libs/reactor-setup/readme.md)
## MSI preview

Build the x64 MSI with WiX 6.0.2 using `./scripts/package-msi.ps1` from the repository root. Pass `-Wix` with the path to the WiX CLI if it is outside PATH. Pass `-UseExistingRelease` to package an existing release build. The output is `target/installer/Director-<version>-x64.msi`.

**Just for me** is the default and uses `%LOCALAPPDATA%\Programs\Director`. **Everyone on this PC** uses `%ProgramFiles%\Director` and requests elevation as needed. Both add a Start menu shortcut and append the install directory to PATH for the selected scope. Open a new terminal to use `director` from any folder. Uninstall removes the PATH entry. Director runs in the user's session and stores presets, TV pairing data and logs in `%LOCALAPPDATA%\Director`. The installer owns the application files and shortcut. Upgrades apply within the same installation scope. To change scope, uninstall the existing installation and then install again with the other scope.

For a quiet install, use `msiexec /i Director-0.9.1-x64.msi /qn` for the default per-user scope, or add `ALLUSERS=1` from an elevated shell for a machine install.

Install the compatible x64 Windows App Runtime separately before opening the window on a clean PC. The preview MSI is unsigned. The build and administrative extraction passed, including byte checks for the executable and all seven assets. Clean-PC installation, standard-account launch, upgrade and uninstall tests remain open.
