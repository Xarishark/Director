# Requirements and implementation order

Status: planned product scope. The audio-output slice now includes named presets and a main view; see [current implementation and validation](IMPLEMENTATION.md) for what works. Requirements below are not a claim that every feature is implemented.

## 1. Scenes, presets, and actions

### Display presets

Capture and apply enabled displays, primary display, resolution, refresh rate, rotation, and desktop positions. Preserve fractional refresh rates where supported. Report missing or ambiguous displays rather than selecting an unintended one.

A display-only operation makes no explicit audio, TV, Steam, or RTSS changes. Windows may naturally change HDMI audio availability when displays are enabled or disabled.

### Audio presets

Create audio presets by selecting input/output devices from dropdowns, not by capturing the current audio configuration. For each selected device, offer a separate Set volume switch with a percentage value from 0 to 100. With the switch off, leave that device volume unchanged. Apply input selection, output selection, and device volume independently. A preset can change only an input, only an output, only an explicit device's volume, or a combination. Omitted settings remain unchanged. An audio-only operation makes no display, TV, Steam, or RTSS changes.

For the initial implementation, switching Default audio sets the Windows Console and Multimedia roles together. Do not expose Communications controls or explicitly change the Communications assignment. Communications support is deferred. Snapshot, verify, and restore each affected role independently, even if their prior endpoints differ.

### Scenes

A scene can select a display preset, an audio preset, either independently, and optional TV/RTSS/final actions. Components can be created independently in any order and reused by multiple scenes.

A scene is the agreed top-level bundle. Presets hold saved settings; actions perform operations such as TV commands, frame-limit changes, executable launches, or URI dispatch. TV preparation actions can run before presets; app/URI launches belong to the optional final chain.

Scene stages have a fixed order: **Display → Audio → Apps & URIs**. Omitted stages are skipped. Users may reorder applicable steps within a stage, but cannot move audio before display changes or launches before configuration. TV wake/input preparation belongs to the display stage when required. After display changes, refresh endpoint availability and wait for the selected audio device before applying audio settings.

References to saved components, update propagation, and deletion behavior remain design proposals; see [the decision record](DECISIONS.md).

## 2. Creation and management

The first implementation captures the current monitor layout into a display preset. Audio presets are created separately with input/output device dropdowns and optional Set volume switches/percentage values. Use remembered devices in these dropdowns, including unavailable devices with clear status. Review, name, and save the result. Capturing, choosing a device, toggling Set volume, and saving configure the preset only; they must not change hardware settings or apply the result.

Provide preset editing, duplication, deletion, and a readable summary. As basic workflows become available, allow independently created presets to be assembled into scenes.

Add step-by-step custom preset creation later while keeping monitor-layout capture available. Its exact release milestone, including whether it blocks 1.0, is undecided. Capture does not infer desired TV power actions, launch actions, or timings from current state.

## 3. Remembered audio devices

- A device must be enumerated at least once before it can be chosen for a preset.
- Retain remembered devices when unavailable and show availability clearly.
- Display the Windows-friendly name and expose a copyable identity in device details.
- Names can change and are not unique. Renaming an endpoint must not change a preset's intended target.
- Prefer `PKEY_AudioEndpoint_StableId` when available on Windows 11 24H2 or later. Fall back to the ordinary endpoint ID when unavailable.
- Treat both IDs as opaque values. Stable IDs are case-sensitive and are not guaranteed permanent.
- Refresh mutable properties after resolving a remembered device. Retain the last observed name while unavailable.
- If the identity can no longer be resolved, allow explicit user relinking. Do not silently substitute an endpoint based on its name.
- Remembering a device does not itself enable automatic switching.

When a scene enables the TV, wait a bounded, configurable period for its remembered HDMI audio endpoint before applying audio settings. A timeout must not cause selection or volume changes on another device.

