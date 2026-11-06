# Scenes

Scenes are saved in the Windows Local AppData Director folder as versioned scenes.json. They reference display presets and remembered TVs by ID, and hold their sound settings inline (the Sound step); display layouts and pairing credentials are not copied into scenes. Unique names are case insensitive. Stale writers and malformed/future-version files are rejected without overwriting them. Removal of a referenced preset or TV is blocked with the scene names; edit those scenes first. Missing references from external file edits remain visible and must be relinked explicitly.

## Native editor

Scenes uses the existing + button and rectangular preview-card vocabulary, with Apply/Edit/Delete in the same position as presets. The editor has optional TV preparation, Displays, Audio, and ordered After setup sections. Wide windows show an execution summary beside the form; narrow windows stack it. Manage preset buttons preserve the scene draft across navigation and reload available presets on return. Save only saves. Sound-only, display-only and action-only scenes are supported.

TV preparation selects one saved TV, wake-if-needed and optionally a reported input (initially its saved PC input). The Sound step can use a TV's linked output ("TV's sound"); it does not implicitly wake or switch the TV: TV preparation is a separate explicit choice. See the Sound step section below.

App actions use the native executable/folder pickers, separate arguments (one per line, spaces preserved) and an optional working directory. URI actions dispatch through the Windows registered handler. Add, edit, reorder and remove final actions. No scripts or shell command parsing are involved. Dispatch is reported honestly; app readiness, process exit and URI completion are not tracked. Disabled per-action drafts and per-action icons remain follow-up polish.

## Shared execution

GUI and CLI resolve fresh catalogs under the existing cross-process operation lock. They use the same runner. All scene I/O runs away from the UI thread. Progress and final results use structured rotating logs; raw TV credentials are excluded. The runner validates references and app paths before any device changes, then snapshots the actual relevant Windows state, not a previous preset.

Order is fixed: TV readiness/input → Displays → Sound readiness/apply → final apps/URIs. Omitted stages are skipped. TV and audio readiness waits are individually bounded by the scene timeout (default 30 seconds, 1–120 allowed); individual transport calls still have their own existing timeout and cancellation is checked between them. Wake dispatch is capped at five attempts, spaced at least five seconds apart, and only occurs when an awake state has not been reported. Input selection must be read back before display application continues; while waiting, the input is requested again (at most every 1.5 seconds) if the TV reports another one. Once it is read back, TV input protection (Options, default 10 seconds, 0–60, 0 off) keeps the TV on that input on a background thread while the scene continues: it polls every 0.5 seconds and switches back when a TV box or console has taken the input over (another input or an app in front). Only TVs that report their current input (LG) get it. Any other TV power or input request, a new scene and a scene that stops and restores end the guard first; a CLI run waits for it before exiting. No display detection probe runs automatically: a scene uses a previously configured/verified display preset.

After displays, enumerate fresh audio devices until every selected output/communications output/input is active and resolves exactly by endpoint ID. A timeout never substitutes another device. Apply the shared audio request, including optional microphone and volume semantics. Display presets retain their saved Keep/Revert preference; the GUI uses a native 15-second dialog and CLI accepts an explicit override.

On failure/cancel, stop subsequent stages and final actions. Default recovery restores requested device levels while newly enabled endpoints are still available, then restores the Windows display snapshot and original render/capture role IDs. It verifies restoration and reports incomplete recovery if needed. TV preparation is included in Windows snapshots because waking a TV can itself change Windows topology/defaults; an unchanged display snapshot needs no restoration write. Audio role snapshots precede TV/display preparation; a newly appearing device’s volume can only be captured once it becomes available, immediately before its audio writes. Input roles are never read or written unless the Sound step picks a microphone. A display-only scene also snapshots render defaults for failed-display recovery. Display application retains its own safety recovery even when the scene keeps earlier completed stages. TV power/input and dispatched app/URI effects are explicitly not rolled back. No screen blanking is implemented or planned.

## Headless commands

- director scene list
- director scene check tv-gaming
- director scene apply tv-gaming [--yes | --confirm]

CLI names (the scene name in lowercase with dashes, as `director scene list` shows) or IDs are accepted; a quoted name such as "TV gaming" still works. Check validates saved references and executable paths only, without device mutation or live readiness claims. Apply does not start the GUI or require Director to remain resident. --yes skips display confirmation; --confirm requires it. Exit code is zero only on complete success.

## Validation and remaining work

Model/runner tests cover stable reference resolution, direct TV audio, stage order, early failure gating, cancellation, both recovery policies, stale/corrupt storage and legacy TV compatibility. Native fixture tests cover creation/editing/deletion, action reordering, draft navigation and remembered links at wide/narrow sizes without real settings writes, audio/display mutations, TV commands or launches. Physical end-to-end HDMI arrival and restoration still need a hardware run. See [Audio](AUDIO.md) for the shared remembered-device inventory and relinking. Automatic TV-to-audio correlation, notifications, app-exit tracking and import/export are outside this slice.

## FPS stage

A scene may optionally set a global NVIDIA/RTSS FPS limit. Order is TV → displays → sound → FPS → final launches. Unsupported providers fail preflight before device changes; default failure/cancel recovery includes the selected FPS configuration. FPS-only scenes are supported. Off means unchanged; Disable global limit is an explicit operation. See FPS.md.

