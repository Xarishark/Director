# Development and testing

## Build and run

Director is one Rust application with a native WinUI 3 window through Microsoft Windows Reactor 0.100.0. Running without arguments opens the window; CLI commands use the same backend and return before GUI initialization.

Develop on Windows x64 with Rust 1.95 or newer, a Windows linker/resource compiler, and a compatible Windows App Runtime. The validated local setup uses Rust 1.99.1, x86_64-pc-windows-gnullvm, LLVM-MinGW 20261020 (UCRT), Windows build 26300, and the already installed Windows App Runtime 2.6.4. Other Windows toolchains/builds remain unvalidated. See [runtime and interface notes](WINDOWS_GUI.md).

```powershell
.\scripts\dev.ps1 check --locked
.\scripts\dev.ps1 test --locked
.\scripts\dev.ps1 build --locked --release
.\scripts\dev.ps1 run --locked
.\scripts\dev.ps1 -Command clippy -CargoArguments @('--locked', '--all-targets', '--', '-D', 'warnings')
.\scripts\dev.ps1 -Command fmt -CargoArguments @('--all', '--', '--check')
```

The script detects existing project-local Rust/Cargo and LLVM tools in .localonly/tools, or uses a suitable toolchain already on PATH. It explicitly sets target as the build output and changes environment variables only for its process. The GNULLVM configuration requests static CRT linking to avoid a toolchain libunwind.dll dependency. The script does not install a runtime or change system PATH.

Open target/release/director.exe after a release build. Keep the generated director.ico beside the executable for the window icon. The executable also has an embedded icon resource. Cargo.lock is part of the application source; target and globally ignored .localonly are machine-local. No bootstrap or installer is provided yet.

## Command-line usage

```text
director --help
director list
director audio list
director audio apply desk
director audio outputs
director audio outputs --json
director audio check
director audio apply "<endpoint-id>"
director audio apply "<endpoint-id>" --volume 40
director audio apply "<endpoint-id>" --keep-on-failure
```

