# Architecture proposal

Status: the first audio slice implements the shared transaction engine in src/audio.rs, a direct Windows adapter in src/windows_audio.rs, versioned audio-preset persistence in src/presets.rs, and WinUI 3 entry point and headless CLI in src/main.rs. Broader module boundaries and scene/storage design below remain proposals. See IMPLEMENTATION.md for validation.

## Shared engine

Keep preset capture, persistence, validation, planning, application, verification, and recovery in Rust code independent of the GUI. The GUI and CLI call this same engine so behavior does not drift between entry points.

The CLI path must not construct the GUI, start its event loop, or require an existing GUI process. The first slice uses one director executable: no arguments open the GUI; audio subcommands run without UI initialization.

## Integration policy

Use direct Windows API calls, device-protocol clients, and linked Rust/native libraries wherever practical. Do not spawn NirCmd, Display Changer II, ColorControl, rtss-cli, frltoggle, or batch files to implement their replacement workflows. A library compiled or linked into Director is different from an independently installed helper application. Keep unavoidable dependencies explicit. User-configured application launches and URI dispatch remain supported scene actions.

## Proposed boundaries

| Component | Responsibility |
| --- | --- |
| Domain/model | Display presets, audio presets, scenes, action definitions, device identities, failure policy, operation outcomes |
| Engine | Capture, validate, build an ordered plan, snapshot affected state, apply, verify, and recover |
| Windows displays | Enumerate/query layouts and apply/verify Windows display changes |
| Windows audio | Enumerate endpoints, read identity/name/availability, select defaults, read/set device volume, observe availability changes |
| LG integration | Pairing, Wake-on-LAN, webOS connection, input/power actions, bounded readiness waits |
| Frame-limit providers | Discover supported direct GPU-driver controls and optional RTSS; query limits, snapshot/apply/verify/restore the selected provider |
| Launch integration | Executable launch and URI dispatch; accurate dispatch outcomes |
| Persistence | Versioned presets, remembered inventory, operation results, recovery state, separate credential storage |
| Notifications | Native Windows notifications and activation into result details |
| GUI | Native WinUI 3 views, user input, progress, results; no duplicated device-control policy |
| CLI | Argument parsing, shared engine invocation, output and exit status |

These are logical boundaries, not a committed crate/package layout.

## Proposed data relationships

- Give app presets and scenes stable internal IDs with editable display names.
- Keep app preset IDs distinct from Windows device identities.
- Let a scene reference optional display/audio presets rather than copy their settings.
- Editing a referenced component would affect future applications of every scene that uses it; duplication would create an independent variant.
- Show dependent scenes before deleting a component and prevent dangling references.
- Store optional fields explicitly so an omitted field means leave unchanged.

Reference/update/deletion semantics need user confirmation before the schema is finalized.

## Initial creation UI

Capture current layout reads monitor settings only. Audio presets use independent input/output dropdowns populated from the observed/remembered inventory. Each selected device has an optional Set volume switch and a 0–100% value; disabled means leave volume unchanged. Editing or saving a preset does not apply it. Reading current audio state for status or rollback remains necessary and is separate from a user-facing audio-capture feature.

## Initial audio role scope

Output presets map Console, Multimedia and Communications to the selected output by default. An optional separate communications output overrides only that role. An independently optional microphone selection changes all three capture roles; microphone volume is optional. Snapshot every affected render/capture role and requested endpoint volume before the first write, and verify/restore them independently. See AUDIO.md.

## Device identity

Audio endpoint identity is separate from its mutable Windows name and current availability. Persist the preferred Stable ID when available, plus the current ordinary endpoint ID and last observed name. Re-query properties when resolving a device; never parse opaque IDs or resolve ambiguity by friendly name alone.

The display identity strategy needs separate validation. Do not assume transient display numbering or adapter IDs survive hardware/driver changes.

## Proposed execution lifecycle

1. Resolve the requested preset/scene and obtain exclusive operation ownership.
2. Validate configuration and dependencies that can be checked without changing hardware.
3. Capture known previous state for affected resources before changing them.
4. Prepare devices needed by later steps, such as waking the TV and selecting its input.
5. Apply/verify display settings, wait for the requested audio endpoint, then apply/verify audio settings.
6. Apply other requested configuration, including the selected frame-limit provider.
7. Dispatch optional final actions if the agreed gating rules permit them.
8. Persist and report the result; notify according to settings.

On a required failure, use the selected policy. The default recovers affected settings using the captured state, keeps operation ownership during recovery, and reports the recovery result separately from the original failure.

The accepted scene stage order is Display → Audio → Apps & URIs. Skip omitted stages and constrain user reordering to steps within their stage. TV preparation/readiness is part of the display stage; refresh audio availability after changing displays. Detailed device dependencies must handle both desk and TV transitions without allowing cross-stage reordering. The exact placement of frame-limit configuration remains to be finalized; it must precede final launches.

