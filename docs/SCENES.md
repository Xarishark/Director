# Scenes

Scenes are saved in the Windows Local AppData Director folder as versioned scenes.json. They reference display/audio presets and remembered TVs by ID; configurations and pairing credentials are not copied into scenes. Unique names are case insensitive. Stale writers and malformed/future-version files are rejected without overwriting them. Removal of a referenced preset or TV is blocked with the scene names; edit those scenes first. Missing references from external file edits remain visible and must be relinked explicitly.

## Native editor

Scenes uses the existing + button and rectangular preview-card vocabulary, with Apply/Edit/Delete in the same position as presets. The editor has optional TV preparation, Displays, Audio, and ordered After setup sections. Wide windows show an execution summary beside the form; narrow windows stack it. Manage preset buttons preserve the scene draft across navigation and reload available presets on return. Save only saves. Audio-only, display-only and action-only scenes are supported.

TV preparation selects one saved TV, wake-if-needed and optionally a reported input (initially its saved PC input). Audio can select an existing preset, or a TV’s linked output directly. A direct TV output sets all three render defaults and leaves microphone, volume and mute untouched; use an audio preset for other settings. It does not implicitly wake or switch the TV: TV preparation is a separate explicit choice.

App actions use the native executable/folder pickers, separate arguments (one per line, spaces preserved) and an optional working directory. URI actions dispatch through the Windows registered handler. Add, edit, reorder and remove final actions. No scripts or shell command parsing are involved. Dispatch is reported honestly; app readiness, process exit and URI completion are not tracked. Disabled per-action drafts and per-action icons remain follow-up polish.

## Shared execution

GUI and CLI resolve fresh catalogs under the existing cross-process operation lock. They use the same runner. All scene I/O runs away from the UI thread. Progress and final results use structured rotating logs; raw TV credentials are excluded. The runner validates references and app paths before any device changes, then snapshots the actual relevant Windows state, not a previous preset.

Order is fixed: TV readiness/input → Displays → Audio readiness/apply → final apps/URIs. Omitted stages are skipped. TV and audio readiness waits are individually bounded by the scene timeout (default 30 seconds, 1–120 allowed); individual transport calls still have their own existing timeout and cancellation is checked between them. Wake dispatch is capped at five attempts, spaced at least five seconds apart, and only occurs when an awake state has not been reported. Input selection must be read back before display application continues. No display detection probe runs automatically: a scene uses a previously configured/verified display preset.

After displays, enumerate fresh audio devices until every selected output/communications output/input is active and resolves exactly by endpoint ID. A timeout never substitutes another device. Apply the shared audio request, including optional microphone and volume semantics. Display presets retain their saved Keep/Revert preference; the GUI uses a native 15-second dialog and CLI accepts an explicit override.

On failure/cancel, stop subsequent stages and final actions. Default recovery restores requested device levels while newly enabled endpoints are still available, then restores the Windows display snapshot and original render/capture role IDs. It verifies restoration and reports incomplete recovery if needed. TV preparation is included in Windows snapshots because waking a TV can itself change Windows topology/defaults; an unchanged display snapshot needs no restoration write. Audio role snapshots precede TV/display preparation; a newly appearing device’s volume can only be captured once it becomes available, immediately before its audio writes. Input roles are never read or written without an opted-in input preset. A display-only scene also snapshots render defaults for failed-display recovery. Display application retains its own safety recovery even when the scene keeps earlier completed stages. TV power/input and dispatched app/URI effects are explicitly not rolled back. No screen blanking is implemented or planned.

## Headless commands

- director scene list
- director scene check "TV gaming"
- director scene apply "TV gaming" [--yes | --confirm]

Names or IDs are accepted. Check validates saved references and executable paths only, without device mutation or live readiness claims. Apply does not start the GUI or require Director to remain resident. --yes skips display confirmation; --confirm requires it. Exit code is zero only on complete success.

## Validation and remaining work

Model/runner tests cover stable reference resolution, direct TV audio, stage order, early failure gating, cancellation, both recovery policies, stale/corrupt storage and legacy TV compatibility. Native fixture tests cover creation/editing/deletion, action reordering, draft navigation and remembered links at wide/narrow sizes without real settings writes, audio/display mutations, TV commands or launches. Physical end-to-end HDMI arrival and restoration still need a hardware run. See [Audio](AUDIO.md) for the shared remembered-device inventory and relinking. Automatic TV-to-audio correlation, notifications, app-exit tracking and import/export are outside this slice.

## FPS stage

A scene may optionally set a global NVIDIA/RTSS FPS limit. Order is TV → displays → audio → FPS → final launches. Unsupported providers fail preflight before device changes; default failure/cancel recovery includes the selected FPS configuration. FPS-only scenes are supported. Off means unchanged; Disable global limit is an explicit operation. See FPS.md.