- list reads endpoint availability and current normal-audio assignments; check additionally checks policy-interface availability. Neither changes audio.
- presets lists saved presets. apply resolves a name ignoring case, or an exact Windows endpoint ID beginning with {. It rejects unavailable targets and changes Console and Multimedia together.
- Named presets use their saved optional volume and failure policy. --volume overrides volume for one invocation and accepts integers 0 through 100. Omitted volume leaves the target volume unchanged.
- --keep-on-failure selects keep-changes behavior. Otherwise a named preset uses its saved policy; raw endpoint requests default to reverting.
- Exit 0 means success, including an already-matching configuration; 1 means application failed; 2 means argument, access, or initialization error.

Readback verifies Windows settings, not audible playback. Applications explicitly bound to a device can retain it. Director preserves mute and does not change per-application routing or explicitly modify Communications.

## Backend, worker, and storage

Core Audio enumeration/default reads and endpoint volume use direct Windows APIs. Default switching uses the undocumented PolicyConfig interface isolated in src/windows_audio.rs. Interface failure is reported without invoking a helper executable.

Reactor background jobs perform storage and audio work. COM objects are created and released in their worker context; only plain Rust data crosses into UI messages. The UI does not synchronously wait or poll hardware. GUI requests are serialized; a local-session named mutex also protects Apply and preset-file writes across processes. A native window subclass defers normal closure while work is pending. Native calls cannot safely be forcibly canceled; forced-termination recovery is not implemented.

Presets use schema version 1 at %LOCALAPPDATA%\Director\audio-presets.json. SHGetKnownFolderPath resolves the real Windows Local AppData folder, including redirection; errors do not fall back to arbitrary folders. Each preset stores a stable internal ID, editable unique name, endpoint ID and label, optional volume, and failure policy. Names must be nonblank, unique ignoring case, and at most 160 UTF-8 bytes. The picker remembers devices that Director has seen. See [Audio](AUDIO.md) for identity matching and relinking.

Saving validates the document, checks the current on-disk list matches the window snapshot under the mutex, flushes a temporary file, and replaces the original with MoveFileExW replace-existing/write-through. Stale writers must refresh. Corrupt/unsupported files are preserved and reported. Atomic replacement is not a permanent recovery journal or a power-loss guarantee. Saving, cancellation, and deletion never apply audio.

## Artwork and validation

assets/director.svg is the editable source for the README and native icons. build/windows_icon.rs rasterizes seven sizes with build-only resvg and embeds the resource using windres (WINDRES override) or rc.exe. The build stages director.ico beside the binary. MSVC builds remain unvalidated.

Automated audio tests use a fake backend and cover no-op behavior, optional volume, partial failures, prior role assignments, and recovery. Persistence tests cover round trips, duplicate names, stale writes, and corrupt/future files. A hidden native window test checks busy-close protection.

Private UI checks use uniquely named fixtures and remove them afterward while preserving original presets. Audio UI checks only Apply a guaranteed nonexistent endpoint, exercising preflight without Windows audio writes. Capture only the owned application window. Real mouse/keyboard behavior and physical switching require separate validation; record observed results in [implementation status](IMPLEMENTATION.md).

Manual acceptance: create/save a preset without applying; reopen; edit/cancel; edit/save; confirm the CLI lists it; apply without volume, then with optional volume; inspect actual defaults/volume; delete without changing audio; restore your preferred output. Keep read-only/UI evidence distinct from real routing and hardware recovery checks.
## Display capture

See [display presets](DISPLAY_PRESETS.md) for native capture, schema, identities, CLI commands, and limits. Display Apply is implemented separately through direct SetDisplayConfig validation, temporary application, native/CLI confirmation, persistence, and restoration. See [the Apply contract](DISPLAY_APPLY.md). Audio and display drafts, names, and settings files remain independent.

Display Apply follows the saved **Ask to keep display changes** preference. Override it for one invocation with --yes (keep verified changes automatically) or --confirm (require the terminal confirmation). New presets default to keeping verified changes automatically. Existing preferences are preserved; legacy presets without the field still ask. Both paths retain readback and failure recovery.

Native UI Automation: query an open ComboBox's own descendants with a name/type condition rather than recursively enumerating the entire window. An unrestricted FindAll against the open live-audio dropdown stalled the provider during testing; the scoped query and selection completed normally. This does not establish full screen-reader compatibility.

## TV setup checks

See [TVs](TVS.md) for the setup flow and `director tv list`, `director tv pair <tv>`, and `director tv check <tv>`. TV commands are dispatched before Windows audio initialization and do not create the WinUI window. Add/edit/remove definitions in the native TVs view. Pairing is explicit, prompts on the TV, and stores only DPAPI-protected credentials. Connection checking is read-only on the TV. No persistent control process is required.

Offline tests include a loopback WebSocket mock using native WinHTTP, a silent-peer deadline test, and DPAPI tampering checks. They do not contact physical TVs. Firmware/TLS/permissions must still be validated against an on, reachable LG TV with user approval on its screen. There is no discovery or MAC/address migration yet; avoid treating mock success as a hardware compatibility claim.

## Guided TV and wake validation

See TVS.md for the new guide and tv learn/wake. Existing TVs can use Edit → Read TV details → Save TV without recreation. Normal tests send wake packets only to a loopback receiver; the headless missing-MAC path rejects before UDP dispatch. A successful wake command means packet dispatch, not verified panel wake.

The ignored tv_ui::native_preview::guide_fixture application test provides test-only in-memory data for native input/wake/display/review checks. A private owned-window script launches it explicitly, never saves it, and never contacts a TV. Production exposes no fixture flag. Physical validation must separately check firmware network-information permissions, MAC reads, the full on-TV pairing guide, and wake with the TV’s network power-on setting enabled.

## TV control validation notes

TV status/off/input CLI commands use the shared control engine; see TVS.md. Ordinary tests exercise the actual native WebSocket power-off/input payloads against a loopback mock, never a physical TV. Power-off acknowledgment is distinct from physical panel state, and foreground input readback is distinct from HDMI/Windows readiness.

The ignored tv_controls_ui::native_preview::controls_fixture test opens the modal after startup with in-memory data. It verifies one power action, pending selection, Unknown handling, Close, and layout; its fake definition fails preflight before any network operation. Production has no fixture entrypoint. Read-only live checks separately exercise fresh status/input reads without pressing Power or Switch.

The pinned Reactor version rejects the tooltip wrapper on Refresh inside ContentDialog with StructureUnsupported. The refresh button retains its accessible name without that wrapper. No dependency/cache modification was made. Keep this native regression check when changing dialog content.

The same rule applies to everything a dialog hosts: the scene step dialogs once had tooltips on their action rows (Move up, Move down, Remove) and on the chosen app's name, and opening a step with actions closed the window. Give those controls an automation name instead. `scene_editor_ui::tests::dialog_content_has_no_tooltips` scans the step dialogs, the FPS controls, the crash dialog and the command dialog; extend it when a new dialog hosts shared controls. Shared helpers can hide a tooltip too: the display preview put one on each screen and crashed the Displays step dialog, so dialogs use `layout_preview_in_dialog`, and the test only lets the step dialogs call helpers on its checked list.

## Crash reports

`director::crash::install` sets a panic hook (after `diagnostics::init`), and a fatal Reactor error from `App::run_component` is recorded too. Each report is a text file in `Logs\Crashes` (`crash-<unix ms>.txt`, newest 10 kept) with the summary, version, Windows build, message, backtrace for panics and the newest 40 log records. On the next start the newest unoffered report is shown once; dismissing it renames it to `.seen.txt`. Report on GitHub opens `issues/new` with only the summary, version and backtrace in the URL: the log lines stay in the file for the user to attach. Panics caught by worker `catch_unwind` safety nets are recorded as well, since each one is a bug. Native faults that bypass the panic hook (fail-fast, access violations) still appear only in Windows' Application event log.


## Action SVG assets

Build stages assets/icons/remote.svg and delete.svg into each Cargo profile’s icons directory. Keep that directory beside director.exe when copying the preview, alongside the existing icon/orientation assets. The shared icons module uses the profile folder for test executables in deps. No image-generation dependency or helper executable was added. Native checks should retain accessible action labels, verify SVG rendering, and confirm Apply is unchanged.

For visual captures, foreground the owned test window and allow a frame to render. An occluded-window run produced a blank captured client despite valid native accessibility controls; foreground recapture verified the actual UI.


## Runtime diagnostics

Use **Options → Open logs folder** to inspect the automatically rotating local logs. [Diagnostics](DIAGNOSTICS.md) describes file retention, operation correlation, covered stages, excluded secrets, and best-effort/queue limits. Reproduce a failure with the current build before reading its log; older attempts made before logging existed cannot be reconstructed.


## GitHub Actions

The [Windows build workflow](../.github/workflows/windows-build.yml) runs for pull requests and pushes to main. You can also run it manually from the Actions tab. It uses the hosted Windows runner and the MSVC Rust toolchain, runs formatting, Clippy and tests, then builds the x64 MSI and the setup.exe that embeds it, and uploads the setup.exe. Find the unsigned installer under the run’s **Director-Windows-x64** artifact. GitHub zips Actions artifacts; release assets download as the plain file. A version increase merged into main publishes a GitHub Release with the verified setup.exe after the build succeeds. See [setup and MSI](WINDOWS_GUI.md#setup-and-msi). A manual main-branch run can also publish the current version by enabling publish_release. GUI and physical-device checks remain separate from CI.
## Versioning

Set the app version in the root [Cargo.toml](../Cargo.toml) under `[package]`. Keep it in three-part numeric form, such as `0.9.1`, for MSI packaging. After changing it, refresh the root package entry in `Cargo.lock` with a normal Cargo check before running the locked build. The [MSI packaging script](../scripts/package-msi.ps1) reads the version from Cargo.toml and uses it for the MSI and setup.exe versions and filenames.

## Protecting main

The build check is named **Windows x64 build**. Configure it as a required check with an up-to-date branch and require pull requests for main, including administrators. Private repository branch protection requires GitHub Pro or an eligible paid organization plan. GitHub rejected protection on the repository’s current plan.

For local protection, run git config --local core.hooksPath .githooks after cloning. This checkout has the same guard installed in its local Git hooks directory so it survives switching branches. The pre-push hook blocks updates and deletion of main. GitHub-side rules are needed to enforce PR-only changes and required checks for all clients.