## Frame-limit providers

Keep frame-limit actions independent of the limiting mechanism. A provider reports availability, supported scope/range, and read/apply/restore capabilities. Research direct NVIDIA NVAPI, AMD ADLX FRTC, and Intel IGCL alongside RTSS. Users without RTSS should still be able to use a supported driver provider. GPU vendor alone is insufficient to establish support, and multiple-GPU systems require an explicit target. Do not promise that a saved driver setting takes effect in an already-running game or reproduces RTSS behavior. Preserve previous provider settings for recovery, and do not silently enable multiple limiters or change unrelated driver settings. Driver-based limiting is the primary/default method when supported; RTSS and other future external limiters are optional alternatives. Add a Set FPS limit switch, Method dropdown, and method-validated FPS value in the relevant action editor; off means leave existing limits unchanged. Never silently fall back to an external provider. Provider implementation details/order remain open; audio switching stays the first work item.

## UI and execution threading

The UI must remain responsive during discovery, preset/scene application, device readiness waits, verification, and rollback. WinUI owns the main UI/event-loop thread. Run device work on a background Rust worker/executor and send progress/result messages back to the UI; never perform blocking Windows calls, synchronous network/file work, sleeps, or worker joins in UI callbacks.

The first audio slice uses Reactor background jobs with command/result messages. Serialize mutation requests even when background tasks are asynchronous; separate threads must not permit overlapping preset changes. The operation uses a snapshot of the requested preset so edits do not alter an in-flight plan.

Reactor background tasks return plain-data messages to the component update loop. Keep UI updates short and exchange no COM objects across threads. Merely marking a callback async does not move blocking work off the UI thread.

The CLI uses the same execution engine without GUI initialization. Normal closing is deferred while work is active. Forced termination recovery and interruption of an unresponsive native call remain unresolved; do not assume every system call can be safely canceled.


## Responsiveness and resource use

- Keep network and device waits off the UI thread.
- Prefer device notifications and bounded waits over permanent polling.
- Run CLI operations without UI initialization.
- Keep tray residency and startup integration out of the first implementation.
- Measure release-build behavior before selecting rendering options or claiming resource targets.

## State and local files

During development, keep repository-local notes, downloaded tools, test credentials/inventories, logs, and other machine-only artifacts inside .localonly/. The machine-wide Git excludes file owns the /.localonly/ rule; do not duplicate broad local-file patterns in the repository .gitignore. This development convention does not select installed-app storage locations.

Presets, remembered devices, credentials, diagnostic logs, and recovery snapshots have different purposes. Do not treat the last requested/successful scene as proof of the current Windows state.

Choose storage locations, schema migration, credential protection, and interrupted-operation recovery during implementation planning. Real machine inventories, pairing keys, and runtime results must not be committed to the repository.

## Technical starting points