## Before, After and When leaving — version 2

Scenes are stored as `version: 2`. Version 1 files still load; their per-scene `keep_on_failure` and `wait_seconds` are read and dropped, and are no longer written. Older Director builds refuse a version 2 file instead of rewriting it.

Each scene has three action lists. **Before** runs first, after the settings snapshot and before any device change. **After** (the old final actions) runs once every device step has succeeded. **When leaving** runs when a different scene is applied, from the window or the CLI, or when the active scene is ended. Order is Before → TV → Displays → Sound → FPS → After.

Actions are Run app, Open link (URI) and Close app. An app can **wait until it finishes**: it then fails the step on a non-zero exit code or when it runs longer than the device wait from Options, and the process is left running on timeout. New Before apps wait by default; After and leave apps don't. Close app asks every running copy of a program (by `name.exe` or full path) to close through its windows, waits up to 10 seconds, and ends it only when "End it if it doesn't close" is on. A program that isn't running counts as closed. Steam Big Picture is best closed with the link `steam://close/bigpicture`.

The active scene is stored in `active-scene.json` with the time Windows started, so it survives closing Director and is shared with the CLI, but is forgotten after a restart. A successful apply records the scene; a failed apply clears it. Leave actions never block the next scene: each failure is reported as a warning and the remaining actions still run. End runs the leave actions and forgets the active scene; it doesn't restore any settings. `director scene list` marks the active scene and `director scene end` ends it.

## Sound step — version 3

Scenes are stored as `version: 3`. The Sound step (`sound`) replaces the audio preset reference (`audio`) and the linked TV audio fields (`tv_audio`, `tv_audio_volume`). It holds:

- `output`: a device (`{"kind":"device","id","name"}`) or a TV's sound (`{"kind":"tv","id"}`, the output linked to that TV); absent leaves the default output as is.
- `volume` (0–100) and `mute` (true or false); absent leaves them as they are. With no output, they apply to whichever output is the default when the scene runs.
- `calls`: an optional separate Communications output.
- `mic`: an optional microphone with its own `volume` and `mute`; it takes all three input roles.

Version 1 and 2 files still load. A scene that referred to an audio preset gets that preset's output, volume, calls device and microphone, read once from `audio-presets.json`; a scene that used a TV's linked audio gets `output: tv` with its volume. A preset that can't be found leaves an empty Sound step, which the scene card reports so the scene can be edited. The next save writes version 3 and Director no longer reads or writes `audio-presets.json` otherwise.

The runner snapshots volume and mute for every endpoint the step changes (a device that appears late is snapshotted just before its writes) and restores them, with the default roles, on failure. A scene card shows the scene's main change at the top: its display layout, else a sound panel (output, volume bar or Muted, calls and mic), else its TV, FPS limit or actions.

## Turning the TV off when leaving

The Prepare TV step has **Turn off the TV when you leave this scene** (`off_when_leaving` on the scene's TV preparation). When it's on, a locked "Turn off C1" card sits under When leaving; its pencil opens Prepare TV and its bin turns the setting off. Removing the TV step clears it, and choosing another TV keeps it. Leaving runs the leave actions first and then turns the TV off through the same path as `director tv off`. A TV that doesn't answer is reported as a warning and never blocks the next scene.

**End** appears next to the Scenes page's **+** only while the active scene has leave actions or turns its TV off; its tooltip lists what will run. The active card keeps an "Active" label.

## Commands for shortcuts

The **>\_** button next to a scene's or preset's name opens "Run … from a shortcut" with two commands, each with a Copy button: by CLI name (`director scene apply console`, readable, but copy it again after a rename) and by ID (keeps working after a rename). A scene with leave work also shows `director scene end`. Display presets accept their ID in `director display apply`, as scenes do. The by-name command uses the CLI name, unquoted (`director scene apply console`). Setup puts `director` on PATH, so the commands don't include a full path.

## First scene

An empty Scenes page offers to save the current setup as the everyday scene (named "Default" unless changed). It reads the layout with the display capture, the console default output (else any active one) with its volume, and the global FPS limit from the FPS survey: a provider with a limit wins, RTSS wins when both have one, and a zero value saves "no global limit". Each part can be unticked; a part that couldn't be read is disabled. Saving holds the operation lock, reuses a display preset with an identical layout, otherwise adds "Default screens" (made unique), and puts the output and volume in the scene's Sound step. The display preset is saved before the scene, so a scene never points at a preset that wasn't saved. No device changes. The logic is `director::starter`.

## Global failure policy

"Keep changes that finished if a later step fails" and the device wait (1–120 seconds, default 30) are Options settings in `preferences.json`, shared by scenes and waited apps. Live sound changes (Now and `director audio`) change one thing at a time and always put it back when Windows does not confirm it.

## Step-flow editor

The editor is one fixed-width column of step cards in run order, joined by down arrows, with When leaving set apart below. A card shows its title and a one-line summary, with edit and remove buttons; remove resets that step. Editing opens the step's controls in a Done/Cancel dialog, and Cancel restores the step as it was when the dialog opened. Empty steps are dashed outlines with a hint and an Add button that open the same dialog. App actions show Choose app and the chosen app's name; arguments, working folder and the editable path are under Advanced options. The former right-hand summary panel and the per-scene "Readiness and recovery" section were removed. This supersedes the editor layout described under "Native editor" above.
