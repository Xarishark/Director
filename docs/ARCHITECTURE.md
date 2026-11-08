# Architecture

Director is one Rust executable, `director.exe`. Started without arguments it opens a native WinUI 3 window; with arguments it runs a CLI command and exits without creating any UI. Both paths call the same library code, so a scene behaves the same from the window, a terminal or a Stream Deck shortcut.

## Crates and entry points

| Part | Where | What it holds |
| --- | --- | --- |
| Library crate `director` | `src/lib.rs` | Domain models, persistence, the scene and device engines, Windows and TV backends, the CLI |
| Binary | `src/main.rs` and `src/gui/` | The `DirectorApp` WinUI component (through `windows-reactor` 0.100), its pages and dialogs, and the shared UI vocabulary in `gui/ui.rs` |
| Build script | `build.rs`, `packaging/windows_icon.rs` | Renders `assets/director.svg` into the embedded icon and `director.ico`, embeds `packaging/director.manifest`, stages SVG assets beside the executable |
| Installer | `installer/`, `scripts/package-msi.ps1` | WiX MSI plus a `setup.exe` bundle that installs the Windows App Runtime when missing; see [Development](DEVELOPMENT.md#installer) |

## Integration policy

Director talks to Windows and to TVs directly: Win32 and WinRT APIs, Core Audio, the display-configuration (CCD) API, NVIDIA's driver interface, the RTSS shared interface, and each TV's local network protocol. It does not shell out to helper tools such as NirCmd, display-switching utilities or TV CLIs. Launching apps and opening URIs are user-chosen scene actions, not implementation shortcuts.

## Library modules

Each folder groups one area; Windows-specific code sits in the `windows_*` module beside the platform-neutral model it implements.

| Folder | Modules | Notes |
| --- | --- | --- |
| `src/scene/` | `scenes`, `scene_session`, `starter`, `windows_scenes` | A scene references a display preset and a TV by ID and holds its own Sound, FPS and action steps. `windows_scenes` runs the ordered transaction: Before → TV → Displays → Sound → FPS → After, with When leaving steps and an optional Return scene. See [Scenes](SCENES.md). |
| `src/display/` | `displays`, `display_apply`, `display_editor`, `display_capabilities`, `display_detection`, `display_mirroring`, `display_cli`, `windows_displays`, `windows_display_apply`, `windows_display_modes`, `windows_display_detection`, `windows_hdr` | Capture and apply layouts through CCD with strict readback and an exact restore. `windows_hdr` defines the HDR requests missing from the `windows` bindings. See [Displays](DISPLAYS.md). |
| `src/audio/` | `mod.rs` (`audio::apply`), `audio_control`, `audio_devices`, `audio_cli`, `windows_audio` | `audio::apply` is the multi-step transaction a scene's Sound step uses; `audio_control` makes one live change at a time for Now and `director audio`. Both sit on one `AudioBackend` (default roles, volume, mute). `audio_devices` remembers endpoints seen before. `windows_audio` also holds the cross-process `OperationLock`. See [Sound](SOUND.md). |
| `src/tv/` | `tvs`, `tv_setup`, `tv_control`, `tv_network`, `tv_input_guard`, `tv_report`, `tv_cli`, `windows_tv`, and per brand `lg_protocol` + `windows_lg`, `samsung` + `windows_samsung`, `android_tv` + `windows_android_tv` | Saved TV definitions, the setup guide's state, a brand-neutral control trait, Wake-on-LAN, input protection and compatibility reports. `windows_tv` dispatches by brand. LG and Samsung run over WinHTTP WebSockets; Android TV Remote v2 over in-process TLS (`rustls`) with a per-TV client certificate. See [TVs](TVS.md). |
| `src/fps/` | `mod.rs`, `windows_fps` | Global NVIDIA or RTSS frame-rate limit with snapshot, readback and restore. See [FPS](FPS.md). |
| `src/cli/` | `mod.rs`, `cli_list`, `cli_name` | Argument parsing, `director list` and CLI names, which are derived from item names and never stored. Each area's commands live in its folder (`audio_cli`, `display_cli`, `tv_cli`). |
| `src/` | `storage`, `preferences`, `backup`, `diagnostics`, `crash`, `notifications`, `windows_close` | Settings files, preferences, backup and restore, logs, crash reports, Windows notifications and close protection. See [Troubleshooting](TROUBLESHOOTING.md). |

The window lives in `src/gui/`: one module per page or dialog (`scene_ui`, `scene_editor_ui`, `now_ui`, `display_ui`, `tv_ui`, `options_ui` and others), `editor_flow` for the jumps between pages, and `ui` for the shared layout and theme helpers.

## Threading and locking

WinUI owns the UI thread. Device, network and file work runs in background jobs started with `work(cx, closure)`; each returns plain Rust data as a `Message` to the component's update loop. COM objects never cross threads, and UI callbacks never block, sleep or wait on a worker.

Every device change and every settings write holds `audio::windows_audio::OperationLock`, a named mutex shared by all Director processes, so the window and a CLI scene can't change devices at the same time. While work is pending, a window subclass (`close_guard`) defers normal closing. Native calls can't be cancelled safely, so cancellation is cooperative and recovery from a forced kill is not attempted.

## Transactions and recovery

Scenes, display Apply, the Sound step, FPS changes and temporary display detection share one pattern:

1. Resolve the request and take the operation lock.
2. Validate everything that can be checked without changing hardware.
3. Snapshot the state that will change.
4. Apply each step, then read the real state back to verify it.
5. On a failed required step, restore the snapshot (unless Options says to keep completed changes) and report the restore result separately from the failure.

TV commands, app launches and URIs can't be undone, so they are reported but not part of automatic recovery.

## Data and identity

- Settings live in `%LOCALAPPDATA%\Director`, resolved through the Windows known-folder API. `storage::write_document` validates, checks the file hasn't changed since it was read, writes a temporary file and replaces the original atomically. Corrupt or newer-version files are kept and reported, never overwritten.
- Saved items have stable IDs and editable names. Scenes reference display presets and TVs by ID; deleting an item that a scene still uses is refused and names those scenes.
- Audio endpoints are matched by Windows Stable ID when available, then endpoint ID, never by friendly name alone; offline devices can be relinked.
- Displays are matched by monitor device path, not by transient display numbers or adapter IDs.
- TV pairing keys and client identities are encrypted with Windows DPAPI for the current user and never leave the PC: backups and reports exclude them.

## References

Windows APIs:

- [Rust for Windows](https://learn.microsoft.com/en-us/windows/dev-environment/rust/rust-for-windows) and [windows-reactor](https://github.com/microsoft/windows-rs/blob/master/crates/libs/reactor/readme.md)
- [QueryDisplayConfig](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-querydisplayconfig), [SetDisplayConfig](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setdisplayconfig) and [display device-info types](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ne-wingdi-displayconfig_device_info_type) (HDR)
- [Audio device roles](https://learn.microsoft.com/en-us/windows/win32/coreaudio/device-roles) and [endpoint Stable ID](https://learn.microsoft.com/en-us/windows/win32/coreaudio/pkey-audioendpoint-stableid)
- [Windows app notifications](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/) and the [WinHTTP status callback](https://learn.microsoft.com/en-us/windows/win32/api/winhttp/nc-winhttp-winhttp_status_callback) (certificate pinning before headers are sent)

Projects consulted for protocols and design. Director doesn't depend on, bundle or run any of them; its implementations are independent Rust code:

| Project | Used as a reference for |
| --- | --- |
| [lgtvcli](https://github.com/01dnot/lgtvcli), [PyWebOSTV](https://github.com/supersaiyanmode/PyWebOSTV), [lgtv2](https://github.com/hobbyquaker/lgtv2) | LG webOS pairing, inputs, power and network information |
| [samsung-tv-control](https://github.com/timdevlet/samsung-tv-control), [ColorControl](https://github.com/Maassoft/ColorControl) | Samsung Tizen pairing, power state and direct HDMI keys |
| [androidtvremote2](https://github.com/tronikos/androidtvremote2), [Polo pairing protocol](https://android.googlesource.com/platform/external/google-tv-pairing-protocol/) | Android TV Remote v2 pairing and commands |
| [NVAPI](https://github.com/NVIDIA/nvapi), [rtss-cli](https://github.com/xanderfrangos/rtss-cli) | NVIDIA driver profile settings and RTSS profile interfaces |
| [DisplayProfileManager](https://github.com/exytral/DisplayProfileManager), [DisplayHotKeys](https://github.com/jon-mil-92/DisplayHotKeys) | Display editor and mode-switching workflows |