- [Rust for Windows](https://learn.microsoft.com/en-us/windows/dev-environment/rust/rust-for-windows)
- [QueryDisplayConfig](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-querydisplayconfig)
- [SetDisplayConfig](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setdisplayconfig)
- [Audio endpoint Stable ID](https://learn.microsoft.com/en-us/windows/win32/coreaudio/pkey-audioendpoint-stableid)
- [Windows app notifications](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/)

These are research references, not proof of a working integration. Pin dependency/tool versions only when implementation is authorized and they have been validated.


## Implemented audio navigation

Audio presets is the main native NavigationView page. A ListView contains selectable rows with visible Apply/Edit/Delete icon buttons. Add/Edit uses the same window with Name, output ComboBox, optional volume NumberBox, failure policy, device-ID Expander, and Save/Cancel. Selection and storage edits never apply audio. InfoBar reports status and errors; ContentDialog confirms deletion. Stable app preset IDs survive renames. Names are unique ignoring case for CLI lookup.

## Native frontend

The root Cargo package builds one director executable. src/main.rs hosts WinUI through pinned windows-reactor 0.100.0; src/close_guard.rs protects pending operations from normal window closure. src/audio.rs, src/windows_audio.rs, src/presets.rs, and src/cli.rs remain shared backend code. CLI arguments are handled before GUI initialization. Reactor WinRT types and the backend Windows bindings remain isolated; worker results are ordinary Rust data. build/windows_icon.rs generates the executable resource and adjacent director.ico from assets/director.svg. The current framework-dependent GUI uses an installed Windows App Runtime; see [runtime notes](WINDOWS_GUI.md).
## Implemented display capture

The independent domain/storage model is src/displays.rs; src/windows_displays.rs reads CCD paths/modes directly; src/display_ui.rs implements the native capture/list/rename/delete page; src/display_cli.rs handles headless inspection and named capture. Both preset types reuse the known-folder resolution and atomic document write helper, without sharing names or documents. Display jobs use the existing global operation gate and return plain-data messages. src/display_apply.rs provides the shared verified transaction and independent confirmation deadline; src/windows_display_apply.rs resolves current monitor paths/routes and implements native validation, temporary application, persistence, and exact before-state restoration. See [the Apply contract](DISPLAY_APPLY.md). See [display schema and limitations](DISPLAY_PRESETS.md).

## Custom display draft editor

src/display_editor.rs owns pure draft transformations and validation; src/windows_display_modes.rs discovers supported progressive SDR modes through DXGI and a CCD preferred-mode fallback. src/display_editor_ui.rs supplies native controls, the interactive arrangement map, and SVG orientation flyouts. Mode discovery is background-only and returns plain data. Editing never calls a native setter. Saving writes the existing display-preset schema; Apply continues through the shared verified transaction. The build stages four orientation SVGs beside the executable in orientations/.

The shared preset heading is src/preset_ui.rs. Display draft pointer gestures use Reactor Border pointer capture, window-relative coordinates, and plain-data Drag/MapTransform domain objects. Layout edits are provisional until release; cancellation restores the pre-drag draft. Dropping snaps edges, resolves overlap, and normalizes the primary origin. No gesture invokes Windows setters. The numeric coordinate UI and separate Customize action have been removed. Edit discovers capabilities in the background immediately.

## Live configuration and HDR

src/current_ui.rs holds an independent in-memory Current setup session. The shared display editor routes plain-data events to either the preset or Current draft. The synchronous gesture/change subset reuses the existing handler under a scoped state swap; Current capture-loss timers are routed explicitly to the Current session. Preset persistence/Apply messages are excluded from that bridge. Display and audio Apply call the shared engines directly and reread actual state; preset documents are never accessed.

src/windows_hdr.rs defines the HDR-specific CCD ABI missing from the pinned Windows bindings. Optional monitor HDR state is captured by windows_displays, exposed via capability options, checked in display_apply verification, and set/restored after topology by windows_display_apply. The previous adapter-LUID dependency in mode-list matching was removed; identity is the monitor interface path, and mode filtering retains unspecified/progressive scan-line modes.

## First TV slice — implemented

`src/tvs.rs` owns versioned remembered-TV definitions and optional Windows target-path metadata. `src/lg_protocol.rs` validates bounded, correlated SSAP registration/input-list messages. `src/windows_lg.rs` provides native WinHTTP TLS/WebSocket connections, cancellation deadlines, certificate continuity checking, and per-user DPAPI credential protection. `src/tv_ui.rs` and `src/tv_cli.rs` share these modules and the operation lock; no TV helper executable or persistent service is needed. GUI I/O remains on workers. Display links do not apply presets or imply readiness. See [TVs](TVS.md) for the exact first-slice boundaries and proposed MAC/IP identity improvement.

## Guide and wake update

The tv_setup module owns progression/gating and invalidation when a new draft’s endpoint changes. The TV UI retains its protected pairing in memory until final save and runs pairing/metadata reads on workers. Defaulted inputs, pc_input, and network fields keep existing paired definitions readable. The Windows LG inspection reads inputs and optional TV-reported wired/Wi-Fi MACs; refused/partial reads do not erase remembered valid addresses. Re-pairing retains a successful key even if subsequent metadata reads fail.

The tv_network module validates/deduplicates MACs, builds standard magic packets, and dispatches three UDP broadcast rounds. GUI Wake and CLI tv wake share it under the operation lock, without a webOS session or key. Dispatch does not establish physical wake or trigger display/audio/input changes. PC input is connection metadata for later scene actions. Discovery, verified IP migration, input switching, power-off, and scene readiness remain separate work.

## Shared TV controls

The tv_control module defines PowerState, explicit PowerAction, snapshots, a backend trait, fresh-state intent handling, input validation/no-op detection, and bounded readback. WindowsTvControls in windows_lg provides a worker-local authenticated WinHTTP session, latched connection errors, TV input/app-ID mapping, and explicit SSAP off/switch calls. It uses the existing wake sender. New status/off/input CLI commands share this engine; wake stays explicit direct packet dispatch.

The tv_controls_ui module owns ephemeral dialog state, pending target selection, revision-tagged results, and background jobs. It checks the current saved TV definition under the operation lock before connecting or acting, rejecting stale windows. Snapshots are not persisted. A bounded post-action refresh updates the UI, without inferring physical Off from network disappearance or claiming signal readiness from foreground input readback. Main-window close protection remains active during jobs.


## Local diagnostics

The diagnostics module owns a bounded nonblocking producer queue, one process-local writer thread, cross-process file locking, rotation, UTC JSON-line records, operation scopes and timed steps. Both GUI and CLI initialize it and flush briefly before normal exit. Device APIs do not depend on logging success. TV transport logs protocol stages rather than payloads or keys. Audio/display engines record progress and results through the shared logger. The native Options footer opens options_ui; its log-folder action uses the Windows-known Local AppData directory and Windows File Explorer directly on a worker. TV controls keep command details and status details in separate fields. See [diagnostics](DIAGNOSTICS.md) for retention, overload behavior and validation limits.



## Command-aware TV modal transitions

The root tv_power_state module owns ephemeral Idle/Waking/TurningOff/AssumedOff/WakeExpired states and monotonic retry decisions, independently of reported PowerState. tv_control::Report exposes typed effects; wake_snapshot reads power first and reads input metadata only after an awake response. The GUI updates confirmed input changes directly and marks accepted turn-off as assumed Off without a follow-up timeout. Wake completion schedules revision-tagged read/timer messages through background workers; each callback validates dialog lifetime/revision before starting another attempt. Successful command processing requests reconciliation only when its effect is uncertain or requires wake readiness. Command worker panics/delivery failures return through the same completion path so busy state cannot leave the modal permanently pending.


## Optional setup-tip preferences

preferences.rs stores versioned preferences.json in the Windows-known Local AppData Director directory. It uses the existing atomic document writer and rejects stale, corrupt or future-version overwrites. tv_tip_ui owns asynchronous loading/saving, session dismissal, pending preference state and the saved-TV catalog used for contextual guidance. Preference handlers do not release an unrelated device-operation busy guard. The TV guide supports an add-after-load request; display state stays intact during navigation. The shared monitor editor renders linked-TV context in a width-constrained row.


## Temporary display discovery

display_detection.rs owns the backend-independent snapshot/prepare/enable/read/restore/verify transaction, original-screen comparison, cooperative cancellation/deadline and retained recovery state. windows_display_detection.rs adapts it to the existing WindowsDisplays snapshot, temporary CCD setter and live DXGI/CCD inventory. Optional TV preparation shares native WOL/SSAP control. Restoration uses SDC_APPLY without saving a new database topology. display_capabilities.rs stores validated complete inventories, merges remembered inactive choices and installs learned data into the draft after verified restoration.

display_setup_ui.rs owns inline review/progress/result/recovery state and sends stage updates through a standard worker channel. Native setters and cache I/O do not run on the UI thread. The shared operation lock and existing close guard cover setup; restoration ignores ordinary work cancellation so it can finish safely. See DISPLAY_SETUP.md for the exact behavior, limits and validation boundary.


TV controls own a cancellable PowerWatch. A native worker receives authenticated SSAP power events; the view drains them through its token-checked message path. Read-only reconnects never acquire the mutation lock or wake the TV. Explicit wake retransmissions use the existing operation lock and a bounded per-request retry policy.

## Scene composition

scenes.rs owns independent ID references, versioned persistence, validation and the ordered framework-independent transaction. windows_scenes.rs supplies native snapshots, bounded TV/audio readiness, display/audio verification, launch dispatch and restoration under OperationLock. scene_ui.rs keeps drafts/cards/progress on the UI thread, owns cancellation and confirmation channels, and runs I/O on workers. TV audio metadata remains in tvs.json; selecting that source resolves its current saved endpoint ID without duplicating an audio preset. See SCENES.md for recovery boundaries and known limits.

## Samsung provider boundary

windows_tv dispatches setup, control, CLI, scene and display-preparation operations by persisted TV brand. Missing brand defaults to LG. samsung owns direct-command schemas, latest confirmed capabilities and the allowlisted report; windows_samsung implements native Tizen transport and guarded power operations. Existing WinHTTP socket/DPAPI helpers in windows_lg are shared, including pre-header TLS pin validation for token-bearing Samsung URLs. LG keeps its subscription; Samsung uses a modal-scoped bounded power poll. tv_setup adds brand selection; samsung_ui presents an optional test and explicit visual confirmation before enabling each port. No sequence fallback or cloud integration is present.

Android TV Remote v2 uses an independent Rust TLS/protocol implementation (android_tv.rs / windows_android_tv.rs) behind windows_tv. Pending pairing sessions stay in memory while entering the code; account-protected per-TV client identity is persisted only after successful confirmation. Android and Samsung maintain separate compatibility metadata but share the explicit input-test UI/report format. Shared DPAPI document helpers retain the existing LG/Samsung token validation. No new runtime process is introduced.

Implemented global FPS providers: fps.rs owns requests, snapshots, verification/recovery; windows_fps.rs loads NVIDIA DRS / installed RTSS interfaces. fps_ui.rs shares controls across Current setup and scenes. Scenes snapshot FPS during preflight and apply after audio, before launch actions. Global driver profile is the explicit NVIDIA target; it is not a monitor-specific cap. Blocking calls remain worker-owned and SDK sessions are serialized.
