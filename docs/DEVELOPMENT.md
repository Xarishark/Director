# Development and testing

## Requirements

- Windows 11 24H2 or newer, x64.
- [Rust](https://rustup.rs) 1.95 or newer with the MSVC toolchain (`x86_64-pc-windows-msvc`), plus the Visual Studio C++ Build Tools ("Desktop development with C++"). CI uses exactly this setup.
- The [Windows App Runtime 2.6](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/downloads) to run the window. Installing Director once also installs it.

The `x86_64-pc-windows-gnullvm` target with LLVM-MinGW also works. `.cargo/config.toml` links its CRT statically, so the executable doesn't need the toolchain's `libunwind.dll`.

## Build, run and check

Run these from the repository root:

```powershell
cargo build                     # debug build: target\debug\director.exe
cargo run                       # open the window
cargo run -- list               # run a CLI command
cargo build --locked --release  # optimized build: target\release\director.exe
```

Before opening a pull request, run the same checks as CI:

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked -- --test-threads=1
```

The build script stages `director.ico`, `icons\` and `orientations\` beside the executable. Keep them there if you copy the executable somewhere else. `Cargo.lock` is committed and every CI build is `--locked`.

`scripts/dev.ps1` is an optional wrapper. It finds a project-local Rust and LLVM-MinGW toolchain in `.localonly\tools` (machine-local; keep it out of commits) or falls back to the one on PATH, and changes environment variables only for its own process:

```powershell
.\scripts\dev.ps1 test --locked
.\scripts\dev.ps1 -Command clippy -CargoArguments @('--locked', '--all-targets', '--', '-D', 'warnings')
```

## Command-line usage

`director --help` lists every command. The main ones:

```text
director list
director scene apply tv-gaming
director scene end
director display apply desk-monitors
director audio status
director audio use soundbar-stage-v2
director audio volume +5
director audio volume --mic 80
director audio mute toggle
director tv input living-room-tv HDMI_2
director tv toggle living-room-tv
```

- Items are found by CLI name (the name in lowercase with dashes) or by ID. Add `--json` to list commands for scripts.
- `audio list`, `outputs`, `inputs`, `status` and `check` change nothing. `use`, `volume` and `mute` change one thing, verify it, and put it back when Windows doesn't confirm it. Leaving out the device means the current default.
- Display Apply follows each preset's **Ask to keep display changes** setting. `--yes` keeps verified changes without asking, and `--confirm` asks in the terminal.
- Exit code 0 means success, including a change that was already in place. 2 means an argument, access or device error.

The executable uses the Windows GUI subsystem. To capture CLI output from PowerShell reliably, use `Start-Process director -ArgumentList … -Wait -NoNewWindow -RedirectStandardOutput out.txt`.

## Settings and storage

Settings live in `%LOCALAPPDATA%\Director`, resolved through the Windows known-folder API, with no fallback folder. `storage::write_document` validates a document, checks under the operation lock that the file on disk still matches what was read, writes a temporary file and replaces the original with `MoveFileExW` (replace existing, write through). If the file changed since it was read, the save is refused and the window reloads. Each file carries a `version`; a corrupt file or one with an unknown version is left untouched and reported, never overwritten. When a format changes incompatibly, raise its version and decide whether to convert or refuse older files.

Back up that folder before running the app against your real settings for testing. Tests never use it.

## Tests

- **Unit tests** use fake backends for audio, displays, FPS and scenes, and cover no-op detection, partial failure and recovery, stale writes, and corrupt or unknown-version files.
- **TV transport tests** run against loopback WebSocket and TLS servers in the test process, plus DPAPI tampering checks. They never contact a real TV, so they don't prove compatibility with a particular model.
- **Wake-on-LAN tests** send magic packets only to a loopback receiver.
- **Ignored tests** (`cargo test -- --ignored <name>`) open preview fixtures with in-memory data for visual checks: `gui::preset_cards_preview::cards_fixture`, `gui::fps_ui::fps_fixture`, `gui::tv_ui::native_preview::guide_fixture` and `gui::tv_controls_ui::native_preview::controls_fixture`. They save nothing and contact no device. Some write their state to `target\preview\` for inspection. Production builds have no fixture entry point.
- `tv::windows_samsung::samsung_tls_fixture` needs a hand-started local TLS server and is meant for manual runs only.

Real hardware checks (applying layouts, switching audio, waking a TV) can't run in CI. Run them by hand, and keep them separate from what automated tests prove.

## WinUI rules worth knowing

- **No tooltips inside a ContentDialog.** The pinned Reactor rejects them with `StructureUnsupported` and closes the window. Give controls in dialogs an automation name instead. `gui::scene_editor_ui::tests::dialog_content_has_no_tooltips` scans the step dialogs, the FPS controls, the relink button, the crash dialog, the command dialog and a checked list of shared helpers; extend it when a dialog hosts something new. Dialogs use `layout_preview_in_dialog` for the same reason.
- WinUI controls fire change events when re-rendered with a new value (sliders, combo boxes). Guard handlers so a re-render isn't treated as an edit.
- UI automation: query an open ComboBox's own descendants with a name/type condition rather than walking the whole window; an unrestricted `FindAll` against an open dropdown can stall the provider.
- For screenshots, bring the window to the front and let a frame render first. An occluded window can capture blank.

## Crash reports

`director::crash::install` sets a panic hook (after `diagnostics::init`), and a fatal Reactor error from `App::run_component` is recorded too. Each report is a text file in `Logs\Crashes` (`crash-<unix ms>.txt`, newest 10 kept) with the summary, version, Windows build, message, a backtrace for panics and the newest 40 log records. On the next start the newest unseen report is offered once; dismissing it renames it to `.seen.txt`. **Report on GitHub** opens `issues/new` with only the summary, version and backtrace in the URL; the log lines stay in the file for the reporter to attach. Panics caught by worker `catch_unwind` safety nets are recorded as well. Native faults that bypass the panic hook (fail-fast, access violations) appear only in Windows' Application event log. See [Diagnostics](TROUBLESHOOTING.md) for the logs.

## Artwork

`assets/director.svg` is the only source for the app icon. `packaging/windows_icon.rs` rasterizes seven sizes with the build-only `resvg` crate, embeds them with `rc.exe` or `windres` (override with `WINDRES`), embeds `packaging/director.manifest`, and stages `director.ico` beside the executable. The installer's 64 px setup logo is copied from that icon.

## Installer

`scripts/package-msi.ps1` builds both installer files with WiX 6.0.2 into `target\installer`:

```powershell
.\scripts\package-msi.ps1                      # release build, then package
.\scripts\package-msi.ps1 -UseExistingRelease  # package the existing release build
.\scripts\package-msi.ps1 -Wix C:\tools\wix.exe
```

It adds the pinned WiX Util and bootstrapper extensions to the global WiX cache when missing.

- `Director-<version>-x64.msi` comes from [installer/Director.wxs](../installer/Director.wxs): a per-machine package with no dialogs of its own. It installs to `%ProgramFiles%\Director`, adds shortcuts, the `director-run:` handler and the PATH entry, and refuses Windows builds below 26100 through a launch condition.
- `Director-<version>-x64-setup.exe` is a WiX Burn bundle from [installer/DirectorSetup.wxs](../installer/DirectorSetup.wxs) with the [Director theme](../installer/theme). It embeds the MSI, checks the Windows build, and installs the Windows App Runtime first when the pinned package isn't registered. Releases publish only this file.

The runtime is pinned in two places. To move to a newer one, update the version, download URL, SHA-512 hash and size in `DirectorSetup.wxs` (`wix burn remotepayload` generates them), and `$runtimeVersion` and `$runtimePackage` in the packaging script. Users see what setup does in [Install](INSTALL.md).

`windows-reactor` is a preview crate, pinned to `=0.100.0` with `Cargo.lock`; its published API differs from current upstream examples, so check the matching source before using a new API. It needs the framework-dependent Windows App Runtime 2.4 or newer and shows a download prompt when it's missing; the CLI doesn't need the runtime. The executable embeds `packaging/director.manifest` (Windows 10/11 supported OS, `maxversiontested` 10.0.26100.0, per-monitor v2 DPI). Raise `maxversiontested` after testing on a newer Windows release.

## GitHub Actions

The [Windows build workflow](../.github/workflows/windows-build.yml) runs for pull requests and pushes to `main`, and can be run by hand from the Actions tab. On the hosted Windows runner with the MSVC toolchain it checks formatting, runs Clippy and the tests, builds the release executable, packages the MSI and the `setup.exe` that embeds it, and uploads the setup as the **Director-Windows-x64** artifact (GitHub zips artifacts; release assets download as the plain file). When a merge into `main` raises the version, the workflow publishes a GitHub Release with that setup. A manual run on `main` with `publish_release` enabled publishes the current version. See [Setup and MSI](#installer).

## Versioning

Set the version in the root [Cargo.toml](../Cargo.toml) under `[package]`, as three numbers such as `0.12.1`, which the MSI needs. Run `cargo check` afterwards to update the root package entry in `Cargo.lock` before a `--locked` build. The [packaging script](../scripts/package-msi.ps1) reads the version for the MSI and setup versions and file names. Add a line to [CHANGELOG.md](../CHANGELOG.md) for each release.

## Third-party notices

`THIRD_PARTY_LICENSES.md` lists the license of every crate compiled into `director.exe`, and the installer copies it beside the executable with `LICENSE.txt`. Regenerate it after adding or updating a dependency:

```powershell
cargo install cargo-about --locked --features cli
cargo about generate -c packaging/about.toml packaging/about.hbs -o THIRD_PARTY_LICENSES.md
```

`packaging/about.toml` lists the accepted licenses. Generation fails if a new crate uses one that isn't listed, so review that license before adding it there.

## Protecting main

`main` only changes through pull requests that pass the **Windows x64 build** check. Locally, run `git config --local core.hooksPath .githooks` after cloning: the pre-push hook refuses direct pushes to `main`.