Technical references: [endpoint IDs](https://learn.microsoft.com/en-us/windows/win32/coreaudio/endpoint-id-strings), [Stable ID](https://learn.microsoft.com/en-us/windows/win32/coreaudio/pkey-audioendpoint-stableid), [friendly name](https://learn.microsoft.com/en-us/windows/win32/coreaudio/pkey-device-friendlyname).

## 4. Integrations

Project-wide requirement: perform supported operations directly in Director using Windows APIs, device protocols, or libraries linked into the app wherever practical. Do not wrap existing batch scripts or require separately launched helper executables for functionality Director can implement directly. Launching a user-selected application or URI is an intentional scene action, not an implementation dependency. Document unavoidable dependencies explicitly; RTSS remains necessary for the currently planned RTSS-based frame limiting.

### Windows display and audio

Control Windows directly instead of requiring Display Changer II or NirCmd. Verify applied state where APIs permit it. API selection and compatibility details need validation during implementation.

### LG webOS

Configure/pair a TV, retain pairing credentials locally, wake it using Wake-on-LAN, select HDMI inputs, and turn it off. Support bounded retries and configurable settling delays. Implement these operations without ColorControl.

Coordinate TV readiness, display selection, and HDMI audio availability. Do not assume that successfully sending a command proves the final physical TV state.

### Frame-limit providers

GPU-driver-based limiting is the primary frame-limit method. RTSS is an optional alternative; other external limiter integrations can be added later. The UI exposes a Set FPS limit switch, a Method dropdown (supported NVIDIA/AMD/Intel driver or optional RTSS), and an FPS value. Use Method as the user-facing name for the provider/protocol. With the switch off, this scene leaves frame-limit settings unchanged, matching optional audio volume behavior. Switching this control off does not disable an existing driver/RTSS limit.

When enabling the action, prefer a supported driver method for the selected GPU; show unsupported/unavailable options clearly and never silently fall back to RTSS. With multiple compatible GPUs, require an unambiguous target. Validate numeric input against the selected method's reported range and step. Choosing a method or editing an FPS value configures the scene only; applying executes the change. Driver support is an accepted product direction, but each vendor integration still requires implementation and validation; it is not a promise of support on every GPU/driver.

Candidate direct integrations: NVIDIA NVAPI driver settings, AMD ADLX Frame Rate Target Control, and Intel IGCL frame-limit control. Validate the exact setting (including NVIDIA modern Max Frame Rate versus legacy limiter settings), supported GPU/driver combinations, FPS range/step, privileges, global versus per-application scope, and whether running games require restarting. Query support instead of assuming vendor-wide availability.

Read/set RTSS directly without rtss-cli or frltoggle. Let the user select a supported provider; do not silently apply a different limiter or alter competing limits. Save and restore the chosen provider's actual previous state, including enabled/inherited state where applicable. Reading back a setting confirms configuration, not actual frame pacing inside a game.

Research references: [NVIDIA driver settings](https://docs.nvidia.com/nvapi/group__drsapi.html), [AMD ADLX FRTC](https://gpuopen.com/manuals/adlx/adlx-sdk-references/adlx-interfaces/3d-graphics/iadlx3dframeratetargetcontrol/), [Intel IGCL](https://intel.github.io/drivers.gpu.control-library/Control/api.html).

### Ordered final actions

Allow optional ordered executable launches and URI dispatch, including Steam Big Picture. Executable actions support separate arguments and an optional working directory. Basic actions must not require batch files or a shell. Users can add, edit, remove, reorder, and enable/disable actions.

Run final actions after configuration. Define their gating after a failed required step and whether executable actions can wait for exit before implementation. URI dispatch cannot generally prove application readiness or completion. Arbitrary scripting is not an agreed initial requirement.

## 5. Execution and recovery

Prevent overlapping applications from the GUI and CLI, including during recovery. Report per-step progress and distinguish success, failure, partial application, and recovery results. Never report partial application as full success.

Failure behavior is user-selectable:

1. **Revert to previous settings** — the default.
2. **Stop and keep completed changes** — stop further scene actions and report partial application.

For recovery, snapshot the actual relevant pre-operation state rather than the last saved preset. Include each affected audio role and target device's volume. Track attempted changes, because a failed API operation may already have changed some state.

Restore reversible changes in an order that respects device dependencies. Verify restoration, continue independent recovery steps after a recovery error, and report incomplete recovery honestly. App launches and URI side effects may be irreversible. TV recovery requires known prior state and a reachable device.

Display Apply uses an agreed native Keep/Revert confirmation countdown with automatic restoration; the initial timeout is 15 seconds. Per-scene and per-command failure-policy overrides are also undecided.

## 6. GUI, CLI, and notifications

- Provide a proper WinUI 3 desktop window for managing presets and devices.
- Keep the window responsive while discovering devices, applying settings, waiting for devices, verifying results, and reverting changes. Perform blocking work outside the UI thread and dispatch progress/results back safely.
- Serialize setting changes and retain a stable operation snapshot; a background worker must not introduce concurrent preset mutations.
- Share application behavior and saved presets between GUI and CLI.
- CLI application works with the GUI closed and does not initialize a window.
- Provide preset listing, status, monitor-layout capture, application, help, and meaningful error/exit reporting. Exact syntax remains proposed.
- Define how simultaneous GUI/CLI requests are handled before implementation.
- Use native Windows notifications rather than custom notification popups.
- Notifications should identify the scene and outcome and allow opening the relevant result details.
- Respect Windows notification preferences. Preserve local results and CLI output even when notifications are suppressed.
- Failure/success notification defaults and an explicit quiet option still need confirmation.

## 7. Implementation sequence

This sequence is a planning proposal, not authorization to start development.

1. Establish the Rust/WinUI 3 shell and shared model; enumerate displays/audio, capture monitor layouts, and configure audio presets through dropdowns without changing settings.
2. Save, review, and apply independent presets with state verification and recovery.
3. Compose scenes and coordinate TV/display/audio readiness, frame limiting, and final actions.
4. Validate notification activation, headless CLI behavior, packaging, and real hardware workflows.
5. Add guided creation and other later features according to agreed release scope.

System tray mode and starting with Windows are explicitly deferred until after basic functionality. CRU and SpotBye are excluded.

## 8. Before 1.0

Validate display/audio-only isolation, actual desk and TV transitions, fractional refresh/rotation, remembered device reappearance, renamed/relinked endpoints, missing devices, timeouts, partial failures, both failure policies, incomplete recovery, RTSS, ordered launches/URIs, notification suppression, and CLI use with the GUI open and closed.

Verify UI responsiveness during deliberately slow device operations, timeouts, and recovery, and verify duplicate Apply requests cannot overlap.

Measure cold startup, idle CPU, memory, and CLI startup on representative release builds. No performance budgets or comparative framework benchmarks have been agreed yet.

Set the supported Windows baseline, distribution/update model, app/WinUI 3 licensing, and release-blocking feature scope. Validate the packaged app without development tools installed and document working setup/build/test instructions and limitations.


## Accepted initial audio navigation

Start on Audio presets with Add preset and inline Apply/Edit/Delete actions. Add/Edit opens a separate editor page with a name, output dropdown, optional volume, failure policy, and Save/Cancel. Saving must not apply settings. Store named presets between launches and allow applying one through director audio apply with its name. Delete confirms removal and leaves current audio unchanged. Scenes and display navigation are added when those features are implemented.

## Accepted compact workspace

Use an overarching app shell with Audio presets as the working view. Presets use compact selectable rows, a plus control to add, and right-aligned SVG actions: green checkmark to apply, pencil to edit, red cross to delete. Hover, selection, and keyboard focus reveal actions; narrow windows keep them visible. All icon controls need accessible names and keyboard activation. Selection must not apply audio, deletion still requires confirmation, and future sections must be visibly unavailable until implemented. See [interface design](DESIGN.md).

## Accepted editor and convenience additions

These additions were accepted after reviewing Display Profile Manager; they are backlog scope, not claims of implementation or an immediate instruction to implement every item.

- Add a visual monitor arrangement preview/map and a distinct Identify action that labels physical screens. Provide direct arrangement editing when the custom editor is implemented.
- Use native per-monitor editor cards with supported-mode resolution/refresh dropdowns, enabled state, exclusive primary-source selection, rotation, and explicit Extend/Clone group controls. Collapse inactive/unselected cards when useful; keep audio presets independent.
- Every rotation choice pairs a rotated display SVG with a plain-language orientation label. Include an asymmetric top-edge/up marker so 0 versus 180 degrees and 90 versus 270 degrees remain visually distinct. Degrees are secondary detail, never the sole explanation.
- Add per-monitor DPI scaling in later display work. Keep Windows UI/text scaling distinct from image/aspect-ratio scaling; hide advanced controls behind an Advanced section and offer Leave unchanged where supported.
- Add stable-identity desktop/Start Menu shortcuts for scenes, using headless application without requiring a resident process.
- Later, allow launching an app/game with a scene and restoring the actual previous setup on exit. Specify launcher/process tracking and abnormal-exit behavior before implementation.
- Add optional preset/scene descriptions and icons as later polish.

Wallpaper switching remains lower-priority optional scope. Tray, startup, global hotkeys, HDR/advanced display controls, and the guided creator keep their existing deferred/proposed status unless separately agreed. Basic scene composition, TV power/input orchestration and HDMI-audio readiness are now implemented (SCENES.md). See [Audio](AUDIO.md) for audio Stable ID resolution and relinking; initial manual TV setup/pairing is now implemented (TVS.md); accepting this editor design does not make them implemented.
### Display confirmation preference

Display presets allow users to disable the Keep/Revert prompt for trusted layouts. New presets default off; existing explicit choices and legacy confirmation-on behavior are preserved. Confirmation-off keeps verified changes automatically; failures still recover. CLI follows the preset with explicit --yes and --confirm overrides.

### TV integration references

Consult [integration references](INTEGRATION_REFERENCES.md) for the LG and Samsung projects supplied during planning. A native Samsung Tizen preview is now implemented with explicit direct-command testing and report export; physical validation remains pending (SAMSUNG.md). No external TV CLI is required by this decision.

### Custom display creation milestone

Create preset starts a new configurable draft from a fresh Windows layout. Keep Capture current layout as a separate quick snapshot action. Existing saved presets offer Customize layout. The first editor includes enabled/primary selection, supported resolution and exact refresh choices, visual orientation choices, numeric positions, a read-only arrangement preview, and draft-only Save/Cancel behavior. Detect invalid coordinates, an extra primary origin, and overlapping extended outputs. New clone grouping, drag placement, physical Identify, DPI/HDR/color controls remain later work.

### Cohesive direct display editing

The accepted UI now uses a consistent top-right + on audio and display pages, with matching toolbar and editor-action placement. The display pencil opens layout editing directly. Provide a boxed draggable arrangement map and screen hover details instead of raw pixel inputs. Disable the primary display's Enabled switch with a hover explanation. New GUI/captured/CLI display presets default to confirmation off, preserving existing explicit settings and legacy confirmation behavior. This supersedes the earlier default-on and separate Customize/read-only preview decisions.

### Current setup and display refinement

The user requested an alternative combined active-configuration view, explicitly excluding app appearance themes. Provide displays and audio output/volume on one page with independent Apply actions and no preset writes. Preserve staged edits across navigation and read actual state on clean entry/discard. Read audio outputs again after display changes.

Remove Refresh buttons from preset lists. Use compact left-aligned orientation rows with stand-only SVGs and automatic dismissal after selection. Expose supported resolution/refresh lists independently of stale saved adapter IDs; keep valid unspecified/progressive DXGI modes. HDR is now an explicit capability-gated toggle with verified application/recovery, superseding its deferred status; other advanced color/DPI/VRR controls remain deferred.

### TV setup and identity — first slice

Provide a TVs view to add/edit/remove LG webOS devices, explicitly pair with on-TV approval, and check authenticated connectivity without changing TV settings. Store protected local credentials, separate from presets. An optional physical Windows display-path association is device metadata; TV control remains independent of it and of presets. Preserve unavailable links and require explicit relinking when identity changes. IP is a connection address, not durable TV identity. TV-reported MAC learning is now implemented; stable TV identity and verified address migration remain follow-up work; avoid treating a certificate shared by LG models as unique identity. Discovery, power/input actions, and scene readiness remain subsequent slices.



### Guided TV setup and wake — implemented

New TV setup asks for name, IPv4/connection mode, on-TV pairing, automatically read inputs/network information, an explicit PC-input choice, wake addresses/manual fallback, optional physical Windows display, and final review/save. Keep completed choices across Back/navigation. Retain a successful pairing when an input read fails so it can be retried. Network-info refusal must allow manual entry or skipping wake. Cancel writes no new definition; previously granted TV permission must be revoked on the TV separately.

Remember Ethernet/Wi-Fi/manual MACs independently of IP/pairing. Reject invalid/zero/multicast/broadcast MACs, preserve remembered values on missing reads, and deduplicate wake targets. GUI/CLI wake uses direct bounded packet dispatch and reports that result without claiming verified TV power. Setup stores PC/display connections; scene-specific wake/input/power/timing preferences remain outside pairing.

### Basic TV controls — implemented

Provide a native control modal with fresh power/current-input status, manual refresh, one Turn off/Wake button, and target selection requiring an explicit Switch action. Honor the displayed power intent after fresh status verification; never reverse it because the state changed. A failed status query must not imply Off or authorize an off request. Missing MACs disable wake; explicit wake can run without an authenticated control session. Validate input IDs freshly, skip confirmed unchanged inputs, and distinguish accepted commands from confirmed readback/physical state.

Keep CLI and future scene power actions explicit and idempotent in intent. Modal interaction must not change TV connection metadata or saved presets. Perform I/O on workers, serialize operations, reject stale TV definitions, and handle closed dialogs without stale UI updates or unguarded window exit. Volume, full remote controls, and scene readiness remain future work.


TCL Android / Google TV preview: local code pairing, protected client identity, reported state, direct wake/sleep, confirmed-only HDMI inputs, report export, shared scenes and display preparation. No other TCL platform or physical model compatibility is assumed.

## Audio presets superseded

Audio presets were retired in D066. Requirements above that mention audio presets now apply to a scene's Sound step (saved sound) and to the live controls on Now and `director audio` (immediate changes). Saving or editing a Sound step still never changes a device.
